use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::repository::{
    CheckoutHealth, CheckoutRole, PersistedPath, RepositoryHealth, RepositoryState,
    RetainedSessionState, SavedCheckout, SavedRepository, UnavailableCause,
};

pub const SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StateFile {
    pub schema_version: u32,
    pub state: RepositoryState,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct SchemaV1StateFile {
    schema_version: u32,
    state: SchemaV1RepositoryState,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct SchemaV1RepositoryState {
    next_repository_key: u64,
    next_checkout_key: u64,
    next_first_seen_order: u64,
    repositories: Vec<SavedRepository>,
    checkouts: Vec<SchemaV1SavedCheckout>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct SchemaV1SavedCheckout {
    key: crate::repository::CheckoutKey,
    repository_key: crate::repository::RepositoryKey,
    role: CheckoutRole,
    managed_by_baude: bool,
    observed_path: PersistedPath,
    observed_branch: Option<String>,
    first_seen_order: u64,
    active_intent: bool,
    session: RetainedSessionState,
    health: CheckoutHealth,
}

impl SchemaV1StateFile {
    fn migrate(self) -> std::result::Result<StateFile, String> {
        if self.schema_version != 1 {
            return Err(format!("expected schema 1, got {}", self.schema_version));
        }
        let checkouts = self
            .state
            .checkouts
            .into_iter()
            .map(|checkout| {
                if let CheckoutHealth::Unavailable(UnavailableCause::TeardownPending {
                    agent_identity,
                    shell_identity,
                    agent_stopped,
                    shell_stopped,
                    ..
                }) = &checkout.health
                {
                    if (!agent_stopped && agent_identity.is_none())
                        || (!shell_stopped && shell_identity.is_none())
                    {
                        return Err(format!(
                            "checkout {} claims live teardown ownership without exact identity",
                            checkout.key.get()
                        ));
                    }
                }
                let lifecycle = crate::repository::CheckoutLifecycle::from_legacy(
                    checkout.active_intent,
                    &checkout.health,
                );
                Ok(SavedCheckout::new(
                    checkout.key,
                    checkout.repository_key,
                    checkout.role,
                    checkout.managed_by_baude,
                    checkout.observed_path,
                    checkout.observed_branch,
                    checkout.first_seen_order,
                    lifecycle,
                    checkout.session,
                ))
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let state = RepositoryState {
            next_repository_key: self.state.next_repository_key,
            next_checkout_key: self.state.next_checkout_key,
            next_first_seen_order: self.state.next_first_seen_order,
            repositories: self.state.repositories,
            checkouts,
        };
        state.validate().map_err(|error| error.to_string())?;
        state
            .validate_lifecycle_views()
            .map_err(|error| error.to_string())?;
        Ok(StateFile::new(state))
    }
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
    Persist {
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
            Self::Persist { path, cause } => {
                write!(f, "persist migrated state {}: {cause}", path.display())
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
    DirectorySync,
}

#[derive(Debug)]
pub struct SaveError {
    replacement_committed: bool,
    source: anyhow::Error,
}

impl SaveError {
    fn not_committed(source: impl Into<anyhow::Error>) -> Self {
        Self {
            replacement_committed: false,
            source: source.into(),
        }
    }

    pub fn before_replacement(source: impl Into<anyhow::Error>) -> Self {
        Self::not_committed(source)
    }

    pub fn after_replacement(source: impl Into<anyhow::Error>) -> Self {
        Self {
            replacement_committed: true,
            source: source.into(),
        }
    }

    pub fn replacement_committed(&self) -> bool {
        self.replacement_committed
    }
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.source.fmt(f)
    }
}

impl std::error::Error for SaveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.source()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LegacyReconciliation {
    Available {
        common_dir: PersistedPath,
        main_worktree: PersistedPath,
        checkout_path: PersistedPath,
        observed_branch: Option<String>,
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
    save_current_at_status(root, file, state).map_err(anyhow::Error::new)
}

pub fn save_current_at_status(
    root: &std::path::Path,
    file: &str,
    state: &StateFile,
) -> std::result::Result<(), SaveError> {
    atomic_save_current(root, file, state, None, None)
}

pub fn load_current_at(root: &std::path::Path, file: &str) -> Result<StateFile> {
    match load_named_at(root, file)? {
        LoadOutcome::Current(state) => Ok(state),
        LoadOutcome::Missing => {
            anyhow::bail!("state file {} is missing", root.join(file).display())
        }
        LoadOutcome::Legacy(_) => unreachable!("named current loader never migrates legacy state"),
    }
}

pub fn load_for_workspace_strict_at(
    root: &std::path::Path,
    base: &str,
    ws: &crate::workspace::Workspace,
    mut reconcile: impl FnMut(&SavedSession) -> LegacyReconciliation,
) -> std::result::Result<LoadOutcome, LoadError> {
    let primary_file = ws.state_file(base);
    let primary_path = root.join(&primary_file);
    hold_state_lock(&primary_path).map_err(|source| LoadError::Read {
        path: lock_path(&primary_path),
        source,
    })?;
    let (source_path, bytes) = match read_state_source(&primary_path)? {
        Some(bytes) => (primary_path.clone(), bytes),
        None => match ws.legacy_state_file(base) {
            Some(legacy_file) => {
                let legacy_path = root.join(legacy_file);
                match read_state_source(&legacy_path)? {
                    Some(bytes) => (legacy_path, bytes),
                    None => return Ok(LoadOutcome::Missing),
                }
            }
            None => return Ok(LoadOutcome::Missing),
        },
    };

    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|error| LoadError::Malformed {
            path: source_path.clone(),
            cause: error.to_string(),
        })?;
    if let Some(version) = value.get("schema_version") {
        let version = version.as_u64().ok_or_else(|| LoadError::Malformed {
            path: source_path.clone(),
            cause: "schema_version must be an unsigned integer".into(),
        })?;
        if version == 1 && SCHEMA_VERSION == 2 {
            let legacy: SchemaV1StateFile =
                serde_json::from_value(value).map_err(|error| LoadError::Malformed {
                    path: source_path.clone(),
                    cause: error.to_string(),
                })?;
            let migrated = legacy.migrate().map_err(|cause| LoadError::InvalidState {
                path: source_path.clone(),
                cause,
            })?;
            save_current_at(root, &primary_file, &migrated).map_err(|error| {
                LoadError::Persist {
                    path: primary_path,
                    cause: error.to_string(),
                }
            })?;
            return Ok(LoadOutcome::Legacy(migrated));
        }
        if version != u64::from(SCHEMA_VERSION) {
            return Err(LoadError::UnsupportedVersion {
                path: source_path,
                version,
            });
        }
        let current: StateFile =
            serde_json::from_value(value).map_err(|error| LoadError::Malformed {
                path: source_path.clone(),
                cause: error.to_string(),
            })?;
        current
            .state
            .validate()
            .map_err(|error| LoadError::InvalidState {
                path: source_path.clone(),
                cause: error.to_string(),
            })?;
        current
            .state
            .validate_lifecycle_views()
            .map_err(|error| LoadError::InvalidState {
                path: source_path,
                cause: error.to_string(),
            })?;
        return Ok(LoadOutcome::Current(current));
    }

    let legacy: State = serde_json::from_value(value).map_err(|error| LoadError::Malformed {
        path: source_path.clone(),
        cause: error.to_string(),
    })?;
    let migrated_state =
        migrate_legacy(legacy, &mut reconcile).map_err(|error| LoadError::InvalidState {
            path: source_path,
            cause: error.to_string(),
        })?;
    let migrated = StateFile::new(migrated_state);
    save_current_at(root, &primary_file, &migrated).map_err(|error| LoadError::Persist {
        path: primary_path,
        cause: error.to_string(),
    })?;
    Ok(LoadOutcome::Legacy(migrated))
}

fn read_state_source(path: &std::path::Path) -> std::result::Result<Option<Vec<u8>>, LoadError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(LoadError::Read {
            path: path.to_path_buf(),
            source,
        }),
    }
}

#[doc(hidden)]
pub fn save_current_at_test(
    root: &std::path::Path,
    file: &str,
    state: &StateFile,
    failure: Option<AtomicFailure>,
    _first_temp: Option<PathBuf>,
) -> std::result::Result<(), SaveError> {
    atomic_save_current(root, file, state, failure, _first_temp)
}

static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);
static HELD_STATE_LOCKS: OnceLock<Mutex<HashMap<PathBuf, File>>> = OnceLock::new();

fn lock_path(destination: &std::path::Path) -> PathBuf {
    let name = destination
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("state"))
        .to_string_lossy();
    destination.with_file_name(format!(".{name}.lock"))
}

fn hold_state_lock(destination: &std::path::Path) -> std::io::Result<()> {
    let path = lock_path(destination);
    let locks = HELD_STATE_LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut locks = locks
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if locks.contains_key(&path) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)?;
    lock.try_lock()?;
    locks.insert(path, lock);
    Ok(())
}

fn atomic_save_current(
    root: &std::path::Path,
    file: &str,
    state: &StateFile,
    failure: Option<AtomicFailure>,
    first_temp: Option<PathBuf>,
) -> std::result::Result<(), SaveError> {
    let state = state.clone();
    state.state.validate().map_err(SaveError::not_committed)?;
    state
        .state
        .validate_lifecycle_views()
        .map_err(SaveError::not_committed)?;
    let bytes = serde_json::to_vec_pretty(&state).map_err(SaveError::not_committed)?;
    let destination = root.join(file);
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(SaveError::not_committed)?;
    }
    hold_state_lock(&destination).map_err(SaveError::not_committed)?;

    let mut first_temp = first_temp;
    let (temporary, output) = loop {
        let candidate = first_temp.take().unwrap_or_else(|| {
            let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            root.join(format!(".{file}.tmp-{}-{sequence}", std::process::id()))
        });
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(output) => break (candidate, output),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(SaveError::not_committed(error)),
        }
    };
    let mut output = Some(output);

    let mut replacement_committed = false;
    let attempt = (|| -> Result<()> {
        if failure == Some(AtomicFailure::Write) {
            anyhow::bail!("injected write failure");
        }
        let file = output.as_mut().expect("owned temporary file");
        file.write_all(&bytes)?;
        file.flush()?;
        if failure == Some(AtomicFailure::Sync) {
            anyhow::bail!("injected sync failure");
        }
        file.sync_all()?;
        drop(output.take());
        if failure == Some(AtomicFailure::Rename) {
            anyhow::bail!("injected rename failure");
        }
        std::fs::rename(&temporary, &destination)?;
        replacement_committed = true;
        if failure == Some(AtomicFailure::DirectorySync) {
            anyhow::bail!("injected directory sync failure");
        }
        // On Unix, rename durability requires syncing the containing directory
        // as well as the temporary file. Other targets retain rename's native
        // guarantees until they gain an equivalent directory-sync primitive.
        #[cfg(unix)]
        {
            let parent = destination
                .parent()
                .ok_or_else(|| anyhow::anyhow!("state destination has no parent"))?;
            std::fs::File::open(parent)?.sync_all()?;
        }
        Ok(())
    })();

    if attempt.is_err() {
        drop(output.take());
        let _ = std::fs::remove_file(&temporary);
    }
    attempt.map_err(|source| SaveError {
        replacement_committed,
        source,
    })
}

/// Select and migrate exactly one workspace-owned source under an injected root.
pub fn migrate_for_workspace_at(
    root: &std::path::Path,
    base: &str,
    ws: &crate::workspace::Workspace,
    reconcile: impl FnMut(&SavedSession) -> LegacyReconciliation,
) -> std::result::Result<LoadOutcome, LoadError> {
    load_for_workspace_strict_at(root, base, ws, reconcile)
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
    let mut singleton_roles = std::collections::HashSet::new();

    for (source_order, session) in legacy.sessions.into_iter().enumerate() {
        let (
            identity,
            common_dir,
            main_worktree,
            repository_health,
            checkout_path,
            observed_branch,
            mut checkout_role,
            managed_by_baude,
            checkout_health,
        ) = match reconcile(&session) {
            LegacyReconciliation::Available {
                common_dir,
                main_worktree,
                checkout_path,
                observed_branch,
                checkout_role,
                managed_by_baude,
            } => (
                LegacyRepositoryIdentity::Available(common_dir.clone()),
                common_dir,
                main_worktree,
                RepositoryHealth::Available,
                checkout_path,
                observed_branch,
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
                session
                    .branch
                    .as_ref()
                    .map(|branch| format!("refs/heads/{branch}")),
                CheckoutRole::Main,
                false,
                CheckoutHealth::Unavailable(checkout_cause),
            ),
        };

        let repository_key = if let Some(key) = repositories.get(&identity) {
            *key
        } else {
            let key = state.allocate_repository_key()?;
            let first_seen_order = state.allocate_first_seen_order()?;
            state.repositories.push(SavedRepository {
                key,
                observed_common_dir: common_dir,
                observed_main_worktree: main_worktree.clone(),
                first_seen_order,
                health: repository_health,
            });
            repositories.insert(identity, key);
            key
        };

        if matches!(
            checkout_role,
            CheckoutRole::Main | CheckoutRole::PrimaryDefault
        ) && !singleton_roles.insert((repository_key, checkout_role))
        {
            // Legacy state permits multiple sessions in one checkout. Keep one
            // structural singleton and retain every additional session as an
            // independently restorable non-singleton child.
            checkout_role = CheckoutRole::ManagedBranch;
        }

        let checkout_key = state.allocate_checkout_key()?;
        let first_seen_order = state.allocate_first_seen_order()?;
        let is_worktree = checkout_path != main_worktree;
        let lifecycle = crate::repository::CheckoutLifecycle::from_legacy(true, &checkout_health);
        state.checkouts.push(SavedCheckout::new(
            checkout_key,
            repository_key,
            checkout_role,
            managed_by_baude,
            checkout_path.clone(),
            observed_branch,
            first_seen_order,
            lifecycle,
            RetainedSessionState {
                name: session.name,
                cwd: checkout_path,
                repo_root: main_worktree,
                branch: session.branch,
                is_worktree,
                shell_open: session.shell_open,
                archived: session.archived,
                archived_by_user: session.archived_by_user,
                resume_id: None,
            },
        ));
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

pub fn load() -> std::result::Result<LoadOutcome, LoadError> {
    load_for_workspace("state", crate::workspace::active(), |_| {
        LegacyReconciliation::Unavailable {
            repository_cause: UnavailableCause::Other(
                "legacy repository has not been reconciled".into(),
            ),
            checkout_cause: UnavailableCause::Other(
                "legacy checkout has not been reconciled".into(),
            ),
        }
    })
}

pub fn save(state: &StateFile) -> Result<()> {
    save_for_workspace("state", crate::workspace::active(), state)
}

/// Load a workspace's session state (`<base>-<ws>.json`), falling back to
/// the legacy un-suffixed file for the default workspace so pre-workspace
/// session lists survive the upgrade. Saves never target the legacy name.
pub fn load_for_workspace(
    base: &str,
    ws: &crate::workspace::Workspace,
    reconcile: impl FnMut(&SavedSession) -> LegacyReconciliation,
) -> std::result::Result<LoadOutcome, LoadError> {
    load_for_workspace_strict_at(&config_base(), base, ws, reconcile)
}

pub fn save_for_workspace(
    base: &str,
    ws: &crate::workspace::Workspace,
    state: &StateFile,
) -> Result<()> {
    save_named(&ws.state_file(base), state)
}

pub fn save_for_workspace_status(
    base: &str,
    ws: &crate::workspace::Workspace,
    state: &StateFile,
) -> std::result::Result<(), SaveError> {
    save_current_at_status(&config_base(), &ws.state_file(base), state)
}

/// Load session state from a specific file under the config dir. The TUI and
/// the daemon keep separate files so they never clobber each other's sessions.
pub fn load_named(file: &str) -> std::result::Result<LoadOutcome, LoadError> {
    load_named_at(&config_base(), file)
}

pub fn save_named(file: &str, state: &StateFile) -> Result<()> {
    save_current_at(&config_base(), file, state)
}

fn load_named_at(
    root: &std::path::Path,
    file: &str,
) -> std::result::Result<LoadOutcome, LoadError> {
    let path = root.join(file);
    let Some(bytes) = read_state_source(&path)? else {
        return Ok(LoadOutcome::Missing);
    };
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|error| LoadError::Malformed {
            path: path.clone(),
            cause: error.to_string(),
        })?;
    let Some(version) = value.get("schema_version") else {
        return Err(LoadError::Malformed {
            path,
            cause: "legacy state requires workspace-aware migration".into(),
        });
    };
    let version = version.as_u64().ok_or_else(|| LoadError::Malformed {
        path: path.clone(),
        cause: "schema_version must be an unsigned integer".into(),
    })?;
    if version != u64::from(SCHEMA_VERSION) {
        return Err(LoadError::UnsupportedVersion { path, version });
    }
    let current: StateFile =
        serde_json::from_value(value).map_err(|error| LoadError::Malformed {
            path: path.clone(),
            cause: error.to_string(),
        })?;
    current
        .state
        .validate()
        .map_err(|error| LoadError::InvalidState {
            path,
            cause: error.to_string(),
        })?;
    Ok(LoadOutcome::Current(current))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::{
        CheckoutHealth, CheckoutLifecycle, CheckoutRole, PersistedPath, ProcessIdentity,
        RepositoryHealth, RetainedSessionState, SavedCheckout, SavedRepository, UnavailableCause,
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
        let repository_key = state.allocate_repository_key().unwrap();
        let checkout_key = state.allocate_checkout_key().unwrap();
        let repository_order = state.allocate_first_seen_order().unwrap();
        let checkout_order = state.allocate_first_seen_order().unwrap();
        let main = PathBuf::from(format!("/{prefix}/repo"));
        let checkout_path = PathBuf::from(format!("/{prefix}/repo-default"));
        state.repositories.push(SavedRepository {
            key: repository_key,
            observed_common_dir: PersistedPath::from_path(&main.join(".git")),
            observed_main_worktree: PersistedPath::from_path(&main),
            first_seen_order: repository_order,
            health: RepositoryHealth::Unavailable(UnavailableCause::IdentityChanged),
        });
        state.checkouts.push(SavedCheckout::new(
            checkout_key,
            repository_key,
            CheckoutRole::PrimaryDefault,
            true,
            PersistedPath::from_path(&checkout_path),
            Some("feature/retained".into()),
            checkout_order,
            CheckoutLifecycle::Protected(UnavailableCause::Missing),
            RetainedSessionState {
                name: format!("{prefix}-session"),
                cwd: PersistedPath::from_path(&checkout_path),
                repo_root: PersistedPath::from_path(&main),
                branch: Some("feature/retained".into()),
                is_worktree: true,
                shell_open: true,
                archived: true,
                archived_by_user: true,
                resume_id: Some("opaque-retained-id".into()),
            },
        ));
        StateFile::new(state)
    }

    fn schema_v1_protected_fixture() -> SchemaV1StateFile {
        let mut state = RepositoryState::default();
        let repository_key = state.allocate_repository_key().unwrap();
        let repository_order = state.allocate_first_seen_order().unwrap();
        let root = PathBuf::from("/schema-v1/repo");
        let repository = SavedRepository {
            key: repository_key,
            observed_common_dir: PersistedPath::from_path(&root.join(".git")),
            observed_main_worktree: PersistedPath::from_path(&root),
            first_seen_order: repository_order,
            health: RepositoryHealth::Available,
        };
        let identity = ProcessIdentity {
            pid: 4242,
            start_time: 99,
            process_group: 4242,
            session: 4242,
        };
        let causes = vec![
            (
                false,
                UnavailableCause::PendingActivation {
                    branch: "feature/pending".into(),
                    created_branch: None,
                    preexisting_branch_owner: None,
                },
            ),
            (
                false,
                UnavailableCause::ActivationRecovery {
                    branch: "feature/recovery".into(),
                    created_branch: Some(true),
                    preexisting_branch_owner: None,
                    verification: "verify".into(),
                    compensation: "compensate".into(),
                },
            ),
            (
                true,
                UnavailableCause::TeardownPending {
                    agent_pid: Some(identity.pid),
                    shell_pid: None,
                    agent_identity: Some(identity),
                    shell_identity: None,
                    agent_stopped: false,
                    shell_stopped: true,
                    detail: "retry stop".into(),
                },
            ),
            (
                true,
                UnavailableCause::RemovalTombstone("authority revoked".into()),
            ),
            (
                true,
                UnavailableCause::StoppedActiveRecovery {
                    agent_restarted: false,
                    shell_restarted: false,
                    detail: "rollback pending".into(),
                },
            ),
        ];
        let mut checkouts = Vec::new();
        for (index, (active_intent, cause)) in causes.into_iter().enumerate() {
            let key = state.allocate_checkout_key().unwrap();
            let order = state.allocate_first_seen_order().unwrap();
            let path = PathBuf::from(format!("/schema-v1/checkout-{index}"));
            checkouts.push(SchemaV1SavedCheckout {
                key,
                repository_key,
                role: CheckoutRole::ManagedBranch,
                managed_by_baude: true,
                observed_path: PersistedPath::from_path(&path),
                observed_branch: Some(format!("refs/heads/feature/{index}")),
                first_seen_order: order,
                active_intent,
                session: RetainedSessionState {
                    name: format!("protected-{index}"),
                    cwd: PersistedPath::from_path(&path),
                    repo_root: PersistedPath::from_path(&root),
                    branch: Some(format!("feature/{index}")),
                    is_worktree: true,
                    shell_open: false,
                    archived: false,
                    archived_by_user: false,
                    resume_id: None,
                },
                health: CheckoutHealth::Unavailable(cause),
            });
        }
        SchemaV1StateFile {
            schema_version: 1,
            state: SchemaV1RepositoryState {
                next_repository_key: state.next_repository_key,
                next_checkout_key: state.next_checkout_key,
                next_first_seen_order: state.next_first_seen_order,
                repositories: vec![repository],
                checkouts,
            },
        }
    }

    fn malformed_schema_v1_ownership_fixture() -> SchemaV1StateFile {
        let mut fixture = schema_v1_protected_fixture();
        fixture.state.checkouts[2].health =
            CheckoutHealth::Unavailable(UnavailableCause::TeardownPending {
                agent_pid: Some(4242),
                shell_pid: None,
                agent_identity: None,
                shell_identity: None,
                agent_stopped: false,
                shell_stopped: true,
                detail: "missing exact identity".into(),
            });
        fixture
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
                observed_branch: session
                    .branch
                    .as_ref()
                    .map(|branch| format!("refs/heads/{branch}")),
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
        assert_eq!(
            migrated.state.checkouts[0].observed_branch.as_deref(),
            Some("refs/heads/develop"),
            "reconciliation must persist Git's full ref separately from the legacy display branch"
        );
        assert_eq!(
            migrated.state.checkouts[0].session.branch.as_deref(),
            Some("develop")
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
    fn legacy_migration_retains_duplicate_main_and_linked_sessions() {
        let mut legacy = legacy_fixture("duplicates");
        legacy.sessions.push(legacy.sessions[0].clone());
        legacy.sessions.push(legacy.sessions[1].clone());

        let migrated = migrate_legacy(legacy, &mut reconcile_legacy).unwrap();
        assert_eq!(migrated.checkouts.len(), 5);
        let shared_repository = migrated.checkouts[0].repository_key;
        assert_eq!(
            migrated
                .checkouts
                .iter()
                .filter(|checkout| {
                    checkout.repository_key == shared_repository
                        && checkout.role == CheckoutRole::Main
                })
                .count(),
            1
        );
        migrated.validate().unwrap();
        migrated.validate_lifecycle_views().unwrap();
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
    fn second_workspace_owner_cannot_load_while_writer_lock_is_held() {
        let root = isolated_root("writer-lock");
        let workspace = test_workspace("claude");
        let destination = root.join(workspace.state_file("state"));
        let lock_path = lock_path(&destination);
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .unwrap();
        lock.try_lock().unwrap();

        assert!(matches!(
            load_for_workspace_strict_at(&root, "state", &workspace, reconcile_legacy),
            Err(LoadError::Read { ref path, ref source })
                if path == &lock_path && source.kind() == std::io::ErrorKind::WouldBlock
        ));
        lock.unlock().unwrap();
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

        std::fs::write(&destination, old).unwrap();
        assert!(save_current_at_test(
            &root,
            "state-claude.json",
            &fixture,
            Some(AtomicFailure::DirectorySync),
            None,
        )
        .is_err());
        assert_eq!(
            load_current_at(&root, "state-claude.json").unwrap(),
            fixture,
            "a post-rename sync error is reported even though the rename is already visible"
        );

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
        fixture.state.checkouts[0].session.is_worktree = false;
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

    #[test]
    fn lifecycle_schema_v2_migrates_protected_states() {
        assert_eq!(SCHEMA_VERSION, 2);
        let root = isolated_root("lifecycle-schema-v2");
        let workspace = test_workspace("claude");
        let file = workspace.state_file("state");
        let legacy = schema_v1_protected_fixture();
        std::fs::write(
            root.join(&file),
            serde_json::to_vec_pretty(&legacy).unwrap(),
        )
        .unwrap();

        let first = load_for_workspace_strict_at(&root, "state", &workspace, |_| {
            panic!("schema-v1 migration must not use flat legacy reconciliation")
        })
        .unwrap();
        let LoadOutcome::Legacy(first) = first else {
            panic!("schema-v1 input was not migrated")
        };
        assert!(first.state.checkouts.iter().all(|checkout| {
            checkout.lifecycle().is_protected() && !checkout.lifecycle().is_launchable()
        }));
        let first_bytes = std::fs::read(root.join(&file)).unwrap();
        let second =
            load_for_workspace_strict_at(&root, "state", &workspace, |_| unreachable!()).unwrap();
        assert!(matches!(second, LoadOutcome::Current(_)));
        assert_eq!(std::fs::read(root.join(&file)).unwrap(), first_bytes);

        let malformed = malformed_schema_v1_ownership_fixture();
        std::fs::write(root.join(&file), serde_json::to_vec(&malformed).unwrap()).unwrap();
        assert!(
            load_for_workspace_strict_at(&root, "state", &workspace, |_| unreachable!()).is_err()
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
