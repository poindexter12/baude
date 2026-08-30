use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::repository::{
    CheckoutHealth, CheckoutRole, PersistedPath, RepositoryHealth, RepositoryState,
    RetainedSessionState, SavedCheckout, SavedRepository, UnavailableCause,
};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StateFile {
    pub schema_version: u32,
    pub state: RepositoryState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LoadOutcome {
    Missing,
    Legacy(StateFile),
    Current(StateFile),
}

#[derive(Debug)]
pub enum LoadError {
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    Malformed {
        path: PathBuf,
        cause: String,
    },
    UnsupportedVersion {
        path: PathBuf,
        version: u64,
    },
    InvalidState {
        path: PathBuf,
        cause: String,
    },
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read { path, source } => write!(f, "read {}: {source}", path.display()),
            Self::Malformed { path, cause } => {
                write!(f, "malformed state {}: {cause}", path.display())
            }
            Self::UnsupportedVersion { path, version } => {
                write!(
                    f,
                    "unsupported state schema version {version} in {}",
                    path.display()
                )
            }
            Self::InvalidState { path, cause } => {
                write!(f, "invalid state {}: {cause}", path.display())
            }
        }
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AtomicFailure {
    Write,
    Sync,
    Rename,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LegacyReconciliation {
    Available {
        common_dir: PersistedPath,
        main_worktree: PersistedPath,
        checkout_path: PersistedPath,
        checkout_role: CheckoutRole,
        managed_by_baude: bool,
    },
    Unavailable {
        repository_cause: UnavailableCause,
        checkout_cause: UnavailableCause,
    },
}

impl StateFile {
    pub fn new(state: RepositoryState) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            state,
        }
    }
}

/// Isolated-root seam used by persistence tests and explicit state owners.
pub fn save_current_at(root: &std::path::Path, file: &str, state: &StateFile) -> Result<()> {
    state.state.validate()?;
    let path = root.join(file);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(state)?)?;
    Ok(())
}

pub fn load_current_at(root: &std::path::Path, file: &str) -> Result<StateFile> {
    let state: StateFile = serde_json::from_slice(&std::fs::read(root.join(file))?)?;
    state.state.validate()?;
    Ok(state)
}

pub fn load_for_workspace_strict_at(
    _root: &std::path::Path,
    _base: &str,
    _ws: &crate::workspace::Workspace,
    _reconcile: impl FnMut(&SavedSession) -> LegacyReconciliation,
) -> std::result::Result<LoadOutcome, LoadError> {
    // RED: strict decoding is implemented in the GREEN commit.
    Ok(LoadOutcome::Missing)
}

#[doc(hidden)]
pub fn save_current_at_test(
    root: &std::path::Path,
    file: &str,
    state: &StateFile,
    failure: Option<AtomicFailure>,
    _first_temp: Option<PathBuf>,
) -> Result<()> {
    save_current_at(root, file, state)?;
    if let Some(failure) = failure {
        anyhow::bail!("injected {failure:?} failure");
    }
    Ok(())
}

/// Select and migrate exactly one workspace-owned source under an injected root.
pub fn migrate_for_workspace_at(
    root: &std::path::Path,
    base: &str,
    ws: &crate::workspace::Workspace,
    mut reconcile: impl FnMut(&SavedSession) -> LegacyReconciliation,
) -> Result<LoadOutcome> {
    let primary_file = ws.state_file(base);
    let primary_path = root.join(&primary_file);
    let source_file = if primary_path.exists() {
        primary_file.clone()
    } else if let Some(legacy) = ws.legacy_state_file(base) {
        if root.join(&legacy).exists() {
            legacy
        } else {
            return Ok(LoadOutcome::Missing);
        }
    } else {
        return Ok(LoadOutcome::Missing);
    };

    let bytes = std::fs::read(root.join(&source_file))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    if let Some(version) = value.get("schema_version") {
        let version = version
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("schema_version must be an unsigned integer"))?;
        if version != u64::from(SCHEMA_VERSION) {
            anyhow::bail!("unsupported state schema version {version}");
        }
        let current: StateFile = serde_json::from_value(value)?;
        current.state.validate()?;
        return Ok(LoadOutcome::Current(current));
    }

    let legacy: State = serde_json::from_value(value)?;
    let migrated = StateFile::new(migrate_legacy(legacy, &mut reconcile)?);
    save_current_at(root, &primary_file, &migrated)?;
    Ok(LoadOutcome::Legacy(migrated))
}

#[derive(Hash, Eq, PartialEq)]
enum LegacyRepositoryIdentity {
    Available(PersistedPath),
    Unavailable(usize),
}

fn migrate_legacy(
    legacy: State,
    reconcile: &mut impl FnMut(&SavedSession) -> LegacyReconciliation,
) -> Result<RepositoryState> {
    let mut state = RepositoryState::default();
    let mut repositories = HashMap::new();

    for (source_order, session) in legacy.sessions.into_iter().enumerate() {
        let (
            identity,
            common_dir,
            main_worktree,
            repository_health,
            checkout_path,
            checkout_role,
            managed_by_baude,
            checkout_health,
        ) = match reconcile(&session) {
            LegacyReconciliation::Available {
                common_dir,
                main_worktree,
                checkout_path,
                checkout_role,
                managed_by_baude,
            } => (
                LegacyRepositoryIdentity::Available(common_dir.clone()),
                common_dir,
                main_worktree,
                RepositoryHealth::Available,
                checkout_path,
                checkout_role,
                managed_by_baude,
                CheckoutHealth::Available,
            ),
            LegacyReconciliation::Unavailable {
                repository_cause,
                checkout_cause,
            } => (
                LegacyRepositoryIdentity::Unavailable(source_order),
                PersistedPath::from_path(&session.repo_root),
                PersistedPath::from_path(&session.repo_root),
                RepositoryHealth::Unavailable(repository_cause),
                PersistedPath::from_path(&session.cwd),
                CheckoutRole::Main,
                false,
                CheckoutHealth::Unavailable(checkout_cause),
            ),
        };

        let repository_key = if let Some(key) = repositories.get(&identity) {
            *key
        } else {
            let key = state.allocate_repository_key();
            let first_seen_order = state.allocate_first_seen_order();
            state.repositories.push(SavedRepository {
                key,
                observed_common_dir: common_dir,
                observed_main_worktree: main_worktree,
                first_seen_order,
                health: repository_health,
            });
            repositories.insert(identity, key);
            key
        };

        let checkout_key = state.allocate_checkout_key();
        let first_seen_order = state.allocate_first_seen_order();
        state.checkouts.push(SavedCheckout {
            key: checkout_key,
            repository_key,
            role: checkout_role,
            managed_by_baude,
            observed_path: checkout_path,
            observed_branch: session.branch.clone(),
            first_seen_order,
            active_intent: true,
            session: RetainedSessionState {
                name: session.name,
                cwd: PersistedPath::from_path(&session.cwd),
                repo_root: PersistedPath::from_path(&session.repo_root),
                branch: session.branch,
                is_worktree: session.is_worktree,
                shell_open: session.shell_open,
                archived: session.archived,
                archived_by_user: session.archived_by_user,
            },
            health: checkout_health,
        });
    }

    state.validate()?;
    Ok(state)
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub sessions: Vec<SavedSession>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SavedSession {
    pub name: String,
    pub cwd: PathBuf,
    pub repo_root: PathBuf,
    pub branch: Option<String>,
    pub is_worktree: bool,
    pub shell_open: bool,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub archived_by_user: bool,
}

fn config_base() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("baude")
}

fn state_path(file: &str) -> PathBuf {
    config_base().join(file)
}

/// User configuration, ~/.config/baude/config.json. All fields optional.
#[derive(Deserialize, Default)]
pub struct Config {
    /// Command run for each CLAUDE-backend session; BAUDE_CLAUDE_CMD
    /// overrides this. Applies ONLY when the active backend is claude — an
    /// opencode workspace ignores it (a configured `claude
    /// --dangerously-skip-permissions` must never become the opencode spawn
    /// command; claude rejects opencode's flags with "unknown option").
    /// Example: "claude --dangerously-skip-permissions"
    pub claude_cmd: Option<String>,
    /// Command run for each OPENCODE-backend session; BAUDE_OPENCODE_CMD
    /// overrides this. The opencode sibling of `claude_cmd`.
    pub opencode_cmd: Option<String>,
    /// Prefill for the new-session path prompt, e.g. "~/Code/github.com".
    /// Defaults to the directory baude was launched from.
    pub new_session_dir: Option<String>,
    /// Base directory for the `c` clone prompt's default destination,
    /// laid out as `<base>/<host>/<owner>/<repo>`. Defaults to "~/Code".
    pub clone_base_dir: Option<String>,
    /// Command used by the sidebar `e` key to open a session's folder.
    /// The session cwd is appended as an argument. BAUDE_EDITOR_CMD overrides
    /// this. Defaults to "code".
    pub editor_cmd: Option<String>,
    /// Base URL of a remote bauded daemon whose sessions appear in the
    /// sidebar, e.g. "http://bauded:8642". BAUDE_DAEMON_URL overrides.
    pub daemon_url: Option<String>,
    /// Minutes of idle waiting before a session auto-archives; 0 disables
    /// auto-archiving. BAUDED_AUTO_ARCHIVE_MIN overrides. Defaults to 30.
    pub auto_archive_minutes: Option<u64>,
    /// When true, baude auto-starts a local bauded on startup if one is not
    /// already running, and routes new-session creation through it so sessions
    /// survive TUI restarts. BAUDE_AUTO_DAEMON=1 overrides.
    #[serde(default)]
    pub auto_daemon: bool,
    /// Which AI-CLI backend to manage: "claude" (default) or "opencode".
    /// Global — every session in this baude/bauded process uses the same
    /// backend. BAUDE_BACKEND overrides. Unknown values fall back to claude.
    /// With workspaces, this is the fallback for workspaces that carry no
    /// explicit `backend` binding (see [`crate::workspace`]).
    pub backend: Option<String>,
    /// Default workspace to open; BAUDE_WORKSPACE overrides. Defaults to the
    /// backend name, so `claude`/`opencode` separate automatically.
    pub workspace: Option<String>,
    /// Named workspace declarations. Absent entries still resolve — a
    /// workspace is its state namespace first, config second.
    pub workspaces: Option<std::collections::HashMap<String, WorkspaceConfig>>,
    /// macOS desktop banners when a session needs attention (waiting /
    /// permission / finished / exited). Default true (macOS only);
    /// BAUDE_NOTIFY=0 overrides.
    pub desktop_notifications: Option<bool>,
}

/// One `workspaces.<name>` config entry. All fields optional.
#[derive(Deserialize, Clone, Default)]
pub struct WorkspaceConfig {
    /// Backend this workspace is BOUND to ("claude" | "opencode"). A binding
    /// beats BAUDE_BACKEND — the anti-cross-wiring guarantee.
    pub backend: Option<String>,
    /// Remote daemon for this workspace; beats the global `daemon_url`.
    pub daemon_url: Option<String>,
    /// Loopback port auto_daemon uses for this workspace. Implicit
    /// workspaces default (claude 8642, opencode 8643); custom ones need
    /// this set for auto_daemon to work.
    pub daemon_port: Option<u16>,
}

impl Config {
    /// Resolved auto-archive idle window in ms: BAUDED_AUTO_ARCHIVE_MIN env
    /// (minutes, 0 disables), then `auto_archive_minutes`, then 30 minutes.
    pub fn auto_archive_ms(&self) -> u64 {
        std::env::var("BAUDED_AUTO_ARCHIVE_MIN")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .or(self.auto_archive_minutes)
            .map(|min| min * 60_000)
            .unwrap_or(crate::session::AUTO_ARCHIVE_IDLE_MS)
    }
}

pub fn load_config() -> Config {
    std::fs::read_to_string(config_base().join("config.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn load() -> State {
    load_for_workspace("state", crate::workspace::active())
}

pub fn save(state: &State) -> Result<()> {
    save_for_workspace("state", crate::workspace::active(), state)
}

/// Load a workspace's session state (`<base>-<ws>.json`), falling back to
/// the legacy un-suffixed file for the default workspace so pre-workspace
/// session lists survive the upgrade. Saves never target the legacy name.
pub fn load_for_workspace(base: &str, ws: &crate::workspace::Workspace) -> State {
    let primary = ws.state_file(base);
    if state_path(&primary).exists() {
        return load_named(&primary);
    }
    match ws.legacy_state_file(base) {
        Some(legacy) => load_named(&legacy),
        None => State::default(),
    }
}

pub fn save_for_workspace(
    base: &str,
    ws: &crate::workspace::Workspace,
    state: &State,
) -> Result<()> {
    save_named(&ws.state_file(base), state)
}

/// Load session state from a specific file under the config dir. The TUI and
/// the daemon keep separate files so they never clobber each other's sessions.
pub fn load_named(file: &str) -> State {
    std::fs::read_to_string(state_path(file))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_named(file: &str, state: &State) -> Result<()> {
    let path = state_path(file);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(state)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::{
        CheckoutHealth, CheckoutRole, PersistedPath, RepositoryHealth, RetainedSessionState,
        SavedCheckout, SavedRepository, UnavailableCause,
    };

    fn isolated_root(label: &str) -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let root = std::env::temp_dir().join(format!(
            "baude-persist-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn current_fixture(prefix: &str) -> StateFile {
        let mut state = RepositoryState::default();
        let repository_key = state.allocate_repository_key();
        let checkout_key = state.allocate_checkout_key();
        let repository_order = state.allocate_first_seen_order();
        let checkout_order = state.allocate_first_seen_order();
        let main = PathBuf::from(format!("/{prefix}/repo"));
        let checkout_path = PathBuf::from(format!("/{prefix}/repo-default"));
        state.repositories.push(SavedRepository {
            key: repository_key,
            observed_common_dir: PersistedPath::from_path(&main.join(".git")),
            observed_main_worktree: PersistedPath::from_path(&main),
            first_seen_order: repository_order,
            health: RepositoryHealth::Unavailable(UnavailableCause::IdentityChanged),
        });
        state.checkouts.push(SavedCheckout {
            key: checkout_key,
            repository_key,
            role: CheckoutRole::PrimaryDefault,
            managed_by_baude: true,
            observed_path: PersistedPath::from_path(&checkout_path),
            observed_branch: Some("feature/retained".into()),
            first_seen_order: checkout_order,
            active_intent: true,
            session: RetainedSessionState {
                name: format!("{prefix}-session"),
                cwd: PersistedPath::from_path(&checkout_path),
                repo_root: PersistedPath::from_path(&main),
                branch: Some("feature/retained".into()),
                is_worktree: true,
                shell_open: true,
                archived: true,
                archived_by_user: true,
            },
            health: CheckoutHealth::Unavailable(UnavailableCause::Missing),
        });
        StateFile::new(state)
    }

    fn legacy_fixture(prefix: &str) -> State {
        State {
            sessions: vec![
                SavedSession {
                    name: format!("{prefix}-main"),
                    cwd: PathBuf::from(format!("/{prefix}/repo")),
                    repo_root: PathBuf::from(format!("/{prefix}/repo")),
                    branch: Some("develop".into()),
                    is_worktree: false,
                    shell_open: true,
                    archived: false,
                    archived_by_user: false,
                },
                SavedSession {
                    name: format!("{prefix}-child"),
                    cwd: PathBuf::from(format!("/{prefix}/child")),
                    repo_root: PathBuf::from(format!("/{prefix}/repo")),
                    branch: Some("feature/one".into()),
                    is_worktree: true,
                    shell_open: false,
                    archived: true,
                    archived_by_user: true,
                },
                SavedSession {
                    name: format!("{prefix}-missing"),
                    cwd: PathBuf::from(format!("/{prefix}/missing")),
                    repo_root: PathBuf::from(format!("/{prefix}/gone")),
                    branch: None,
                    is_worktree: true,
                    shell_open: true,
                    archived: false,
                    archived_by_user: true,
                },
            ],
        }
    }

    fn test_workspace(name: &str) -> crate::workspace::Workspace {
        crate::workspace::resolve(Some(name), None, &Config::default(), |message| {
            panic!("unexpected workspace warning: {message}")
        })
    }

    fn reconcile_legacy(session: &SavedSession) -> LegacyReconciliation {
        if session.name.ends_with("missing") {
            LegacyReconciliation::Unavailable {
                repository_cause: UnavailableCause::Missing,
                checkout_cause: UnavailableCause::Missing,
            }
        } else {
            LegacyReconciliation::Available {
                common_dir: PersistedPath::from_path(&session.repo_root.join(".git")),
                main_worktree: PersistedPath::from_path(&session.repo_root),
                checkout_path: PersistedPath::from_path(&session.cwd),
                checkout_role: if session.is_worktree {
                    CheckoutRole::ManagedBranch
                } else {
                    CheckoutRole::Main
                },
                managed_by_baude: false,
            }
        }
    }

    fn assert_legacy_migration(base: &str) {
        let root = isolated_root(base);
        let workspace = test_workspace("claude");
        let selected = legacy_fixture(base);
        let dormant = legacy_fixture("dormant");
        let primary_path = root.join(workspace.state_file(base));
        let fallback_path = root.join(workspace.legacy_state_file(base).unwrap());
        let selected_bytes = serde_json::to_vec_pretty(&selected).unwrap();
        std::fs::write(&fallback_path, &selected_bytes).unwrap();

        let outcome = migrate_for_workspace_at(&root, base, &workspace, reconcile_legacy).unwrap();
        let LoadOutcome::Legacy(migrated) = outcome else {
            panic!("expected migrated legacy state");
        };
        assert!(primary_path.exists());
        assert_eq!(std::fs::read(&fallback_path).unwrap(), selected_bytes);
        assert_eq!(
            migrated.state.repositories.len(),
            2,
            "shared identity groups sessions"
        );
        assert_eq!(
            migrated.state.checkouts.len(),
            3,
            "unavailable session is retained"
        );
        assert_eq!(
            migrated.state.checkouts[1].session.name,
            format!("{base}-child")
        );
        assert!(migrated.state.checkouts[1].session.archived);
        assert!(migrated.state.checkouts[1].session.archived_by_user);
        assert!(
            migrated
                .state
                .checkouts
                .iter()
                .all(|checkout| !checkout.managed_by_baude),
            "legacy is_worktree alone must not prove baude ownership"
        );

        let first_bytes = std::fs::read(&primary_path).unwrap();
        let second = migrate_for_workspace_at(&root, base, &workspace, reconcile_legacy).unwrap();
        assert_eq!(second, LoadOutcome::Current(migrated));
        assert_eq!(std::fs::read(&primary_path).unwrap(), first_bytes);

        let dormant_bytes = serde_json::to_vec_pretty(&dormant).unwrap();
        std::fs::write(&fallback_path, &dormant_bytes).unwrap();
        let third = migrate_for_workspace_at(&root, base, &workspace, reconcile_legacy).unwrap();
        assert!(matches!(third, LoadOutcome::Current(_)));
        assert_eq!(std::fs::read(&fallback_path).unwrap(), dormant_bytes);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_migration_local() {
        assert_legacy_migration("state");
    }

    #[test]
    fn legacy_migration_daemon() {
        assert_legacy_migration("daemon-state");
    }

    #[test]
    fn atomic_load_errors_are_not_first_run() {
        let root = isolated_root("atomic-load");
        let workspace = test_workspace("claude");
        let primary = root.join(workspace.state_file("state"));

        assert!(matches!(
            load_for_workspace_strict_at(&root, "state", &workspace, reconcile_legacy).unwrap(),
            LoadOutcome::Missing
        ));

        for bytes in [b"{".as_slice(), b"not json".as_slice()] {
            std::fs::write(&primary, bytes).unwrap();
            assert!(matches!(
                load_for_workspace_strict_at(&root, "state", &workspace, reconcile_legacy),
                Err(LoadError::Malformed { ref path, .. }) if path == &primary
            ));
        }

        std::fs::write(
            &primary,
            br#"{"schema_version":99,"state":{"next_repository_key":1,"next_checkout_key":1,"next_first_seen_order":1,"repositories":[],"checkouts":[]}}"#,
        )
        .unwrap();
        assert!(matches!(
            load_for_workspace_strict_at(&root, "state", &workspace, reconcile_legacy),
            Err(LoadError::UnsupportedVersion { ref path, version: 99 }) if path == &primary
        ));

        std::fs::write(&primary, br#"{"schema_version":1,"state":{}}"#).unwrap();
        assert!(matches!(
            load_for_workspace_strict_at(&root, "state", &workspace, reconcile_legacy),
            Err(LoadError::Malformed { ref path, .. }) if path == &primary
        ));

        let mut invalid = current_fixture("invalid");
        invalid
            .state
            .repositories
            .push(invalid.state.repositories[0].clone());
        std::fs::write(&primary, serde_json::to_vec_pretty(&invalid).unwrap()).unwrap();
        assert!(matches!(
            load_for_workspace_strict_at(&root, "state", &workspace, reconcile_legacy),
            Err(LoadError::InvalidState { ref path, .. }) if path == &primary
        ));

        std::fs::remove_file(&primary).unwrap();
        std::fs::create_dir(&primary).unwrap();
        assert!(matches!(
            load_for_workspace_strict_at(&root, "state", &workspace, reconcile_legacy),
            Err(LoadError::Read { ref path, .. }) if path == &primary
        ));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn atomic_replacement_preserves_old_bytes_and_owned_temp_only() {
        let root = isolated_root("atomic-replace");
        let destination = root.join("state-claude.json");
        let old = b"old destination bytes";
        let fixture = current_fixture("replacement");

        for failure in [
            AtomicFailure::Write,
            AtomicFailure::Sync,
            AtomicFailure::Rename,
        ] {
            std::fs::write(&destination, old).unwrap();
            assert!(save_current_at_test(
                &root,
                "state-claude.json",
                &fixture,
                Some(failure),
                None,
            )
            .is_err());
            assert_eq!(std::fs::read(&destination).unwrap(), old);
            assert!(
                std::fs::read_dir(&root)
                    .unwrap()
                    .filter_map(|entry| entry.ok())
                    .all(|entry| !entry.file_name().to_string_lossy().contains(".tmp-")),
                "failed attempt must clean only its owned temp"
            );
        }

        let collision = root.join(".state-claude.json.tmp-collision");
        std::fs::write(&collision, b"other writer").unwrap();
        save_current_at_test(
            &root,
            "state-claude.json",
            &fixture,
            None,
            Some(collision.clone()),
        )
        .unwrap();
        assert_eq!(std::fs::read(&collision).unwrap(), b"other writer");
        assert_eq!(
            load_current_at(&root, "state-claude.json").unwrap(),
            fixture
        );

        std::fs::remove_file(&destination).unwrap();
        assert!(matches!(
            load_for_workspace_strict_at(
                &root,
                "state",
                &test_workspace("claude"),
                reconcile_legacy
            )
            .unwrap(),
            LoadOutcome::Missing
        ));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn current_round_trip() {
        let claude_root = isolated_root("current-claude");
        let opencode_root = isolated_root("current-opencode");
        let claude = current_fixture("claude");
        let opencode = current_fixture("opencode");
        save_current_at(&claude_root, "state-claude.json", &claude).unwrap();
        save_current_at(&opencode_root, "state-opencode.json", &opencode).unwrap();
        assert_eq!(
            load_current_at(&claude_root, "state-claude.json").unwrap(),
            claude
        );
        assert_eq!(
            load_current_at(&opencode_root, "state-opencode.json").unwrap(),
            opencode
        );
        assert_ne!(
            claude.state.repositories[0].observed_main_worktree,
            opencode.state.repositories[0].observed_main_worktree
        );
        std::fs::remove_dir_all(claude_root).unwrap();
        std::fs::remove_dir_all(opencode_root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_path_round_trip() {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};

        let root = isolated_root("non-utf8");
        let original = PathBuf::from(std::ffi::OsString::from_vec(b"/tmp/repo-\xff".to_vec()));
        let persisted = PersistedPath::from_path(&original);
        assert_eq!(persisted.as_bytes(), original.as_os_str().as_bytes());
        let mut fixture = current_fixture("bytes");
        fixture.state.repositories[0].observed_common_dir = persisted.clone();
        fixture.state.repositories[0].observed_main_worktree = persisted.clone();
        fixture.state.checkouts[0].observed_path = persisted.clone();
        fixture.state.checkouts[0].session.cwd = persisted.clone();
        fixture.state.checkouts[0].session.repo_root = persisted;
        save_current_at(&root, "state-claude.json", &fixture).unwrap();
        let loaded = load_current_at(&root, "state-claude.json").unwrap();
        let reconciled = |path: &PersistedPath| path.to_path_buf();
        let reconstructed = reconciled(&loaded.state.checkouts[0].observed_path);
        assert_eq!(
            reconstructed.as_os_str().as_bytes(),
            original.as_os_str().as_bytes()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn auto_archive_ms_resolves_env_then_config_then_default() {
        std::env::remove_var("BAUDED_AUTO_ARCHIVE_MIN");
        let mut c = Config::default();
        assert_eq!(c.auto_archive_ms(), crate::session::AUTO_ARCHIVE_IDLE_MS);
        c.auto_archive_minutes = Some(5);
        assert_eq!(c.auto_archive_ms(), 5 * 60_000);
        c.auto_archive_minutes = Some(0);
        assert_eq!(c.auto_archive_ms(), 0, "0 disables auto-archiving");
        std::env::set_var("BAUDED_AUTO_ARCHIVE_MIN", "1");
        assert_eq!(c.auto_archive_ms(), 60_000, "env overrides config");
        std::env::remove_var("BAUDED_AUTO_ARCHIVE_MIN");
    }
}
