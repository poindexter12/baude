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
use std::path::Path;

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
    pub notes: Vec<String>,
}

/// Decide the launch hint for one TUI start. Consults the store only when the
/// feature is enabled AND neither env var already names the workspace — an
/// explicit invocation must not even read remembered history. `root: None` is
/// the in-memory mode (tests): no file I/O at all.
pub fn plan_launch(
    enabled: bool,
    ws_env: Option<&str>,
    backend_env: Option<&str>,
    root: Option<&Path>,
    launch_dir: &Path,
) -> LaunchPlan {
    if !enabled || ws_env.is_some() || backend_env.is_some() {
        return LaunchPlan::default();
    }
    let Some(root) = root else {
        return LaunchPlan::default();
    };
    let (file, corrupted) = load_json::<FolderWorkspaceFile>(&root.join(FILE_NAME));
    let mut plan = LaunchPlan {
        hint: file
            .folders
            .get(&folder_key(launch_dir))
            .map(|entry| entry.workspace.clone()),
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
        let dir = scratch("gates");
        record(Some(&dir), Path::new("/mem/api"), "opencode", 5);

        // Folder without an entry.
        let plan = plan_launch(true, None, None, Some(&dir), Path::new("/mem/new"));
        assert_eq!(plan, LaunchPlan::default());
        // Kill switch off.
        let plan = plan_launch(false, None, None, Some(&dir), Path::new("/mem/api"));
        assert_eq!(plan, LaunchPlan::default());
        // Either env var set: explicit choice, memory not even read.
        let plan = plan_launch(true, Some("work"), None, Some(&dir), Path::new("/mem/api"));
        assert_eq!(plan, LaunchPlan::default());
        let plan = plan_launch(
            true,
            None,
            Some("claude"),
            Some(&dir),
            Path::new("/mem/api"),
        );
        assert_eq!(plan, LaunchPlan::default());
        // In-memory mode: no store to read, and recording is a no-op.
        record(None, Path::new("/mem/api"), "oss", 6);
        let plan = plan_launch(true, None, None, None, Path::new("/mem/api"));
        assert_eq!(plan, LaunchPlan::default());
    }

    #[test]
    fn corrupt_file_degrades_to_no_memory_with_one_note_and_heals_on_record() {
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
}
