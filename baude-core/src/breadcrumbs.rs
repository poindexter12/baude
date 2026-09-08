//! Per-launch-folder session breadcrumbs.
//!
//! The TUI records which local sessions were actually used (opened, typed
//! into, created, activated) during runs launched from a given folder, so a
//! relaunch in that folder can scope the sidebar to that context instead of
//! the whole workspace. The store is a sibling of the durable state file —
//! `<config>/breadcrumbs-<workspace>.json` — and never touches the
//! schema-locked `state-<workspace>.json`.
//!
//! Rows are referenced by their NATURAL keys (checkout `observed_path`,
//! standalone `canonical_path`), never by numeric keys: numeric keys are only
//! meaningful inside one state file's counter history, while paths survive a
//! state rebuild. Entries whose path no longer exists in durable state are
//! dropped at flush time.
//!
//! Concurrency: several TUIs may run in one workspace at once, each from its
//! own folder. A flush is a read-merge-write under an advisory file lock — it
//! reloads the file and replaces only its own folder's entry, so parallel
//! sessions never clobber each other's breadcrumbs. The file is advisory
//! UI-scoping data, not lifecycle state: a missing or corrupt file degrades
//! to an empty context and is never allowed to fail a launch.

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::repository::{PersistedPath, RepositoryState};
use crate::workspace::Workspace;

pub const SCHEMA_VERSION: u32 = 1;
/// Base name; the on-disk file is `breadcrumbs-<workspace>.json`.
pub const FILE_BASE: &str = "breadcrumbs";
/// Coalescing window for tick-driven flushes.
const FLUSH_INTERVAL_MS: u64 = 2_000;

fn schema_version_default() -> u32 {
    SCHEMA_VERSION
}

/// Whole-file shape. No `deny_unknown_fields`: additive evolution is expected
/// and an older baude reading a newer file must not treat it as corrupt.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct BreadcrumbFile {
    #[serde(default = "schema_version_default")]
    pub schema_version: u32,
    /// Keyed by the canonical launch directory (lossy UTF-8).
    #[serde(default)]
    pub folders: BTreeMap<String, FolderEntry>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct FolderEntry {
    #[serde(default)]
    pub checkouts: Vec<Crumb>,
    #[serde(default)]
    pub standalones: Vec<Crumb>,
    #[serde(default)]
    pub last_selected: Option<LastSelected>,
    #[serde(default)]
    pub updated_ms: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Crumb {
    pub path: PersistedPath,
    #[serde(default)]
    pub last_used_ms: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LastSelected {
    Checkout { path: PersistedPath },
    Standalone { path: PersistedPath },
}

fn record(list: &mut Vec<Crumb>, path: &PersistedPath, now_ms: u64) {
    match list.iter_mut().find(|crumb| &crumb.path == path) {
        Some(crumb) => crumb.last_used_ms = now_ms,
        None => list.push(Crumb {
            path: path.clone(),
            last_used_ms: now_ms,
        }),
    }
}

impl FolderEntry {
    pub fn contains_checkout(&self, path: &PersistedPath) -> bool {
        self.checkouts.iter().any(|crumb| &crumb.path == path)
    }

    pub fn contains_standalone(&self, path: &PersistedPath) -> bool {
        self.standalones.iter().any(|crumb| &crumb.path == path)
    }
}

/// The TUI-side controller for one launch folder's breadcrumbs: an in-memory
/// working copy plus coalesced, merge-safe persistence. `root: None` is the
/// in-memory mode (tests, and any environment without a config dir): fully
/// functional recording and filtering, no file I/O.
pub struct FolderContext {
    root: Option<PathBuf>,
    file: String,
    folder_key: String,
    entry: FolderEntry,
    dirty: bool,
    last_flush_ms: u64,
    load_note: Option<String>,
}

/// Canonical folder key for a launch directory (lossy UTF-8 of the
/// canonicalized path).
pub fn folder_key(launch_dir: &Path) -> String {
    let canonical = launch_dir
        .canonicalize()
        .unwrap_or_else(|_| launch_dir.to_path_buf());
    canonical.to_string_lossy().into_owned()
}

impl FolderContext {
    pub fn load(
        root: Option<PathBuf>,
        workspace: &Workspace,
        launch_dir: &Path,
        now_ms: u64,
    ) -> Self {
        let file = workspace.state_file(FILE_BASE);
        let folder_key = folder_key(launch_dir);
        let (entry, load_note) = match &root {
            Some(root) => {
                let (loaded, corrupted) = load_file(&root.join(&file));
                let entry = loaded.folders.get(&folder_key).cloned().unwrap_or_default();
                let note = corrupted.then(|| {
                    "breadcrumbs file was unreadable — starting a fresh folder context".to_string()
                });
                (entry, note)
            }
            None => (FolderEntry::default(), None),
        };
        Self {
            root,
            file,
            folder_key,
            entry,
            dirty: false,
            last_flush_ms: now_ms,
            load_note,
        }
    }

    /// One-shot corrupt-file note for the status line.
    pub fn take_load_note(&mut self) -> Option<String> {
        self.load_note.take()
    }

    pub fn folder_key(&self) -> &str {
        &self.folder_key
    }

    pub fn entry(&self) -> &FolderEntry {
        &self.entry
    }

    /// True while nothing has been recorded for this folder — the fail-open
    /// state where filtering must not hide anything.
    pub fn is_unpopulated(&self) -> bool {
        self.entry.checkouts.is_empty() && self.entry.standalones.is_empty()
    }

    pub fn record_checkout(&mut self, path: &PersistedPath, now_ms: u64) {
        record(&mut self.entry.checkouts, path, now_ms);
        self.entry.last_selected = Some(LastSelected::Checkout { path: path.clone() });
        self.entry.updated_ms = now_ms;
        self.dirty = true;
    }

    pub fn record_standalone(&mut self, path: &PersistedPath, now_ms: u64) {
        record(&mut self.entry.standalones, path, now_ms);
        self.entry.last_selected = Some(LastSelected::Standalone { path: path.clone() });
        self.entry.updated_ms = now_ms;
        self.dirty = true;
    }

    /// Coalesced tick flush: writes at most once per [`FLUSH_INTERVAL_MS`].
    pub fn maybe_flush(&mut self, state: &RepositoryState, now_ms: u64) {
        if !self.dirty || now_ms.saturating_sub(self.last_flush_ms) < FLUSH_INTERVAL_MS {
            return;
        }
        self.flush(state, now_ms);
    }

    /// Prune entries that no longer resolve in durable state, then merge this
    /// folder's entry back into the shared file. Best-effort: an I/O failure
    /// keeps the entry dirty for a later retry and never surfaces as an error.
    pub fn flush(&mut self, state: &RepositoryState, now_ms: u64) {
        if !self.dirty {
            return;
        }
        self.entry.checkouts.retain(|crumb| {
            state
                .checkouts
                .iter()
                .any(|c| c.observed_path == crumb.path)
        });
        self.entry.standalones.retain(|crumb| {
            state
                .standalone_sessions
                .iter()
                .any(|s| s.canonical_path == crumb.path)
        });
        self.last_flush_ms = now_ms;
        let Some(root) = &self.root else {
            self.dirty = false;
            return;
        };
        if save_folder(root, &self.file, &self.folder_key, &self.entry).is_ok() {
            self.dirty = false;
        }
    }
}

/// Load the shared file; a missing file is an empty store, an unreadable or
/// unparsable one is an empty store plus a `corrupted` flag.
fn load_file(path: &Path) -> (BreadcrumbFile, bool) {
    match std::fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str(&text) {
            Ok(file) => (file, false),
            Err(_) => (BreadcrumbFile::default(), true),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            (BreadcrumbFile::default(), false)
        }
        Err(_) => (BreadcrumbFile::default(), true),
    }
}

/// Read-merge-write one folder's entry under an advisory lock: reload the
/// file, replace only `folder_key`, write atomically (temp + rename + fsync).
fn save_folder(
    root: &Path,
    file: &str,
    folder_key: &str,
    entry: &FolderEntry,
) -> std::io::Result<()> {
    std::fs::create_dir_all(root)?;
    let destination = root.join(file);
    let lock_path = root.join(format!(".{file}.lock"));
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)?;
    lock.lock()?;
    let result = (|| {
        let (mut merged, _) = load_file(&destination);
        merged.schema_version = SCHEMA_VERSION;
        merged.folders.insert(folder_key.to_string(), entry.clone());
        let bytes =
            serde_json::to_vec_pretty(&merged).map_err(|e| std::io::Error::other(e.to_string()))?;
        let temporary = root.join(format!(".{file}.tmp-{}", std::process::id()));
        let mut output = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temporary)?;
        output.write_all(&bytes)?;
        output.sync_all()?;
        drop(output);
        std::fs::rename(&temporary, &destination)?;
        std::fs::File::open(root)?.sync_all()?;
        Ok(())
    })();
    let _ = lock.unlock();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::{
        CheckoutLifecycle, CheckoutRole, RetainedSessionState, SavedCheckout, SavedRepository,
        SavedStandaloneSession,
    };
    use crate::repository::{
        RepositoryHealth, RetainedStandaloneSessionState, StandaloneLifecycle,
    };

    fn workspace() -> Workspace {
        crate::workspace::resolve(
            Some("crumbtest"),
            None,
            &crate::persist::Config::default(),
            |_| {},
        )
    }

    fn path(value: &str) -> PersistedPath {
        PersistedPath::from_path(Path::new(value))
    }

    /// Fresh scratch dir per test, following the repo's temp-dir convention.
    fn scratch(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("baude-breadcrumbs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn state_with(checkout: &str, standalone: &str) -> RepositoryState {
        let mut state = RepositoryState::default();
        let repository = state.allocate_repository_key().unwrap();
        let repository_order = state.allocate_first_seen_order().unwrap();
        state.repositories.push(SavedRepository {
            key: repository,
            observed_common_dir: path(&format!("{checkout}/.git")),
            observed_main_worktree: path(checkout),
            first_seen_order: repository_order,
            health: RepositoryHealth::Available,
        });
        let key = state.allocate_checkout_key().unwrap();
        let order = state.allocate_first_seen_order().unwrap();
        state.checkouts.push(SavedCheckout::new(
            key,
            repository,
            CheckoutRole::Main,
            false,
            path(checkout),
            Some("main".into()),
            order,
            CheckoutLifecycle::Inactive,
            RetainedSessionState {
                name: "main".into(),
                cwd: path(checkout),
                repo_root: path(checkout),
                branch: Some("main".into()),
                is_worktree: false,
                shell_open: false,
                archived: false,
                archived_by_user: false,
                resume_id: None,
            },
        ));
        let standalone_key = state.allocate_standalone_key().unwrap();
        let standalone_order = state.allocate_first_seen_order().unwrap();
        state.standalone_sessions.push(SavedStandaloneSession::new(
            standalone_key,
            path(standalone),
            standalone_order,
            StandaloneLifecycle::Inactive,
            None,
            RetainedStandaloneSessionState {
                name: "notes".into(),
                shell_open: false,
                archived: false,
                archived_by_user: false,
                resume_id: None,
                ever_launched: false,
            },
        ));
        state
    }

    #[test]
    fn round_trips_one_folder_and_preserves_others() {
        let dir = scratch("roundtrip");
        let ws = workspace();
        let state = state_with("/tmp/crumb/repo", "/tmp/crumb/notes");

        // Another folder's entry, written first.
        let mut other = FolderContext::load(
            Some(dir.clone()),
            &ws,
            Path::new("/tmp/crumb/other-folder"),
            10,
        );
        other.record_checkout(&path("/tmp/crumb/repo"), 10);
        other.flush(&state, 10);

        let mut mine = FolderContext::load(
            Some(dir.clone()),
            &ws,
            Path::new("/tmp/crumb/my-folder"),
            20,
        );
        assert!(mine.is_unpopulated());
        mine.record_standalone(&path("/tmp/crumb/notes"), 20);
        mine.record_checkout(&path("/tmp/crumb/repo"), 21);
        mine.flush(&state, 21);

        // Reload both folders from disk: both survive one shared file.
        let (on_disk, corrupted) = load_file(&dir.join(ws.state_file(FILE_BASE)));
        assert!(!corrupted);
        assert_eq!(on_disk.schema_version, SCHEMA_VERSION);
        assert_eq!(on_disk.folders.len(), 2);
        let mine_reloaded = FolderContext::load(
            Some(dir.clone()),
            &ws,
            Path::new("/tmp/crumb/my-folder"),
            30,
        );
        assert!(mine_reloaded
            .entry()
            .contains_checkout(&path("/tmp/crumb/repo")));
        assert!(mine_reloaded
            .entry()
            .contains_standalone(&path("/tmp/crumb/notes")));
        assert_eq!(
            mine_reloaded.entry().last_selected,
            Some(LastSelected::Checkout {
                path: path("/tmp/crumb/repo")
            })
        );
        let other_reloaded = FolderContext::load(
            Some(dir.clone()),
            &ws,
            Path::new("/tmp/crumb/other-folder"),
            30,
        );
        assert!(other_reloaded
            .entry()
            .contains_checkout(&path("/tmp/crumb/repo")));
    }

    #[test]
    fn corrupted_file_degrades_to_empty_context_with_note() {
        let dir = scratch("corrupt");
        let ws = workspace();
        std::fs::write(dir.join(ws.state_file(FILE_BASE)), b"{not json").unwrap();
        let mut context =
            FolderContext::load(Some(dir.clone()), &ws, Path::new("/tmp/crumb/folder"), 5);
        assert!(context.is_unpopulated());
        assert!(context.take_load_note().is_some());
        assert!(context.take_load_note().is_none());

        // A flush replaces the corrupt file with a valid one.
        let state = state_with("/tmp/crumb/repo", "/tmp/crumb/notes");
        context.record_checkout(&path("/tmp/crumb/repo"), 6);
        context.flush(&state, 6);
        let (reloaded, corrupted) = load_file(&dir.join(ws.state_file(FILE_BASE)));
        assert!(!corrupted);
        assert_eq!(reloaded.folders.len(), 1);
    }

    #[test]
    fn flush_prunes_entries_missing_from_durable_state_and_coalesces() {
        let dir = scratch("prune");
        let ws = workspace();
        let state = state_with("/tmp/crumb/repo", "/tmp/crumb/notes");
        let mut context =
            FolderContext::load(Some(dir.clone()), &ws, Path::new("/tmp/crumb/folder"), 0);
        context.record_checkout(&path("/tmp/crumb/repo"), 1);
        context.record_checkout(&path("/tmp/crumb/removed-worktree"), 2);
        context.record_standalone(&path("/tmp/crumb/gone"), 3);

        // Inside the coalescing window nothing is written.
        context.maybe_flush(&state, 1_000);
        assert!(!dir.join(ws.state_file(FILE_BASE)).exists());

        context.maybe_flush(&state, 3_000);
        let reloaded = FolderContext::load(
            Some(dir.clone()),
            &ws,
            Path::new("/tmp/crumb/folder"),
            4_000,
        );
        assert!(reloaded.entry().contains_checkout(&path("/tmp/crumb/repo")));
        assert!(!reloaded
            .entry()
            .contains_checkout(&path("/tmp/crumb/removed-worktree")));
        assert!(!reloaded
            .entry()
            .contains_standalone(&path("/tmp/crumb/gone")));
    }

    #[test]
    fn in_memory_mode_records_without_touching_disk() {
        let ws = workspace();
        let state = state_with("/tmp/crumb/repo", "/tmp/crumb/notes");
        let mut context = FolderContext::load(None, &ws, Path::new("/tmp/crumb/folder"), 0);
        context.record_checkout(&path("/tmp/crumb/repo"), 1);
        assert!(context.entry().contains_checkout(&path("/tmp/crumb/repo")));
        context.flush(&state, 5_000);
        assert!(!context.is_unpopulated());
    }
}
