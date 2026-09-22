//! Per-launch-folder workspace memory.
//!
//! The folder-context breadcrumbs ([`crate::breadcrumbs`]) remember WHICH
//! sessions a folder used; this store remembers WHICH WORKSPACE (and thereby
//! backend, daemon, and state namespace) the folder was last launched in, so
//! a plain `baude` in that folder comes back up where the user left it.
//!
//! Unlike the breadcrumbs file this store cannot be workspace-suffixed — it
//! is read BEFORE the workspace is known — so it is one workspace-agnostic
//! file, `<config>/folder-workspaces.json`, keyed by canonical launch dir.
//!
//! Precedence is decided in [`crate::workspace::resolve_with_hint`]: an
//! explicit `BAUDE_WORKSPACE`/`BAUDE_BACKEND` always wins (and suppresses the
//! hint), the hint outranks the config defaults, and with no hint resolution
//! is unchanged. [`plan_launch`] additionally never reads the store when the
//! shared `folder_context` kill switch is off or an env var already decides.
//!
//! Same durability posture as breadcrumbs: advisory UI memory, not lifecycle
//! state. Writes are read-merge-write under the shared advisory-lock helper
//! so parallel TUIs launched from different folders never clobber each other;
//! a missing or corrupt file degrades to "no memory" and never fails a
//! launch. Entries are never pruned automatically — a folder on a detached
//! volume is legitimate history.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::breadcrumbs::{folder_key, load_json, locked_merge_write};
use crate::persist::Config;

pub const SCHEMA_VERSION: u32 = 1;
/// Workspace-agnostic by necessity: read before the workspace exists.
pub const FILE_NAME: &str = "folder-workspaces.json";

fn schema_version_default() -> u32 {
    SCHEMA_VERSION
}

/// Whole-file shape. No `deny_unknown_fields`: additive evolution is expected
/// and an older baude reading a newer file must not treat it as corrupt.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct FolderWorkspaceFile {
    #[serde(default = "schema_version_default")]
    pub schema_version: u32,
    /// Keyed by the canonical launch directory (lossy UTF-8).
    #[serde(default)]
    pub folders: BTreeMap<String, FolderWorkspaceEntry>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct FolderWorkspaceEntry {
    pub workspace: String,
    #[serde(default)]
    pub updated_ms: u64,
}

/// What `main` feeds into [`crate::workspace::initialize`] plus any one-shot
/// status notes for the TUI.
#[derive(Debug, Default, PartialEq)]
pub struct LaunchPlan {
    pub hint: Option<String>,
    pub repo_root: Option<PathBuf>,
    pub notes: Vec<String>,
}

/// Find a recorded folder binding by walking up the directory tree from `launch_dir`.
///
/// Walks upward checking for recorded bindings at each ancestor, stopping at the
/// home directory boundary (returns `None` when home is reached) or filesystem root.
/// Returns the first matching workspace name or `None`.
///
/// This is used to implement stable folder memory: the same repository returns the
/// same workspace regardless of which subfolder it's launched from.
///
/// # Canonicalization
///
/// **Important:** `launch_dir` must be canonicalized by the caller for correct home boundary
/// checking via `std::fs::canonicalize`. The `home` path is canonicalized internally to ensure
/// consistent comparison, even when reached through symlinks. On systems where the home
/// directory is a symlink (e.g., macOS with `/var` → `/private/var`), this ensures the
/// walk stops at the correct boundary and does not escape to parent directories.
pub fn find_binding(root: &Path, launch_dir: &Path, home: &Path) -> Option<String> {
    let (file, _) = load_json::<FolderWorkspaceFile>(&root.join(FILE_NAME));

    // Canonicalize home for consistent comparison with canonicalized launch_dir.
    // If canonicalization fails, fall back to non-canonical comparison.
    let canonical_home = match home.canonicalize() {
        Ok(ch) => ch,
        Err(_) => {
            // Home doesn't exist; use it as-is for comparison (walk will likely not match anyway)
            home.to_path_buf()
        }
    };

    let mut current = launch_dir.to_path_buf();
    loop {
        let key = folder_key(&current);
        if let Some(entry) = file.folders.get(&key) {
            return Some(entry.workspace.clone());
        }

        // Stop if we've reached home (now with consistent canonicalization).
        if current == canonical_home {
            return None;
        }

        // Stop if we've reached the root.
        if !current.pop() {
            return None;
        }
    }
}

/// Decide the launch hint for one TUI start. Consults the store only when the
/// feature is enabled AND neither env var already names the workspace — an
/// explicit invocation must not even read remembered history. `root: None` is
/// the in-memory mode (tests): no file I/O at all.
///
/// Discovers the repository root (if inside a git repository) and performs an
/// ancestor walk to find recorded folder bindings, stopping at the home directory.
/// Per the precedence reorder (D-02), BAUDE_BACKEND no longer suppresses the
/// ancestor walk or derivation — it only affects fallback resolution.
pub fn plan_launch(
    enabled: bool,
    ws_env: Option<&str>,
    _backend_env: Option<&str>,
    root: Option<&Path>,
    launch_dir: &Path,
) -> LaunchPlan {
    // Discover repository root via git (succeeds only if inside a repo).
    let repo_root = crate::git::repo_root(launch_dir);

    if !enabled || ws_env.is_some() {
        return LaunchPlan {
            repo_root,
            ..Default::default()
        };
    }
    let Some(root) = root else {
        return LaunchPlan {
            repo_root,
            ..Default::default()
        };
    };

    // Perform ancestor walk to find a recorded binding.
    let home = crate::persist::home_dir();
    let hint = find_binding(root, launch_dir, &home);

    // Check if the folder-workspaces file is corrupted (for diagnostics).
    let (_, corrupted) = load_json::<FolderWorkspaceFile>(&root.join(FILE_NAME));

    let mut plan = LaunchPlan {
        hint,
        repo_root,
        notes: Vec::new(),
    };
    if corrupted {
        plan.notes.push(
            "folder workspace memory was unreadable — using the configured workspace".to_string(),
        );
    }
    plan
}

/// The one-shot status note shown when folder memory changed the outcome:
/// the resolved workspace differs from what env + config alone would have
/// picked. `None` when the hint was absent, suppressed, or agreed with the
/// baseline.
pub fn applied_note(
    resolved: &str,
    ws_env: Option<&str>,
    backend_env: Option<&str>,
    config: &Config,
) -> Option<String> {
    let baseline = crate::workspace::resolve(ws_env, backend_env, config, |_| {});
    (resolved != baseline.name).then(|| format!("workspace {resolved} (folder history)"))
}

/// Record `launch_dir` → `workspace` after resolution, however the workspace
/// was chosen — an explicit `BAUDE_WORKSPACE=oss baude` run teaches the
/// folder. Best-effort: failures are ignored (advisory memory), and `root:
/// None` records nothing.
pub fn record(root: Option<&Path>, launch_dir: &Path, workspace: &str, now_ms: u64) {
    let Some(root) = root else { return };
    let key = folder_key(launch_dir);
    let entry = FolderWorkspaceEntry {
        workspace: workspace.to_string(),
        updated_ms: now_ms,
    };
    let _ = locked_merge_write(root, FILE_NAME, |merged: &mut FolderWorkspaceFile| {
        merged.schema_version = SCHEMA_VERSION;
        merged.folders.insert(key, entry);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestRedirect;
    use std::path::PathBuf;

    /// Fresh scratch dir per test, following the repo's temp-dir convention.
    fn scratch(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "baude-folder-workspace-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn records_and_recalls_one_folder_preserving_others() {
        let _redirect = TestRedirect::new(scratch("roundtrip-root"));
        let dir = scratch("roundtrip");
        record(Some(&dir), Path::new("/mem/api"), "opencode", 10);
        record(Some(&dir), Path::new("/mem/web"), "work", 20);
        // Re-record the first folder: only its entry changes.
        record(Some(&dir), Path::new("/mem/api"), "oss", 30);

        let plan = plan_launch(true, None, None, Some(&dir), Path::new("/mem/api"));
        assert_eq!(plan.hint.as_deref(), Some("oss"));
        assert!(plan.notes.is_empty());
        let plan = plan_launch(true, None, None, Some(&dir), Path::new("/mem/web"));
        assert_eq!(plan.hint.as_deref(), Some("work"));
        let (on_disk, corrupted) = load_json::<FolderWorkspaceFile>(&dir.join(FILE_NAME));
        assert!(!corrupted);
        assert_eq!(on_disk.schema_version, SCHEMA_VERSION);
        assert_eq!(on_disk.folders.len(), 2);
        assert_eq!(
            on_disk.folders[&folder_key(Path::new("/mem/api"))].updated_ms,
            30
        );
    }

    #[test]
    fn unknown_folder_disabled_switch_or_env_never_consult_memory() {
        let _redirect = TestRedirect::new(scratch("gates-root"));
        let dir = scratch("gates");
        record(Some(&dir), Path::new("/mem/api"), "opencode", 5);

        // Folder without an entry.
        let plan = plan_launch(true, None, None, Some(&dir), Path::new("/mem/new"));
        assert_eq!(plan, LaunchPlan::default());
        // Kill switch off.
        let plan = plan_launch(false, None, None, Some(&dir), Path::new("/mem/api"));
        assert_eq!(plan, LaunchPlan::default());
        // BAUDE_WORKSPACE env var suppresses memory read (explicit choice).
        let plan = plan_launch(true, Some("work"), None, Some(&dir), Path::new("/mem/api"));
        assert_eq!(plan, LaunchPlan::default());
        // BAUDE_BACKEND env var no longer suppresses memory read (per D-02 precedence reorder).
        let plan = plan_launch(
            true,
            None,
            Some("claude"),
            Some(&dir),
            Path::new("/mem/api"),
        );
        assert_eq!(plan.hint.as_deref(), Some("opencode"));
        // In-memory mode: no store to read, and recording is a no-op.
        record(None, Path::new("/mem/api"), "oss", 6);
        let plan = plan_launch(true, None, None, None, Path::new("/mem/api"));
        assert_eq!(plan, LaunchPlan::default());
    }

    #[test]
    fn corrupt_file_degrades_to_no_memory_with_one_note_and_heals_on_record() {
        let _redirect = TestRedirect::new(scratch("corrupt-root"));
        let dir = scratch("corrupt");
        std::fs::write(dir.join(FILE_NAME), b"{not json").unwrap();
        let plan = plan_launch(true, None, None, Some(&dir), Path::new("/mem/api"));
        assert_eq!(plan.hint, None);
        assert_eq!(plan.notes.len(), 1);
        assert!(plan.notes[0].contains("unreadable"));

        record(Some(&dir), Path::new("/mem/api"), "opencode", 7);
        let plan = plan_launch(true, None, None, Some(&dir), Path::new("/mem/api"));
        assert_eq!(plan.hint.as_deref(), Some("opencode"));
        assert!(plan.notes.is_empty());
    }

    #[test]
    fn applied_note_fires_only_when_memory_changed_the_outcome() {
        let config = Config {
            workspace: Some("work".into()),
            ..Config::default()
        };
        // Hint diverged from the config default → note, with the exact copy.
        assert_eq!(
            applied_note("opencode", None, None, &config).as_deref(),
            Some("workspace opencode (folder history)")
        );
        // Resolution matches what env+config would have picked → silent.
        assert_eq!(applied_note("work", None, None, &config), None);
        assert_eq!(applied_note("claude", None, None, &Config::default()), None);
        assert_eq!(applied_note("oss", Some("oss"), None, &config), None);
    }

    #[test]
    fn find_binding_at_parent_level_returns_parent_workspace() {
        let _redirect = TestRedirect::new(scratch("find-binding-parent-root"));
        let dir = scratch("find-binding-parent");
        let parent = Path::new("/mem");
        let home = Path::new("/home/user");

        record(Some(&dir), parent, "poindexter12", 10);

        let binding = find_binding(&dir, parent, home);
        assert_eq!(binding.as_deref(), Some("poindexter12"));
    }

    #[test]
    fn find_binding_at_grandparent_when_parent_has_no_binding() {
        let _redirect = TestRedirect::new(scratch("find-binding-grandparent-root"));
        let dir = scratch("find-binding-grandparent");
        let grandparent = Path::new("/mem");
        let _parent = Path::new("/mem/api");
        let launch_dir = Path::new("/mem/api/src");
        let home = Path::new("/home/user");

        record(Some(&dir), grandparent, "oss", 10);

        let binding = find_binding(&dir, launch_dir, home);
        assert_eq!(binding.as_deref(), Some("oss"));
    }

    #[test]
    fn find_binding_nearest_wins_when_both_parent_and_grandparent_have_bindings() {
        let _redirect = TestRedirect::new(scratch("find-binding-nearest-root"));
        let dir = scratch("find-binding-nearest");
        let grandparent = Path::new("/mem");
        let parent = Path::new("/mem/api");
        let launch_dir = Path::new("/mem/api/src");
        let home = Path::new("/home/user");

        record(Some(&dir), grandparent, "oss", 10);
        record(Some(&dir), parent, "work", 20);

        let binding = find_binding(&dir, launch_dir, home);
        assert_eq!(binding.as_deref(), Some("work"));
    }

    #[test]
    fn find_binding_stops_at_home_boundary() {
        let _redirect = TestRedirect::new(scratch("find-binding-home-root"));
        let dir = scratch("find-binding-home");
        let above_home = Path::new("/");
        let home = Path::new("/home/user");
        let launch_dir = home;

        record(Some(&dir), above_home, "should-not-find", 10);

        let binding = find_binding(&dir, launch_dir, home);
        assert_eq!(binding, None);
    }

    #[test]
    fn find_binding_returns_none_when_no_binding_exists() {
        let _redirect = TestRedirect::new(scratch("find-binding-none-root"));
        let dir = scratch("find-binding-none");
        let launch_dir = Path::new("/mem/api/src");
        let home = Path::new("/home/user");

        let binding = find_binding(&dir, launch_dir, home);
        assert_eq!(binding, None);
    }

    #[test]
    fn find_binding_at_home_with_binding_returns_some() {
        let _redirect = TestRedirect::new(scratch("find-binding-at-home-root"));
        let dir = scratch("find-binding-at-home");
        let home = Path::new("/home/user");

        record(Some(&dir), home, "bound-at-home", 10);

        let binding = find_binding(&dir, home, home);
        assert_eq!(binding.as_deref(), Some("bound-at-home"));
    }

    #[test]
    fn find_binding_at_home_without_binding_returns_none() {
        let _redirect = TestRedirect::new(scratch("find-binding-no-binding-root"));
        let dir = scratch("find-binding-no-binding");
        let home = Path::new("/home/user");

        let binding = find_binding(&dir, home, home);
        assert_eq!(binding, None);
    }

    #[test]
    fn plan_launch_with_baude_backend_and_binding_returns_hint() {
        let _redirect = TestRedirect::new(scratch("plan-launch-backend-root"));
        let dir = scratch("plan-launch-backend");

        let parent_dir = scratch("plan-launch-backend-parent");
        std::fs::create_dir_all(&parent_dir).unwrap();
        let launch_dir = parent_dir.join("subfolder");
        std::fs::create_dir_all(&launch_dir).unwrap();

        record(Some(&dir), &parent_dir, "bound-workspace", 10);

        // Per the plan: BAUDE_BACKEND does NOT suppress binding lookup.
        // The hint should be returned even when BAUDE_BACKEND is set.
        let plan = plan_launch(true, None, Some("opencode"), Some(&dir), &launch_dir);
        assert_eq!(plan.hint.as_deref(), Some("bound-workspace"));
    }

    #[test]
    fn plan_launch_with_baude_backend_and_no_binding_derives() {
        let _redirect = TestRedirect::new(scratch("plan-launch-derive-root"));
        let dir = scratch("plan-launch-derive");

        // Create a temporary git repository to test derivation.
        let repo_dir = scratch("plan-launch-derive-repo");
        std::fs::create_dir_all(&repo_dir).unwrap();
        let _ = std::process::Command::new("git")
            .args(["init"])
            .current_dir(&repo_dir)
            .output();

        // Per the plan: BAUDE_BACKEND does NOT suppress derivation.
        // When no binding exists, the repo_root should be returned for derivation.
        let plan = plan_launch(true, None, Some("opencode"), Some(&dir), &repo_dir);
        assert!(plan.repo_root.is_some());
    }

    #[test]
    fn find_binding_respects_symlinked_home_boundary() {
        // Test that the ancestor walk correctly stops at a home directory
        // reached through a symlink. This validates the fix for CR-01/WR-01:
        // on macOS and other systems with symlinked home dirs, the walk must
        // not escape the home boundary through the symlink.
        let fixture_root = scratch("find-binding-symlink-home-root");
        let config_dir = scratch("find-binding-symlink-home-config");

        // Create the directory structure:
        // - fixture/real-home/    <- the actual home directory
        // - fixture/home-link ->  <- a symlink to real-home
        let real_home = fixture_root.join("real-home");
        let home_link = fixture_root.join("home-link");
        std::fs::create_dir_all(&real_home).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real_home, &home_link).unwrap();
        #[cfg(not(unix))]
        {
            // Windows: use junction instead (or skip the test if unavailable).
            // For now, just copy the directory structure.
            std::fs::create_dir_all(&home_link).unwrap();
        }

        // Create a launch directory inside the real home, then a subdirectory.
        let launch_dir = real_home.join("projects").join("myrepo");
        std::fs::create_dir_all(&launch_dir).unwrap();

        // Record a binding at a parent directory ABOVE the real home.
        // This binding should NOT be found when walking from launch_dir
        // through the symlinked home boundary.
        let above_home = fixture_root.join("above");
        record(Some(&config_dir), &above_home, "should-not-find", 10);

        // Use TestRedirect to set the home to the symlink.
        let _redirect = TestRedirect::new(&fixture_root);

        // The critical path: canonicalize launch_dir and call find_binding
        // with the symlinked home path. The fix ensures that even though
        // home_link is a symlink, it will be canonicalized to real-home,
        // and the comparison will correctly stop the walk at the boundary.
        let canonical_launch = launch_dir
            .canonicalize()
            .unwrap_or_else(|_| launch_dir.clone());

        // Call find_binding with the symlinked home.
        let binding = find_binding(&config_dir, &canonical_launch, &home_link);

        // Should NOT find the binding above the home boundary, even though
        // home is reached via a symlink.
        assert_eq!(
            binding, None,
            "walk should stop at symlinked home boundary and not escape"
        );
    }
}
