use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use anyhow::{anyhow, Result};

use crate::repository::SavedCheckout;

/// One checkout reported by Git's stable, NUL-delimited worktree inventory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorktreeRecord {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub bare: bool,
    pub detached: bool,
    pub locked: bool,
    pub prunable: bool,
}

/// Canonical Git-owned facts for a repository and the checkout containing the input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepositorySnapshot {
    pub canonical_input: PathBuf,
    pub common_dir: PathBuf,
    pub main_worktree: PathBuf,
    pub selected_worktree: WorktreeRecord,
    pub worktrees: Vec<WorktreeRecord>,
}

/// Fail-closed repository discovery failures, kept typed for admission callers.
#[derive(Debug)]
pub enum RepositoryDiscoveryError {
    Canonicalize {
        path: PathBuf,
        source: std::io::Error,
    },
    CommandStart {
        operation: &'static str,
        source: std::io::Error,
    },
    GitCommand {
        operation: &'static str,
        status: Option<i32>,
        stderr: String,
    },
    MalformedTopology(String),
    InvalidPathOutput(&'static str),
    SelectedWorktreeMissing(PathBuf),
}

impl fmt::Display for RepositoryDiscoveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Canonicalize { path, source } => {
                write!(
                    f,
                    "canonicalize repository input {}: {source}",
                    path.display()
                )
            }
            Self::CommandStart { operation, source } => {
                write!(f, "start Git {operation}: {source}")
            }
            Self::GitCommand {
                operation,
                status,
                stderr,
            } => write!(
                f,
                "Git {operation} failed (status {}): {stderr}",
                status.map_or_else(|| "signal".to_owned(), |code| code.to_string())
            ),
            Self::MalformedTopology(reason) => {
                write!(f, "malformed Git worktree topology: {reason}")
            }
            Self::InvalidPathOutput(operation) => {
                write!(f, "Git {operation} returned an invalid path")
            }
            Self::SelectedWorktreeMissing(path) => write!(
                f,
                "Git selected checkout {} is absent from its worktree inventory",
                path.display()
            ),
        }
    }
}

impl std::error::Error for RepositoryDiscoveryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Canonicalize { source, .. } | Self::CommandStart { source, .. } => Some(source),
            _ => None,
        }
    }
}

fn git_bytes(
    repo: &Path,
    args: &[&OsStr],
    operation: &'static str,
) -> std::result::Result<Output, RepositoryDiscoveryError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|source| RepositoryDiscoveryError::CommandStart { operation, source })?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(RepositoryDiscoveryError::GitCommand {
            operation,
            status: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        })
    }
}

fn os_string_from_git(bytes: Vec<u8>) -> std::result::Result<OsString, ()> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Ok(OsString::from_vec(bytes))
    }
    #[cfg(not(unix))]
    {
        String::from_utf8(bytes).map(OsString::from).map_err(|_| ())
    }
}

fn path_from_git_stdout(
    mut bytes: Vec<u8>,
    operation: &'static str,
) -> std::result::Result<PathBuf, RepositoryDiscoveryError> {
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
    }
    if bytes.is_empty() || bytes.contains(&0) {
        return Err(RepositoryDiscoveryError::InvalidPathOutput(operation));
    }
    os_string_from_git(bytes)
        .map(PathBuf::from)
        .map_err(|_| RepositoryDiscoveryError::InvalidPathOutput(operation))
}

#[derive(Default)]
struct WorktreeRecordBuilder {
    path: Option<PathBuf>,
    has_head: bool,
    branch: Option<String>,
    bare: bool,
    detached: bool,
    locked: bool,
    prunable: bool,
}

impl WorktreeRecordBuilder {
    fn finish(self) -> std::result::Result<WorktreeRecord, RepositoryDiscoveryError> {
        let path = self.path.ok_or_else(|| {
            RepositoryDiscoveryError::MalformedTopology("record has no worktree path".into())
        })?;
        if !self.bare && !self.has_head {
            return Err(RepositoryDiscoveryError::MalformedTopology(format!(
                "worktree {} has no HEAD field",
                path.display()
            )));
        }
        if self.bare && (self.has_head || self.branch.is_some() || self.detached) {
            return Err(RepositoryDiscoveryError::MalformedTopology(format!(
                "bare repository {} contains checkout fields",
                path.display()
            )));
        }
        if !self.bare && (self.branch.is_some() == self.detached) {
            return Err(RepositoryDiscoveryError::MalformedTopology(format!(
                "worktree {} must contain exactly one branch or detached marker",
                path.display()
            )));
        }
        Ok(WorktreeRecord {
            path,
            branch: self.branch,
            bare: self.bare,
            detached: self.detached,
            locked: self.locked,
            prunable: self.prunable,
        })
    }
}

fn parse_worktree_porcelain(
    bytes: &[u8],
) -> std::result::Result<Vec<WorktreeRecord>, RepositoryDiscoveryError> {
    if bytes.is_empty() || !bytes.ends_with(b"\0\0") {
        return Err(RepositoryDiscoveryError::MalformedTopology(
            "inventory is not terminated by an empty NUL field".into(),
        ));
    }

    let mut records = Vec::new();
    let mut current: Option<WorktreeRecordBuilder> = None;
    for field in bytes.split(|byte| *byte == 0) {
        if field.is_empty() {
            if let Some(builder) = current.take() {
                records.push(builder.finish()?);
            }
            continue;
        }

        if let Some(path) = field.strip_prefix(b"worktree ") {
            if current.is_some() || path.is_empty() {
                return Err(RepositoryDiscoveryError::MalformedTopology(
                    "unexpected worktree field".into(),
                ));
            }
            let path = os_string_from_git(path.to_vec()).map_err(|_| {
                RepositoryDiscoveryError::MalformedTopology(
                    "worktree path is not representable on this platform".into(),
                )
            })?;
            current = Some(WorktreeRecordBuilder {
                path: Some(PathBuf::from(path)),
                ..WorktreeRecordBuilder::default()
            });
            continue;
        }

        let builder = current.as_mut().ok_or_else(|| {
            RepositoryDiscoveryError::MalformedTopology(
                "field appears before its worktree path".into(),
            )
        })?;
        if let Some(head) = field.strip_prefix(b"HEAD ") {
            if builder.has_head || head.is_empty() {
                return Err(RepositoryDiscoveryError::MalformedTopology(
                    "invalid or duplicate HEAD field".into(),
                ));
            }
            builder.has_head = true;
        } else if let Some(branch) = field.strip_prefix(b"branch ") {
            if builder.branch.is_some() || branch.is_empty() {
                return Err(RepositoryDiscoveryError::MalformedTopology(
                    "invalid or duplicate branch field".into(),
                ));
            }
            builder.branch = Some(String::from_utf8(branch.to_vec()).map_err(|_| {
                RepositoryDiscoveryError::MalformedTopology("branch ref is not UTF-8".into())
            })?);
        } else if field == b"bare" {
            builder.bare = true;
        } else if field == b"detached" {
            builder.detached = true;
        } else if field == b"locked" || field.starts_with(b"locked ") {
            builder.locked = true;
        } else if field == b"prunable" || field.starts_with(b"prunable ") {
            builder.prunable = true;
        } else {
            return Err(RepositoryDiscoveryError::MalformedTopology(format!(
                "unknown field {:?}",
                String::from_utf8_lossy(field)
            )));
        }
    }

    if records.is_empty() {
        return Err(RepositoryDiscoveryError::MalformedTopology(
            "inventory contains no worktrees".into(),
        ));
    }
    Ok(records)
}

/// Discover canonical repository membership without reading Git's on-disk layout.
pub fn discover_repository(
    path: &Path,
) -> std::result::Result<RepositorySnapshot, RepositoryDiscoveryError> {
    let canonical_input =
        path.canonicalize()
            .map_err(|source| RepositoryDiscoveryError::Canonicalize {
                path: path.to_path_buf(),
                source,
            })?;

    let common_output = git_bytes(
        &canonical_input,
        &[
            OsStr::new("rev-parse"),
            OsStr::new("--path-format=absolute"),
            OsStr::new("--git-common-dir"),
        ],
        "rev-parse common directory",
    )?;
    let common_dir = path_from_git_stdout(common_output.stdout, "rev-parse common directory")?
        .canonicalize()
        .map_err(|source| RepositoryDiscoveryError::Canonicalize {
            path: path.to_path_buf(),
            source,
        })?;

    let inventory_output = git_bytes(
        &canonical_input,
        &[
            OsStr::new("worktree"),
            OsStr::new("list"),
            OsStr::new("--porcelain"),
            OsStr::new("-z"),
        ],
        "worktree inventory",
    )?;
    let mut worktrees = parse_worktree_porcelain(&inventory_output.stdout)?;
    for record in &mut worktrees {
        if let Ok(canonical) = record.path.canonicalize() {
            record.path = canonical;
        }
    }
    let main_worktree = worktrees
        .first()
        .expect("parser rejects an empty inventory")
        .path
        .clone();

    // This query identifies the selected checkout only; repository identity is the
    // canonical common directory plus Git's main-first worktree inventory above.
    let selected_output = git_bytes(
        &canonical_input,
        &[
            OsStr::new("rev-parse"),
            OsStr::new("--path-format=absolute"),
            OsStr::new("--show-toplevel"),
        ],
        "rev-parse selected worktree",
    )?;
    let selected_path =
        path_from_git_stdout(selected_output.stdout, "rev-parse selected worktree")?
            .canonicalize()
            .map_err(|source| RepositoryDiscoveryError::Canonicalize {
                path: path.to_path_buf(),
                source,
            })?;
    let selected_worktree = worktrees
        .iter()
        .find(|record| record.path == selected_path)
        .cloned()
        .ok_or(RepositoryDiscoveryError::SelectedWorktreeMissing(
            selected_path,
        ))?;

    Ok(RepositorySnapshot {
        canonical_input,
        common_dir,
        main_worktree,
        selected_worktree,
        worktrees,
    })
}

/// A persisted checkout observation that no longer authorizes reuse or launch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReconciliationUnavailable {
    Missing {
        path: PathBuf,
    },
    Discovery {
        path: PathBuf,
        detail: String,
    },
    IdentityChanged {
        expected_common_dir: PathBuf,
        observed_common_dir: PathBuf,
    },
    PathChanged {
        expected: PathBuf,
        observed: PathBuf,
    },
    BranchChanged {
        expected: Option<String>,
        observed: Option<String>,
    },
    Detached,
    LockedOrPrunable,
}

impl fmt::Display for ReconciliationUnavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing { path } => write!(f, "checkout {} is missing", path.display()),
            Self::Discovery { path, detail } => {
                write!(f, "cannot inspect checkout {}: {detail}", path.display())
            }
            Self::IdentityChanged {
                expected_common_dir,
                observed_common_dir,
            } => write!(
                f,
                "repository identity changed from {} to {}",
                expected_common_dir.display(),
                observed_common_dir.display()
            ),
            Self::PathChanged { expected, observed } => write!(
                f,
                "registered checkout path changed from {} to {}",
                expected.display(),
                observed.display()
            ),
            Self::BranchChanged { expected, observed } => {
                write!(
                    f,
                    "checkout branch changed from {expected:?} to {observed:?}"
                )
            }
            Self::Detached => write!(f, "checkout is detached"),
            Self::LockedOrPrunable => write!(f, "checkout is locked or prunable"),
        }
    }
}

impl std::error::Error for ReconciliationUnavailable {}

/// Rediscover and compare every Git-owned fact that authorizes checkout reuse.
pub fn reconcile_checkout(
    expected_common_dir: &Path,
    expected_path: &Path,
    expected_branch: Option<&str>,
) -> std::result::Result<RepositorySnapshot, ReconciliationUnavailable> {
    if !expected_path.exists() {
        return Err(ReconciliationUnavailable::Missing {
            path: expected_path.to_path_buf(),
        });
    }
    let snapshot = discover_repository(expected_path).map_err(|error| {
        ReconciliationUnavailable::Discovery {
            path: expected_path.to_path_buf(),
            detail: error.to_string(),
        }
    })?;
    if snapshot.common_dir != expected_common_dir {
        return Err(ReconciliationUnavailable::IdentityChanged {
            expected_common_dir: expected_common_dir.to_path_buf(),
            observed_common_dir: snapshot.common_dir.clone(),
        });
    }
    if snapshot.selected_worktree.path != expected_path {
        return Err(ReconciliationUnavailable::PathChanged {
            expected: expected_path.to_path_buf(),
            observed: snapshot.selected_worktree.path.clone(),
        });
    }
    if snapshot.selected_worktree.locked || snapshot.selected_worktree.prunable {
        return Err(ReconciliationUnavailable::LockedOrPrunable);
    }
    if snapshot.selected_worktree.detached {
        return Err(ReconciliationUnavailable::Detached);
    }
    if snapshot.selected_worktree.branch.as_deref() != expected_branch {
        return Err(ReconciliationUnavailable::BranchChanged {
            expected: expected_branch.map(str::to_owned),
            observed: snapshot.selected_worktree.branch.clone(),
        });
    }
    Ok(snapshot)
}

/// A locally verified remote default and its exact local/remote ref names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefaultBranch {
    pub remote: String,
    pub remote_ref: String,
    pub local_branch: String,
    pub local_ref: String,
}

/// Actionable reasons why local Git metadata cannot authorize a default checkout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DefaultBranchUnavailable {
    DetachedMainHead,
    UnbornMainHead {
        head_ref: String,
    },
    NoCandidate {
        remotes: Vec<String>,
    },
    MalformedMainHead {
        detail: String,
    },
    MalformedTarget {
        remote: String,
        target: String,
    },
    DanglingTarget {
        remote: String,
        target: String,
    },
    UnsupportedCommand {
        operation: &'static str,
        detail: String,
    },
}

impl fmt::Display for DefaultBranchUnavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DetachedMainHead => write!(
                f,
                "the main worktree is detached; check out a local branch and record a remote HEAD"
            ),
            Self::UnbornMainHead { head_ref } => write!(
                f,
                "the main worktree branch {head_ref} is unborn; create its first commit before admission"
            ),
            Self::NoCandidate { remotes } => write!(
                f,
                "no verified local remote HEAD was found (tried {}); fetch or set the remote HEAD explicitly",
                remotes.join(", ")
            ),
            Self::MalformedMainHead { detail } => {
                write!(f, "the main worktree HEAD is malformed: {detail}")
            }
            Self::MalformedTarget { remote, target } => write!(
                f,
                "remote {remote} HEAD points outside its remote-tracking namespace: {target}"
            ),
            Self::DanglingTarget { remote, target } => write!(
                f,
                "remote {remote} HEAD target {target} is missing; refresh local remote metadata"
            ),
            Self::UnsupportedCommand { operation, detail } => write!(
                f,
                "installed Git cannot perform {operation}: {detail}"
            ),
        }
    }
}

impl std::error::Error for DefaultBranchUnavailable {}

fn git_probe(
    repo: &Path,
    args: &[&OsStr],
    operation: &'static str,
) -> std::result::Result<Output, DefaultBranchUnavailable> {
    Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|error| DefaultBranchUnavailable::UnsupportedCommand {
            operation,
            detail: error.to_string(),
        })
}

fn utf8_line(
    stdout: &[u8],
    operation: &'static str,
) -> std::result::Result<String, DefaultBranchUnavailable> {
    let line = std::str::from_utf8(stdout).map_err(|error| {
        DefaultBranchUnavailable::UnsupportedCommand {
            operation,
            detail: format!("non-UTF-8 ref output: {error}"),
        }
    })?;
    let line = line.strip_suffix('\n').unwrap_or(line);
    let line = line.strip_suffix('\r').unwrap_or(line);
    if line.is_empty() || line.contains(['\r', '\n', '\0']) {
        return Err(DefaultBranchUnavailable::UnsupportedCommand {
            operation,
            detail: "empty or multi-line ref output".into(),
        });
    }
    Ok(line.to_owned())
}

fn unsupported_from_output(operation: &'static str, output: &Output) -> DefaultBranchUnavailable {
    DefaultBranchUnavailable::UnsupportedCommand {
        operation,
        detail: format!(
            "status {}: {}",
            output
                .status
                .code()
                .map_or_else(|| "signal".to_owned(), |code| code.to_string()),
            String::from_utf8_lossy(&output.stderr).trim()
        ),
    }
}

fn verify_commit(
    repo: &Path,
    reference: &str,
    operation: &'static str,
) -> std::result::Result<Output, DefaultBranchUnavailable> {
    let commit = format!("{reference}^{{commit}}");
    git_probe(
        repo,
        &[
            OsStr::new("rev-parse"),
            OsStr::new("--verify"),
            OsStr::new("--quiet"),
            OsStr::new("--end-of-options"),
            OsStr::new(&commit),
        ],
        operation,
    )
}

/// Resolve the default branch solely from verified local refs in the main worktree.
pub fn resolve_default_branch(
    snapshot: &RepositorySnapshot,
) -> std::result::Result<DefaultBranch, DefaultBranchUnavailable> {
    let main_record =
        snapshot
            .worktrees
            .first()
            .ok_or_else(|| DefaultBranchUnavailable::MalformedMainHead {
                detail: "worktree inventory is empty".into(),
            })?;
    if main_record.path != snapshot.main_worktree {
        return Err(DefaultBranchUnavailable::MalformedMainHead {
            detail: "the first inventory record is not the recorded main worktree".into(),
        });
    }
    if main_record.detached {
        return Err(DefaultBranchUnavailable::DetachedMainHead);
    }

    let symbolic = git_probe(
        &snapshot.main_worktree,
        &[
            OsStr::new("symbolic-ref"),
            OsStr::new("-q"),
            OsStr::new("HEAD"),
        ],
        "main HEAD symbolic-ref",
    )?;
    if !symbolic.status.success() {
        if symbolic.status.code() == Some(1) {
            return Err(DefaultBranchUnavailable::DetachedMainHead);
        }
        return Err(unsupported_from_output("main HEAD symbolic-ref", &symbolic));
    }
    let main_head = utf8_line(&symbolic.stdout, "main HEAD symbolic-ref")?;
    let branch_name = main_head
        .strip_prefix("refs/heads/")
        .filter(|name| !name.is_empty());
    if branch_name.is_none() || main_record.branch.as_deref() != Some(main_head.as_str()) {
        return Err(DefaultBranchUnavailable::MalformedMainHead {
            detail: format!(
                "symbolic HEAD {main_head} does not match inventory branch {:?}",
                main_record.branch
            ),
        });
    }

    let verified_main = verify_commit(
        &snapshot.main_worktree,
        &main_head,
        "verify main branch commit",
    )?;
    if !verified_main.status.success() {
        if verified_main.status.code() == Some(1) {
            return Err(DefaultBranchUnavailable::UnbornMainHead {
                head_ref: main_head,
            });
        }
        return Err(unsupported_from_output(
            "verify main branch commit",
            &verified_main,
        ));
    }

    let upstream = git_probe(
        &snapshot.main_worktree,
        &[
            OsStr::new("for-each-ref"),
            OsStr::new("--format=%(upstream:remotename)"),
            OsStr::new("--"),
            OsStr::new(&main_head),
        ],
        "resolve main upstream remote",
    )?;
    if !upstream.status.success() {
        return Err(unsupported_from_output(
            "resolve main upstream remote",
            &upstream,
        ));
    }
    let upstream = std::str::from_utf8(&upstream.stdout)
        .map_err(|error| DefaultBranchUnavailable::UnsupportedCommand {
            operation: "resolve main upstream remote",
            detail: error.to_string(),
        })?
        .trim_end_matches(['\r', '\n']);
    if upstream.contains(['\r', '\n', '\0']) {
        return Err(DefaultBranchUnavailable::UnsupportedCommand {
            operation: "resolve main upstream remote",
            detail: "multi-line remote output".into(),
        });
    }

    let mut remotes = Vec::with_capacity(2);
    if !upstream.is_empty() && upstream != "." {
        remotes.push(upstream.to_owned());
    }
    if !remotes.iter().any(|remote| remote == "origin") {
        remotes.push("origin".into());
    }

    let mut dangling = None;
    for remote in &remotes {
        let remote_head = format!("refs/remotes/{remote}/HEAD");
        let symbolic = git_probe(
            &snapshot.main_worktree,
            &[
                OsStr::new("symbolic-ref"),
                OsStr::new("-q"),
                OsStr::new(&remote_head),
            ],
            "remote HEAD symbolic-ref",
        )?;
        if !symbolic.status.success() {
            if symbolic.status.code() == Some(1) {
                continue;
            }
            return Err(unsupported_from_output(
                "remote HEAD symbolic-ref",
                &symbolic,
            ));
        }
        let target = utf8_line(&symbolic.stdout, "remote HEAD symbolic-ref")?;
        let prefix = format!("refs/remotes/{remote}/");
        let Some(local_branch) = target.strip_prefix(&prefix).filter(|name| !name.is_empty())
        else {
            return Err(DefaultBranchUnavailable::MalformedTarget {
                remote: remote.clone(),
                target,
            });
        };
        let local_branch = local_branch.to_owned();
        let verified = verify_commit(
            &snapshot.main_worktree,
            &target,
            "verify remote HEAD target",
        )?;
        if verified.status.success() {
            return Ok(DefaultBranch {
                remote: remote.clone(),
                remote_ref: target,
                local_branch: local_branch.clone(),
                local_ref: format!("refs/heads/{local_branch}"),
            });
        }
        if verified.status.code() != Some(1) {
            return Err(unsupported_from_output(
                "verify remote HEAD target",
                &verified,
            ));
        }
        dangling = Some((remote.clone(), target));
    }

    if let Some((remote, target)) = dangling {
        Err(DefaultBranchUnavailable::DanglingTarget { remote, target })
    } else {
        Err(DefaultBranchUnavailable::NoCandidate { remotes })
    }
}

/// Where an ensured default checkout came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DefaultWorktreeOutcome {
    Main(WorktreeRecord),
    ExistingLinked(WorktreeRecord),
    CreatedManaged(WorktreeRecord),
}

#[derive(Debug)]
pub enum EnsureDefaultWorktreeError {
    PathCollision(PathBuf),
    CreateParent {
        path: PathBuf,
        source: std::io::Error,
    },
    CommandStart(std::io::Error),
    GitCommand {
        status: Option<i32>,
        stderr: String,
    },
    Discovery(RepositoryDiscoveryError),
    Verification(String),
}

impl fmt::Display for EnsureDefaultWorktreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PathCollision(path) => write!(
                f,
                "managed worktree path collision at {}; the path is not registered by Git",
                path.display()
            ),
            Self::CreateParent { path, source } => {
                write!(
                    f,
                    "create managed worktree parent {}: {source}",
                    path.display()
                )
            }
            Self::CommandStart(source) => write!(f, "start Git worktree add: {source}"),
            Self::GitCommand { status, stderr } => write!(
                f,
                "Git worktree add failed (status {}): {stderr}",
                status.map_or_else(|| "signal".to_owned(), |code| code.to_string())
            ),
            Self::Discovery(error) => write!(f, "verify created worktree: {error}"),
            Self::Verification(detail) => {
                write!(f, "created worktree failed verification: {detail}")
            }
        }
    }
}

impl std::error::Error for EnsureDefaultWorktreeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::CreateParent { source, .. } | Self::CommandStart(source) => Some(source),
            Self::Discovery(error) => Some(error),
            _ => None,
        }
    }
}

/// Reuse or create the exact verified default checkout without changing the main checkout.
pub fn ensure_default_worktree(
    snapshot: &RepositorySnapshot,
    default: &DefaultBranch,
    managed_path: &Path,
) -> std::result::Result<DefaultWorktreeOutcome, EnsureDefaultWorktreeError> {
    if let Some(main) = snapshot.worktrees.first() {
        if main.branch.as_deref() == Some(default.local_ref.as_str()) {
            return Ok(DefaultWorktreeOutcome::Main(main.clone()));
        }
    }
    if let Some(linked) = snapshot
        .worktrees
        .iter()
        .skip(1)
        .find(|record| record.branch.as_deref() == Some(default.local_ref.as_str()))
    {
        return Ok(DefaultWorktreeOutcome::ExistingLinked(linked.clone()));
    }
    if managed_path.exists() {
        return Err(EnsureDefaultWorktreeError::PathCollision(
            managed_path.to_path_buf(),
        ));
    }
    if let Some(parent) = managed_path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| {
            EnsureDefaultWorktreeError::CreateParent {
                path: parent.to_path_buf(),
                source,
            }
        })?;
    }

    let local_commit = format!("{}^{{commit}}", default.local_ref);
    let local = Command::new("git")
        .arg("-C")
        .arg(&snapshot.main_worktree)
        .args([
            OsStr::new("rev-parse"),
            OsStr::new("--verify"),
            OsStr::new("--quiet"),
            OsStr::new("--end-of-options"),
            OsStr::new(&local_commit),
        ])
        .output()
        .map_err(EnsureDefaultWorktreeError::CommandStart)?;
    let mut command = Command::new("git");
    command.arg("-C").arg(&snapshot.main_worktree);
    if local.status.success() {
        command
            .args([OsStr::new("worktree"), OsStr::new("add"), OsStr::new("--")])
            .arg(managed_path)
            .arg(&default.local_branch);
    } else if local.status.code() == Some(1) {
        // The remote target was already commit-verified by resolve_default_branch.
        let source = format!("{}/{}", default.remote, default.local_branch);
        command
            .args([
                OsStr::new("worktree"),
                OsStr::new("add"),
                OsStr::new("--track"),
                OsStr::new("-b"),
                OsStr::new(&default.local_branch),
                OsStr::new("--"),
            ])
            .arg(managed_path)
            .arg(source);
    } else {
        return Err(EnsureDefaultWorktreeError::GitCommand {
            status: local.status.code(),
            stderr: String::from_utf8_lossy(&local.stderr).trim().to_owned(),
        });
    }
    let output = command
        .output()
        .map_err(EnsureDefaultWorktreeError::CommandStart)?;
    if !output.status.success() {
        return Err(EnsureDefaultWorktreeError::GitCommand {
            status: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }

    let fresh = discover_repository(managed_path).map_err(EnsureDefaultWorktreeError::Discovery)?;
    let canonical_managed =
        managed_path
            .canonicalize()
            .map_err(|source| EnsureDefaultWorktreeError::CreateParent {
                path: managed_path.to_path_buf(),
                source,
            })?;
    if fresh.common_dir != snapshot.common_dir {
        return Err(EnsureDefaultWorktreeError::Verification(
            "repository common directory changed".into(),
        ));
    }
    if fresh.selected_worktree.path != canonical_managed {
        return Err(EnsureDefaultWorktreeError::Verification(
            "selected path does not match the managed allocation".into(),
        ));
    }
    if fresh.selected_worktree.branch.as_deref() != Some(default.local_ref.as_str()) {
        return Err(EnsureDefaultWorktreeError::Verification(format!(
            "expected branch {}, observed {:?}",
            default.local_ref, fresh.selected_worktree.branch
        )));
    }
    Ok(DefaultWorktreeOutcome::CreatedManaged(
        fresh.selected_worktree,
    ))
}

/// Git-verified classification of one literal local branch request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BranchActivation {
    New {
        name: String,
        full_ref: String,
    },
    ExistingLocal {
        name: String,
        full_ref: String,
        oid: String,
    },
    Occupied {
        name: String,
        full_ref: String,
        record: WorktreeRecord,
    },
}

/// Observable result of a verified non-force worktree activation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BranchActivationOutcome {
    CreatedManaged(WorktreeRecord),
    ActivatedManaged(WorktreeRecord),
    Reused(WorktreeRecord),
}

#[derive(Debug)]
pub enum BranchActivationError {
    InvalidLiteral {
        name: String,
        detail: String,
    },
    RemoteOnly {
        name: String,
        refs: Vec<String>,
    },
    DefaultUnavailable(DefaultBranchUnavailable),
    DefaultLocalRefMissing(String),
    PathCollision(PathBuf),
    PathInspection {
        path: PathBuf,
        source: std::io::Error,
    },
    CreateParent {
        path: PathBuf,
        source: std::io::Error,
    },
    CommandStart {
        operation: &'static str,
        source: std::io::Error,
    },
    GitCommand {
        operation: &'static str,
        status: Option<i32>,
        stderr: String,
    },
    Discovery(RepositoryDiscoveryError),
    Verification(String),
    PostAddCompensationFailed {
        repository: PathBuf,
        path: PathBuf,
        branch: String,
        created_branch: bool,
        verification: Box<BranchActivationError>,
        compensation: String,
    },
}

impl fmt::Display for BranchActivationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLiteral { name, detail } => {
                write!(f, "invalid literal branch {name:?}: {detail}")
            }
            Self::RemoteOnly { name, refs } => write!(
                f,
                "branch {name:?} exists only as remote refs ({}); create an explicit local branch first",
                refs.join(", ")
            ),
            Self::DefaultUnavailable(error) => write!(f, "resolve repository default: {error}"),
            Self::DefaultLocalRefMissing(reference) => write!(
                f,
                "verified default local ref {reference} is missing; reopen the repository primary first"
            ),
            Self::PathCollision(path) => {
                write!(f, "managed branch path collision at {}", path.display())
            }
            Self::PathInspection { path, source } => {
                write!(f, "inspect managed branch path {}: {source}", path.display())
            }
            Self::CreateParent { path, source } => {
                write!(f, "create managed branch parent {}: {source}", path.display())
            }
            Self::CommandStart { operation, source } => {
                write!(f, "start Git {operation}: {source}")
            }
            Self::GitCommand {
                operation,
                status,
                stderr,
            } => write!(
                f,
                "Git {operation} failed (status {}): {stderr}",
                status.map_or_else(|| "signal".to_owned(), |code| code.to_string())
            ),
            Self::Discovery(error) => write!(f, "discover branch worktree: {error}"),
            Self::Verification(detail) => write!(f, "verify branch activation: {detail}"),
            Self::PostAddCompensationFailed {
                repository,
                path,
                branch,
                created_branch,
                verification,
                compensation,
            } => write!(
                f,
                "post-add activation verification failed for {branch} at {} in {} (created branch: {created_branch}): {verification}; compensation failed: {compensation}",
                path.display(),
                repository.display()
            ),
        }
    }
}

impl std::error::Error for BranchActivationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::DefaultUnavailable(error) => Some(error),
            Self::PathInspection { source, .. }
            | Self::CreateParent { source, .. }
            | Self::CommandStart { source, .. } => Some(source),
            Self::Discovery(error) => Some(error),
            Self::PostAddCompensationFailed { verification, .. } => Some(verification),
            _ => None,
        }
    }
}

fn activation_command(
    repo: &Path,
    operation: &'static str,
    args: &[&OsStr],
) -> std::result::Result<Output, BranchActivationError> {
    Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|source| BranchActivationError::CommandStart { operation, source })
}

fn new_branch_add_arguments(name: &str, path: &Path, start_ref: &str) -> Vec<OsString> {
    vec![
        OsString::from("worktree"),
        OsString::from("add"),
        OsString::from("-b"),
        OsString::from(name),
        OsString::from("--"),
        path.as_os_str().to_owned(),
        OsString::from(start_ref),
    ]
}

fn existing_branch_add_arguments(name: &str, path: &Path) -> Vec<OsString> {
    vec![
        OsString::from("worktree"),
        OsString::from("add"),
        OsString::from("--"),
        path.as_os_str().to_owned(),
        OsString::from(name),
    ]
}

fn candidate_inventory_path(path: &Path) -> std::result::Result<PathBuf, BranchActivationError> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => return Err(BranchActivationError::PathCollision(path.to_path_buf())),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(BranchActivationError::PathInspection {
                path: path.to_path_buf(),
                source,
            });
        }
    }
    let parent = path.parent().ok_or_else(|| {
        BranchActivationError::Verification("managed branch path has no parent".into())
    })?;
    let name = path.file_name().ok_or_else(|| {
        BranchActivationError::Verification("managed branch path has no final component".into())
    })?;
    match parent.canonicalize() {
        Ok(parent) => Ok(parent.join(name)),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(path.to_path_buf()),
        Err(source) => Err(BranchActivationError::PathInspection {
            path: parent.to_path_buf(),
            source,
        }),
    }
}

fn require_unoccupied_candidate(
    snapshot: &RepositorySnapshot,
    path: &Path,
) -> std::result::Result<(), BranchActivationError> {
    let candidate = candidate_inventory_path(path)?;
    if snapshot
        .worktrees
        .iter()
        .any(|record| record.path == candidate || record.path == path)
    {
        return Err(BranchActivationError::PathCollision(path.to_path_buf()));
    }
    Ok(())
}

fn activation_add_command(
    repo: &Path,
    operation: &'static str,
    args: &[OsString],
) -> std::result::Result<Output, BranchActivationError> {
    Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|source| BranchActivationError::CommandStart { operation, source })
}

fn activation_oid(
    repo: &Path,
    reference: &str,
    operation: &'static str,
) -> std::result::Result<String, BranchActivationError> {
    let commit = format!("{reference}^{{commit}}");
    let output = activation_command(
        repo,
        operation,
        &[
            OsStr::new("rev-parse"),
            OsStr::new("--verify"),
            OsStr::new("--quiet"),
            OsStr::new("--end-of-options"),
            OsStr::new(&commit),
        ],
    )?;
    if !output.status.success() {
        return Err(BranchActivationError::GitCommand {
            operation,
            status: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    let oid = std::str::from_utf8(&output.stdout)
        .map_err(|error| BranchActivationError::Verification(error.to_string()))?
        .trim_end_matches(['\r', '\n']);
    if oid.is_empty() || oid.contains(['\r', '\n', '\0']) {
        return Err(BranchActivationError::Verification(format!(
            "Git {operation} returned malformed object id"
        )));
    }
    Ok(oid.to_owned())
}

/// Validate a branch as literal text and classify only its exact local ref.
pub fn classify_branch(
    snapshot: &RepositorySnapshot,
    literal: &str,
) -> std::result::Result<BranchActivation, BranchActivationError> {
    // `check-ref-format --branch` accepts lone `@` even though Git forbids it as
    // a ref name. Lifecycle state stores an exact refs/heads identity, so refuse
    // the one porcelain-only spelling before invoking any allocating behavior.
    if literal == "@" {
        return Err(BranchActivationError::InvalidLiteral {
            name: literal.to_owned(),
            detail: "lone @ is not a durable Git ref name".into(),
        });
    }
    let checked = activation_command(
        &snapshot.main_worktree,
        "check branch literal",
        &[
            OsStr::new("check-ref-format"),
            OsStr::new("--branch"),
            OsStr::new(literal),
        ],
    )?;
    if !checked.status.success() {
        return Err(BranchActivationError::InvalidLiteral {
            name: literal.to_owned(),
            detail: String::from_utf8_lossy(&checked.stderr).trim().to_owned(),
        });
    }
    let validated = std::str::from_utf8(&checked.stdout)
        .map_err(|error| BranchActivationError::InvalidLiteral {
            name: literal.to_owned(),
            detail: format!("non-UTF-8 validation output: {error}"),
        })?
        .trim_end_matches(['\r', '\n']);
    if validated != literal || validated.contains(['\r', '\n', '\0']) {
        return Err(BranchActivationError::InvalidLiteral {
            name: literal.to_owned(),
            detail: "Git expanded or altered the requested branch".into(),
        });
    }

    let full_ref = format!("refs/heads/{literal}");
    let local = activation_command(
        &snapshot.main_worktree,
        "find exact local branch",
        &[
            OsStr::new("show-ref"),
            OsStr::new("--verify"),
            OsStr::new("--quiet"),
            OsStr::new("--"),
            OsStr::new(&full_ref),
        ],
    )?;
    if local.status.success() {
        if let Some(record) = snapshot
            .worktrees
            .iter()
            .find(|record| record.branch.as_deref() == Some(full_ref.as_str()))
        {
            return Ok(BranchActivation::Occupied {
                name: literal.to_owned(),
                full_ref,
                record: record.clone(),
            });
        }
        let oid = activation_oid(
            &snapshot.main_worktree,
            &full_ref,
            "verify exact local branch commit",
        )?;
        return Ok(BranchActivation::ExistingLocal {
            name: literal.to_owned(),
            full_ref,
            oid,
        });
    }
    if local.status.code() != Some(1) {
        return Err(BranchActivationError::GitCommand {
            operation: "find exact local branch",
            status: local.status.code(),
            stderr: String::from_utf8_lossy(&local.stderr).trim().to_owned(),
        });
    }

    let remotes = activation_command(
        &snapshot.main_worktree,
        "find matching remote branches",
        &[
            OsStr::new("for-each-ref"),
            OsStr::new("--format=%(refname)"),
            OsStr::new("refs/remotes"),
        ],
    )?;
    if !remotes.status.success() {
        return Err(BranchActivationError::GitCommand {
            operation: "find matching remote branches",
            status: remotes.status.code(),
            stderr: String::from_utf8_lossy(&remotes.stderr).trim().to_owned(),
        });
    }
    let suffix = format!("/{literal}");
    let matching: Vec<String> = std::str::from_utf8(&remotes.stdout)
        .map_err(|error| BranchActivationError::Verification(error.to_string()))?
        .lines()
        .filter(|reference| {
            reference.starts_with("refs/remotes/")
                && reference.ends_with(&suffix)
                && !reference.ends_with("/HEAD")
        })
        .map(str::to_owned)
        .collect();
    if !matching.is_empty() {
        return Err(BranchActivationError::RemoteOnly {
            name: literal.to_owned(),
            refs: matching,
        });
    }
    Ok(BranchActivation::New {
        name: literal.to_owned(),
        full_ref,
    })
}

/// Rediscover, classify, and add or reuse one branch worktree without force or guessing.
pub fn activate_branch(
    repository_child: &Path,
    literal: &str,
    managed_path: &Path,
) -> std::result::Result<BranchActivationOutcome, BranchActivationError> {
    activate_branch_with_post_add_hook(repository_child, literal, managed_path, || {})
}

fn activate_branch_with_post_add_hook(
    repository_child: &Path,
    literal: &str,
    managed_path: &Path,
    after_add: impl FnOnce(),
) -> std::result::Result<BranchActivationOutcome, BranchActivationError> {
    let snapshot =
        discover_repository(repository_child).map_err(BranchActivationError::Discovery)?;
    let activation = classify_branch(&snapshot, literal)?;
    if let BranchActivation::Occupied { record, .. } = activation {
        return Ok(BranchActivationOutcome::Reused(record));
    }
    require_unoccupied_candidate(&snapshot, managed_path)?;
    if let Some(parent) = managed_path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| BranchActivationError::CreateParent {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    // Parent creation can expose a different canonical spelling, and another
    // process may have changed worktree topology since the request began. Use a
    // complete fresh inventory and exact branch classification at the last
    // decision point before Git's own non-force add safeguard.
    let fresh =
        discover_repository(&snapshot.main_worktree).map_err(BranchActivationError::Discovery)?;
    if fresh.common_dir != snapshot.common_dir {
        return Err(BranchActivationError::Verification(
            "repository identity changed before branch activation".into(),
        ));
    }
    let activation = classify_branch(&fresh, literal)?;
    if let BranchActivation::Occupied { record, .. } = activation {
        return Ok(BranchActivationOutcome::Reused(record));
    }
    require_unoccupied_candidate(&fresh, managed_path)?;

    let (name, full_ref, expected_oid, created) = match activation {
        BranchActivation::New { name, full_ref } => {
            let default = resolve_default_branch(&fresh)
                .map_err(BranchActivationError::DefaultUnavailable)?;
            let expected_oid = activation_oid(
                &fresh.main_worktree,
                &default.local_ref,
                "verify default local branch commit",
            )
            .map_err(|error| match error {
                BranchActivationError::GitCommand {
                    status: Some(1), ..
                } => BranchActivationError::DefaultLocalRefMissing(default.local_ref.clone()),
                other => other,
            })?;
            // Use the already captured commit, not the mutable default ref, so
            // a concurrent default-branch update cannot change the add base.
            let arguments = new_branch_add_arguments(&name, managed_path, &expected_oid);
            let output =
                activation_add_command(&fresh.main_worktree, "create branch worktree", &arguments)?;
            if !output.status.success() {
                return Err(BranchActivationError::GitCommand {
                    operation: "create branch worktree",
                    status: output.status.code(),
                    stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
                });
            }
            (name, full_ref, expected_oid, true)
        }
        BranchActivation::ExistingLocal {
            name,
            full_ref,
            oid,
        } => {
            // Git 2.50 detaches when given refs/heads/<name> here. The exact full
            // ref was classified and commit-verified above; pass its literal short
            // branch spelling so the resulting worktree remains attached.
            let arguments = existing_branch_add_arguments(&name, managed_path);
            let output = activation_add_command(
                &fresh.main_worktree,
                "activate local branch worktree",
                &arguments,
            )?;
            if !output.status.success() {
                return Err(BranchActivationError::GitCommand {
                    operation: "activate local branch worktree",
                    status: output.status.code(),
                    stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
                });
            }
            (name, full_ref, oid, false)
        }
        BranchActivation::Occupied { .. } => unreachable!("occupied returned above"),
    };
    after_add();

    // Git add is the commit boundary. Every subsequent failure compensates
    // through plain Git removal before the error escapes durable ownership.
    let verified = (|| {
        let fresh = discover_repository(managed_path).map_err(BranchActivationError::Discovery)?;
        let canonical =
            managed_path
                .canonicalize()
                .map_err(|source| BranchActivationError::CreateParent {
                    path: managed_path.to_path_buf(),
                    source,
                })?;
        if fresh.common_dir != snapshot.common_dir
            || fresh.selected_worktree.path != canonical
            || fresh.selected_worktree.branch.as_deref() != Some(full_ref.as_str())
        {
            return Err(BranchActivationError::Verification(format!(
                "expected {full_ref} at {}, observed {:?}",
                canonical.display(),
                fresh.selected_worktree
            )));
        }
        let observed_oid = activation_oid(&canonical, "HEAD", "verify activated branch commit")?;
        if observed_oid != expected_oid {
            return Err(BranchActivationError::Verification(format!(
                "branch {name} changed from {expected_oid} to {observed_oid}"
            )));
        }
        Ok((fresh, canonical))
    })();
    let (fresh, _) = match verified {
        Ok(verified) => verified,
        Err(error) => {
            if let Err(compensation) = remove_added_worktree(&snapshot.main_worktree, managed_path)
            {
                return Err(BranchActivationError::PostAddCompensationFailed {
                    repository: snapshot.main_worktree.clone(),
                    path: managed_path.to_path_buf(),
                    branch: name,
                    created_branch: created,
                    verification: Box::new(error),
                    compensation: compensation.to_string(),
                });
            }
            return Err(error);
        }
    };
    if created {
        Ok(BranchActivationOutcome::CreatedManaged(
            fresh.selected_worktree,
        ))
    } else {
        Ok(BranchActivationOutcome::ActivatedManaged(
            fresh.selected_worktree,
        ))
    }
}

fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(anyhow!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

pub fn repo_root(path: &Path) -> Option<PathBuf> {
    git(path, &["rev-parse", "--show-toplevel"])
        .ok()
        .map(PathBuf::from)
}

fn worktrees_base() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".local").join("share")))
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("baude")
        .join("worktrees")
}

/// Deterministic workspace-local allocation for a durable primary checkout.
/// The opaque keys are scoped by the active workspace state file, so include
/// the workspace name to keep independent backends from sharing a path.
pub fn managed_default_worktree_path(repository_key: u64, checkout_key: u64) -> PathBuf {
    worktrees_base()
        .join(&crate::workspace::active().name)
        .join(format!("repository-{repository_key}"))
        .join(format!("primary-{checkout_key}"))
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '.' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// Stable workspace-local path for a durable managed branch checkout.
pub fn managed_branch_worktree_path(
    repository_key: u64,
    checkout_key: u64,
    branch: &str,
) -> PathBuf {
    let sanitized = sanitize(branch);
    let mut label = String::new();
    for character in sanitized.chars() {
        if label.len() + character.len_utf8() > 48 {
            break;
        }
        label.push(character);
    }
    if label.is_empty() {
        label.push_str("branch");
    }
    worktrees_base()
        .join(&crate::workspace::active().name)
        .join(format!("repository-{repository_key}"))
        .join(format!("{label}-{checkout_key}"))
}

/// A repo named for cloning: where it lives and the URL to clone with.
pub struct CloneTarget {
    pub host: String,
    pub owner: String,
    pub repo: String,
    /// Canonical URL handed to `git clone` — scp-style ssh unless the input
    /// was an `http(s)://` URL, which keeps https.
    pub url: String,
}

/// Parse user input naming a repo to clone. Accepts scp-style ssh
/// (`git@github.com:owner/repo.git`), `ssh://`/`http(s)://` URLs (browser
/// URLs with trailing segments like `/tree/main` are tolerated), a
/// scheme-less `host/owner/repo`, or the `owner/repo` shorthand (assumes
/// github.com). Everything except pasted https keeps ssh, so unattended
/// pushes ride the usual ssh auth.
pub fn parse_clone_target(input: &str) -> Option<CloneTarget> {
    let input = input.trim().split(['?', '#']).next()?.trim_end_matches('/');
    // Strip a trailing `.git` and reject empty/nested segments.
    let clean = |s: &str| {
        let s = s.strip_suffix(".git").unwrap_or(s);
        (!s.is_empty() && !s.contains('/')).then(|| s.to_string())
    };
    let ssh_url = |h: &str, o: &str, r: &str| format!("git@{h}:{o}/{r}.git");
    if let Some(rest) = input.strip_prefix("git@") {
        let (host, path) = rest.split_once(':')?;
        let mut seg = path.split('/');
        let (owner, repo) = (clean(seg.next()?)?, clean(seg.next()?)?);
        return Some(CloneTarget {
            url: ssh_url(host, &owner, &repo),
            host: host.to_string(),
            owner,
            repo,
        });
    }
    if let Some((scheme, rest)) = input.split_once("://") {
        let rest = rest.split_once('@').map_or(rest, |(_, r)| r);
        let mut seg = rest.split('/');
        let host = seg.next()?.split(':').next()?.to_string();
        if host.is_empty() {
            return None;
        }
        let (owner, repo) = (clean(seg.next()?)?, clean(seg.next()?)?);
        let url = match scheme {
            "http" | "https" => format!("https://{host}/{owner}/{repo}.git"),
            _ => ssh_url(&host, &owner, &repo),
        };
        return Some(CloneTarget {
            host,
            owner,
            repo,
            url,
        });
    }
    let segs: Vec<&str> = input.split('/').collect();
    let (host, owner, repo) = match segs.as_slice() {
        [owner, repo] => ("github.com".to_string(), clean(owner)?, clean(repo)?),
        [host, owner, repo, ..] if host.contains('.') => {
            (host.to_string(), clean(owner)?, clean(repo)?)
        }
        _ => return None,
    };
    Some(CloneTarget {
        url: ssh_url(&host, &owner, &repo),
        host,
        owner,
        repo,
    })
}

/// Clone `url` into `dest`, creating parent directories. Blocking — callers
/// run it off the UI thread.
pub fn clone_repo(url: &str, dest: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let out = Command::new("git")
        .args(["clone", "--", url])
        .arg(dest)
        .output()?;
    if out.status.success() {
        Ok(())
    } else {
        Err(anyhow!(
            "git clone: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// A conclusive reason why a linked worktree cannot be removed safely.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RemovalBlocker {
    NotManaged,
    MainWorktree,
    NotLinked,
    Detached,
    Locked,
    Prunable,
    IdentityChanged,
    PathChanged,
    BranchChanged,
    StagedAdd { path: Vec<u8> },
    StagedDelete { path: Vec<u8> },
    StagedRename { path: Vec<u8> },
    StagedModification { path: Vec<u8> },
    UnstagedModification { path: Vec<u8> },
    UnstagedDelete { path: Vec<u8> },
    Conflict { path: Vec<u8> },
    Untracked { path: Vec<u8> },
    Ignored { path: Vec<u8> },
    SubmoduleChange { path: Vec<u8> },
    SubmodulePresent { path: Vec<u8>, status: u8 },
}

/// Git facts captured only after every removal inspection succeeds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedRemovalTarget {
    path: PathBuf,
    common_dir: PathBuf,
    main_worktree: PathBuf,
    branch_ref: String,
    branch_oid: String,
}

impl VerifiedRemovalTarget {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn common_dir(&self) -> &Path {
        &self.common_dir
    }

    pub fn main_worktree(&self) -> &Path {
        &self.main_worktree
    }

    pub fn branch_ref(&self) -> &str {
        &self.branch_ref
    }

    pub fn branch_oid(&self) -> &str {
        &self.branch_oid
    }
}

/// Removal is either conclusively blocked or carries an opaque verified target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RemovalSafety {
    Safe(VerifiedRemovalTarget),
    Blocked(Vec<RemovalBlocker>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedRemoval {
    removed_path: PathBuf,
    branch_ref: String,
    branch_oid: String,
}

impl VerifiedRemoval {
    pub fn removed_path(&self) -> &Path {
        &self.removed_path
    }

    pub fn branch_ref(&self) -> &str {
        &self.branch_ref
    }

    pub fn branch_oid(&self) -> &str {
        &self.branch_oid
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RemovalPostconditionFailure {
    RepositoryUnavailable(String),
    ParentIdentityChanged,
    InventoryStillPresent(PathBuf),
    PathPresent(PathBuf),
    PathInspection { path: PathBuf, detail: String },
    BranchUnavailable(String),
    BranchChanged { expected: String, observed: String },
}

#[derive(Debug)]
pub enum RemoveVerifiedError {
    CommandStart(std::io::Error),
    GitRefused { status: Option<i32>, stderr: String },
    Postcondition(RemovalPostconditionFailure),
}

impl fmt::Display for RemoveVerifiedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CommandStart(error) => write!(f, "start Git worktree removal: {error}"),
            Self::GitRefused { status, stderr } => write!(
                f,
                "plain Git worktree removal refused (status {}): {stderr}",
                status.map_or_else(|| "signal".to_owned(), |code| code.to_string())
            ),
            Self::Postcondition(failure) => {
                write!(f, "worktree removal postcondition failed: {failure:?}")
            }
        }
    }
}

impl std::error::Error for RemoveVerifiedError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::CommandStart(error) => Some(error),
            _ => None,
        }
    }
}

/// External inspection failed, so removal authorization cannot be issued.
#[derive(Debug)]
pub enum InspectionError {
    CommandStart {
        operation: &'static str,
        source: std::io::Error,
    },
    GitCommand {
        operation: &'static str,
        status: Option<i32>,
        stderr: String,
    },
    MalformedStatus(String),
    MalformedSubmoduleStatus(String),
    Topology(RepositoryDiscoveryError),
    Canonicalize {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl fmt::Display for InspectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CommandStart { operation, source } => {
                write!(f, "start Git {operation}: {source}")
            }
            Self::GitCommand {
                operation,
                status,
                stderr,
            } => write!(
                f,
                "Git {operation} failed (status {}): {stderr}",
                status.map_or_else(|| "signal".to_owned(), |code| code.to_string())
            ),
            Self::MalformedStatus(reason) => write!(f, "malformed Git status: {reason}"),
            Self::MalformedSubmoduleStatus(reason) => {
                write!(f, "malformed Git submodule status: {reason}")
            }
            Self::Topology(error) => write!(f, "inspect worktree topology: {error}"),
            Self::Canonicalize { path, source } => {
                write!(
                    f,
                    "canonicalize removal target {}: {source}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for InspectionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::CommandStart { source, .. } | Self::Canonicalize { source, .. } => Some(source),
            Self::Topology(error) => Some(error),
            _ => None,
        }
    }
}

fn removal_status_arguments() -> [&'static OsStr; 7] {
    [
        OsStr::new("--no-optional-locks"),
        OsStr::new("status"),
        OsStr::new("--porcelain=v2"),
        OsStr::new("-z"),
        OsStr::new("--untracked-files=all"),
        OsStr::new("--ignore-submodules=none"),
        OsStr::new("--ignored=matching"),
    ]
}

fn valid_xy(xy: &[u8]) -> bool {
    xy.len() == 2 && xy.iter().all(|byte| b".MADRCUT".contains(byte))
}

fn valid_submodule_field(field: &[u8]) -> bool {
    field == b"N..."
        || (field.len() == 4
            && field[0] == b'S'
            && b".C".contains(&field[1])
            && b".M".contains(&field[2])
            && b".U".contains(&field[3]))
}

fn valid_mode(field: &[u8]) -> bool {
    field.len() == 6 && field.iter().all(|byte| b"01234567".contains(byte))
}

fn valid_oid(field: &[u8]) -> bool {
    !field.is_empty() && field.iter().all(u8::is_ascii_hexdigit)
}

fn malformed_status(reason: impl Into<String>) -> InspectionError {
    InspectionError::MalformedStatus(reason.into())
}

fn tracked_blockers(
    xy: &[u8],
    submodule: &[u8],
    path: &[u8],
) -> std::result::Result<Vec<RemovalBlocker>, InspectionError> {
    if !valid_xy(xy) {
        return Err(malformed_status("invalid XY field"));
    }
    if !valid_submodule_field(submodule) {
        return Err(malformed_status("invalid submodule field"));
    }
    if path.is_empty() {
        return Err(malformed_status("tracked record has an empty path"));
    }

    let path = path.to_vec();
    let mut blockers = Vec::new();
    match xy[0] {
        b'.' => {}
        b'A' => blockers.push(RemovalBlocker::StagedAdd { path: path.clone() }),
        b'D' => blockers.push(RemovalBlocker::StagedDelete { path: path.clone() }),
        b'R' => blockers.push(RemovalBlocker::StagedRename { path: path.clone() }),
        b'U' => blockers.push(RemovalBlocker::Conflict { path: path.clone() }),
        b'M' | b'C' | b'T' => {
            blockers.push(RemovalBlocker::StagedModification { path: path.clone() })
        }
        _ => return Err(malformed_status("unsupported index status")),
    }
    match xy[1] {
        b'.' => {}
        b'D' => blockers.push(RemovalBlocker::UnstagedDelete { path: path.clone() }),
        b'M' | b'T' => blockers.push(RemovalBlocker::UnstagedModification { path: path.clone() }),
        b'A' | b'R' | b'C' | b'U' => blockers.push(RemovalBlocker::Conflict { path: path.clone() }),
        _ => return Err(malformed_status("unsupported worktree status")),
    }
    if submodule != b"N..." {
        blockers.push(RemovalBlocker::SubmoduleChange { path });
    }
    if blockers.is_empty() {
        return Err(malformed_status("tracked record reports no change"));
    }
    Ok(blockers)
}

fn parse_removal_status(bytes: &[u8]) -> std::result::Result<Vec<RemovalBlocker>, InspectionError> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    if !bytes.ends_with(b"\0") {
        return Err(malformed_status("output is not NUL terminated"));
    }

    let fields: Vec<&[u8]> = bytes[..bytes.len() - 1].split(|byte| *byte == 0).collect();
    let mut blockers = Vec::new();
    let mut index = 0;
    while index < fields.len() {
        let record = fields[index];
        if record.is_empty() {
            return Err(malformed_status("empty status record"));
        }
        if let Some(path) = record.strip_prefix(b"? ") {
            if path.is_empty() {
                return Err(malformed_status("untracked record has an empty path"));
            }
            blockers.push(RemovalBlocker::Untracked {
                path: path.to_vec(),
            });
        } else if let Some(path) = record.strip_prefix(b"! ") {
            if path.is_empty() {
                return Err(malformed_status("ignored record has an empty path"));
            }
            blockers.push(RemovalBlocker::Ignored {
                path: path.to_vec(),
            });
        } else if record.starts_with(b"1 ") {
            let parts: Vec<_> = record.splitn(9, |byte| *byte == b' ').collect();
            if parts.len() != 9
                || parts[0] != b"1"
                || !parts[3..=5].iter().all(|field| valid_mode(field))
                || !parts[6..=7].iter().all(|field| valid_oid(field))
            {
                return Err(malformed_status("invalid ordinary tracked record"));
            }
            blockers.extend(tracked_blockers(parts[1], parts[2], parts[8])?);
        } else if record.starts_with(b"2 ") {
            let parts: Vec<_> = record.splitn(10, |byte| *byte == b' ').collect();
            if parts.len() != 10
                || parts[0] != b"2"
                || !parts[3..=5].iter().all(|field| valid_mode(field))
                || !parts[6..=7].iter().all(|field| valid_oid(field))
                || parts[8].len() < 2
                || !b"RC".contains(&parts[8][0])
                || !parts[8][1..].iter().all(u8::is_ascii_digit)
            {
                return Err(malformed_status("invalid renamed or copied record"));
            }
            index += 1;
            if index >= fields.len() || fields[index].is_empty() {
                return Err(malformed_status("renamed record has no original path"));
            }
            blockers.extend(tracked_blockers(parts[1], parts[2], parts[9])?);
        } else if record.starts_with(b"u ") {
            let parts: Vec<_> = record.splitn(11, |byte| *byte == b' ').collect();
            if parts.len() != 11
                || parts[0] != b"u"
                || !valid_xy(parts[1])
                || !valid_submodule_field(parts[2])
                || !parts[3..=6].iter().all(|field| valid_mode(field))
                || !parts[7..=9].iter().all(|field| valid_oid(field))
                || parts[10].is_empty()
            {
                return Err(malformed_status("invalid unmerged record"));
            }
            blockers.push(RemovalBlocker::Conflict {
                path: parts[10].to_vec(),
            });
            if parts[2] != b"N..." {
                blockers.push(RemovalBlocker::SubmoduleChange {
                    path: parts[10].to_vec(),
                });
            }
        } else {
            return Err(malformed_status("unknown status record kind"));
        }
        index += 1;
    }
    Ok(blockers)
}

fn inspect_removal_status_with_program(
    worktree: &Path,
    program: &OsStr,
) -> std::result::Result<Vec<RemovalBlocker>, InspectionError> {
    let arguments = removal_status_arguments();
    let output = Command::new(program)
        .arg(arguments[0])
        .arg("-C")
        .arg(worktree)
        .args(&arguments[1..])
        .output()
        .map_err(|source| InspectionError::CommandStart {
            operation: "removal status inspection",
            source,
        })?;
    if !output.status.success() {
        return Err(InspectionError::GitCommand {
            operation: "removal status inspection",
            status: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    parse_removal_status(&output.stdout)
}

fn inspect_removal_status(
    worktree: &Path,
) -> std::result::Result<Vec<RemovalBlocker>, InspectionError> {
    inspect_removal_status_with_program(worktree, OsStr::new("git"))
}

fn parse_submodule_status(
    bytes: &[u8],
) -> std::result::Result<Vec<RemovalBlocker>, InspectionError> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    if !bytes.ends_with(b"\n") {
        return Err(InspectionError::MalformedSubmoduleStatus(
            "output is not newline terminated".into(),
        ));
    }

    let mut blockers = Vec::new();
    for row in bytes[..bytes.len() - 1].split(|byte| *byte == b'\n') {
        if row.len() < 43 || !b" -+U".contains(&row[0]) {
            return Err(InspectionError::MalformedSubmoduleStatus(
                "invalid row prefix".into(),
            ));
        }
        let oid_end = row[1..]
            .iter()
            .position(|byte| *byte == b' ')
            .map(|position| position + 1)
            .ok_or_else(|| {
                InspectionError::MalformedSubmoduleStatus("row has no path separator".into())
            })?;
        let oid = &row[1..oid_end];
        if !matches!(oid.len(), 40 | 64) || !oid.iter().all(u8::is_ascii_hexdigit) {
            return Err(InspectionError::MalformedSubmoduleStatus(
                "row has an invalid object ID".into(),
            ));
        }
        let remainder = &row[oid_end + 1..];
        if remainder.is_empty() {
            return Err(InspectionError::MalformedSubmoduleStatus(
                "row has an empty path".into(),
            ));
        }
        let path_end = remainder
            .windows(2)
            .rposition(|window| window == b" (")
            .unwrap_or(remainder.len());
        let path = &remainder[..path_end];
        if path.is_empty() {
            return Err(InspectionError::MalformedSubmoduleStatus(
                "row has an empty path".into(),
            ));
        }
        blockers.push(RemovalBlocker::SubmodulePresent {
            path: path.to_vec(),
            status: row[0],
        });
    }
    Ok(blockers)
}

fn inspect_submodules_with_program(
    worktree: &Path,
    program: &OsStr,
) -> std::result::Result<Vec<RemovalBlocker>, InspectionError> {
    let output = Command::new(program)
        .arg("-C")
        .arg(worktree)
        .args(["submodule", "status", "--recursive"])
        .output()
        .map_err(|source| InspectionError::CommandStart {
            operation: "recursive submodule inspection",
            source,
        })?;
    if !output.status.success() {
        return Err(InspectionError::GitCommand {
            operation: "recursive submodule inspection",
            status: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    parse_submodule_status(&output.stdout)
}

fn inspect_submodules(
    worktree: &Path,
) -> std::result::Result<Vec<RemovalBlocker>, InspectionError> {
    inspect_submodules_with_program(worktree, OsStr::new("git"))
}

fn removal_oid(worktree: &Path, revision: &OsStr) -> std::result::Result<String, InspectionError> {
    let output = Command::new("git")
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(worktree)
        .args([OsStr::new("rev-parse"), OsStr::new("--verify")])
        .arg(revision)
        .output()
        .map_err(|source| InspectionError::CommandStart {
            operation: "removal branch OID inspection",
            source,
        })?;
    if !output.status.success() {
        return Err(InspectionError::GitCommand {
            operation: "removal branch OID inspection",
            status: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    let oid = std::str::from_utf8(&output.stdout)
        .map_err(|_| InspectionError::MalformedStatus("branch OID is not UTF-8".into()))?
        .trim_end_matches(['\r', '\n']);
    if !matches!(oid.len(), 40 | 64) || !oid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(InspectionError::MalformedStatus(
            "branch OID is not a full object ID".into(),
        ));
    }
    Ok(oid.to_owned())
}

fn topology_blockers(
    expected_common_dir: &Path,
    expected_path: &Path,
    expected_branch: Option<&str>,
    managed_by_baude: bool,
    snapshot: &RepositorySnapshot,
) -> Vec<RemovalBlocker> {
    let mut blockers = Vec::new();
    if !managed_by_baude {
        blockers.push(RemovalBlocker::NotManaged);
    }
    if snapshot.common_dir != expected_common_dir {
        blockers.push(RemovalBlocker::IdentityChanged);
    }
    if snapshot.selected_worktree.path != expected_path {
        blockers.push(RemovalBlocker::PathChanged);
    }
    if snapshot.selected_worktree.path == snapshot.main_worktree {
        blockers.push(RemovalBlocker::MainWorktree);
    }
    if snapshot.selected_worktree.bare {
        blockers.push(RemovalBlocker::NotLinked);
    }
    if snapshot.selected_worktree.detached {
        blockers.push(RemovalBlocker::Detached);
    }
    if snapshot.selected_worktree.branch.as_deref() != expected_branch {
        blockers.push(RemovalBlocker::BranchChanged);
    }
    if snapshot.selected_worktree.locked {
        blockers.push(RemovalBlocker::Locked);
    }
    if snapshot.selected_worktree.prunable {
        blockers.push(RemovalBlocker::Prunable);
    }
    blockers
}

/// Prove persisted ownership against fresh topology, status, and submodule facts.
/// No caller can construct a verified removal target directly.
pub fn inspect_removal(
    expected_common_dir: &Path,
    checkout: &SavedCheckout,
) -> std::result::Result<RemovalSafety, InspectionError> {
    let persisted_path = checkout.observed_path.to_path_buf();
    let expected_path =
        persisted_path
            .canonicalize()
            .map_err(|source| InspectionError::Canonicalize {
                path: persisted_path,
                source,
            })?;
    let first = discover_repository(&expected_path).map_err(InspectionError::Topology)?;
    let mut blockers = topology_blockers(
        expected_common_dir,
        &expected_path,
        checkout.observed_branch.as_deref(),
        checkout.managed_by_baude,
        &first,
    );
    if !blockers.is_empty() {
        return Ok(RemovalSafety::Blocked(blockers));
    }

    blockers.extend(inspect_removal_status(&expected_path)?);
    blockers.extend(inspect_submodules(&expected_path)?);
    if !blockers.is_empty() {
        return Ok(RemovalSafety::Blocked(blockers));
    }

    let branch_ref = checkout
        .observed_branch
        .as_deref()
        .ok_or_else(|| InspectionError::MalformedStatus("managed target has no branch".into()))?;
    let revision = OsString::from(format!("{branch_ref}^{{commit}}"));
    let branch_oid = removal_oid(&expected_path, &revision)?;
    let head_oid = removal_oid(&expected_path, OsStr::new("HEAD"))?;
    if branch_oid != head_oid {
        return Ok(RemovalSafety::Blocked(vec![RemovalBlocker::BranchChanged]));
    }

    let final_snapshot = discover_repository(&expected_path).map_err(InspectionError::Topology)?;
    blockers.extend(topology_blockers(
        expected_common_dir,
        &expected_path,
        Some(branch_ref),
        true,
        &final_snapshot,
    ));
    if !blockers.is_empty() || first != final_snapshot {
        if blockers.is_empty() {
            blockers.push(RemovalBlocker::PathChanged);
        }
        return Ok(RemovalSafety::Blocked(blockers));
    }

    Ok(RemovalSafety::Safe(VerifiedRemovalTarget {
        path: expected_path,
        common_dir: final_snapshot.common_dir,
        main_worktree: final_snapshot.main_worktree,
        branch_ref: branch_ref.to_owned(),
        branch_oid,
    }))
}

fn verified_remove_arguments() -> [&'static OsStr; 3] {
    [
        OsStr::new("worktree"),
        OsStr::new("remove"),
        OsStr::new("--"),
    ]
}

fn verify_removal_postconditions(
    target: &VerifiedRemovalTarget,
) -> std::result::Result<VerifiedRemoval, RemoveVerifiedError> {
    let fresh = discover_repository(&target.main_worktree).map_err(|error| {
        RemoveVerifiedError::Postcondition(RemovalPostconditionFailure::RepositoryUnavailable(
            error.to_string(),
        ))
    })?;
    if fresh.common_dir != target.common_dir || fresh.main_worktree != target.main_worktree {
        return Err(RemoveVerifiedError::Postcondition(
            RemovalPostconditionFailure::ParentIdentityChanged,
        ));
    }
    if fresh
        .worktrees
        .iter()
        .any(|record| record.path == target.path)
    {
        return Err(RemoveVerifiedError::Postcondition(
            RemovalPostconditionFailure::InventoryStillPresent(target.path.clone()),
        ));
    }
    match std::fs::symlink_metadata(&target.path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Ok(_) => {
            return Err(RemoveVerifiedError::Postcondition(
                RemovalPostconditionFailure::PathPresent(target.path.clone()),
            ));
        }
        Err(error) => {
            return Err(RemoveVerifiedError::Postcondition(
                RemovalPostconditionFailure::PathInspection {
                    path: target.path.clone(),
                    detail: error.to_string(),
                },
            ));
        }
    }
    let revision = OsString::from(format!("{}^{{commit}}", target.branch_ref));
    let observed = removal_oid(&target.main_worktree, &revision).map_err(|error| {
        RemoveVerifiedError::Postcondition(RemovalPostconditionFailure::BranchUnavailable(
            error.to_string(),
        ))
    })?;
    if observed != target.branch_oid {
        return Err(RemoveVerifiedError::Postcondition(
            RemovalPostconditionFailure::BranchChanged {
                expected: target.branch_oid.clone(),
                observed,
            },
        ));
    }
    Ok(VerifiedRemoval {
        removed_path: target.path.clone(),
        branch_ref: target.branch_ref.clone(),
        branch_oid: target.branch_oid.clone(),
    })
}

fn remove_verified_worktree_with_post_remove_hook(
    target: &VerifiedRemovalTarget,
    after_remove: impl FnOnce(),
) -> std::result::Result<VerifiedRemoval, RemoveVerifiedError> {
    let arguments = verified_remove_arguments();
    let output = Command::new("git")
        .arg("-C")
        .arg(&target.main_worktree)
        .args(arguments)
        .arg(&target.path)
        .output()
        .map_err(RemoveVerifiedError::CommandStart)?;
    if !output.status.success() {
        return Err(RemoveVerifiedError::GitRefused {
            status: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    after_remove();
    verify_removal_postconditions(target)
}

/// Invoke plain, non-force Git removal for a target produced by `inspect_removal`,
/// then prove the exact path/inventory record disappeared while its parent and
/// unchanged local branch remain.
pub fn remove_verified_worktree(
    target: &VerifiedRemovalTarget,
) -> std::result::Result<VerifiedRemoval, RemoveVerifiedError> {
    remove_verified_worktree_with_post_remove_hook(target, || {})
}

/// Compensation-only removal for a worktree that the current uncommitted
/// activation just added. Safe user-requested removal must instead consume a
/// `VerifiedRemovalTarget` through `remove_verified_worktree`.
pub(crate) fn remove_added_worktree(main_worktree: &Path, added_path: &Path) -> Result<()> {
    let output = Command::new("git")
        .arg("-C")
        .arg(main_worktree)
        .args(["worktree", "remove", "--"])
        .arg(added_path)
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(anyhow!(
            "git worktree remove compensation: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        activate_branch, activate_branch_with_post_add_hook, classify_branch, discover_repository,
        ensure_default_worktree, existing_branch_add_arguments, inspect_removal,
        inspect_removal_status, inspect_removal_status_with_program, inspect_submodules,
        inspect_submodules_with_program, managed_branch_worktree_path, new_branch_add_arguments,
        parse_clone_target, parse_removal_status, parse_submodule_status, parse_worktree_porcelain,
        reconcile_checkout, removal_status_arguments, remove_verified_worktree,
        remove_verified_worktree_with_post_remove_hook, resolve_default_branch,
        verified_remove_arguments, BranchActivation, BranchActivationError,
        BranchActivationOutcome, DefaultBranchUnavailable, DefaultWorktreeOutcome, InspectionError,
        ReconciliationUnavailable, RemovalBlocker, RemovalPostconditionFailure, RemovalSafety,
        RemoveVerifiedError,
    };
    use std::ffi::OsStr;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

    struct GitFixture {
        root: PathBuf,
    }

    impl GitFixture {
        fn new() -> Self {
            let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir()
                .join(format!("baude-git-test-{}-{sequence}", std::process::id()));
            std::fs::create_dir(&root).expect("create unique Git fixture root");
            Self { root }
        }

        fn repo(&self, relative: impl AsRef<Path>) -> PathBuf {
            let repo = self.root.join(relative);
            git_ok(&self.root, &[OsStr::new("init"), repo.as_os_str()]);
            git_ok(
                &repo,
                &[
                    OsStr::new("config"),
                    OsStr::new("user.name"),
                    OsStr::new("Baude Test"),
                ],
            );
            git_ok(
                &repo,
                &[
                    OsStr::new("config"),
                    OsStr::new("user.email"),
                    OsStr::new("baude@example.invalid"),
                ],
            );
            std::fs::write(repo.join("tracked.txt"), b"fixture\n").expect("write fixture file");
            git_ok(&repo, &[OsStr::new("add"), OsStr::new("tracked.txt")]);
            git_ok(
                &repo,
                &[
                    OsStr::new("commit"),
                    OsStr::new("-m"),
                    OsStr::new("fixture"),
                ],
            );
            git_ok(
                &repo,
                &[OsStr::new("branch"), OsStr::new("-M"), OsStr::new("topic")],
            );
            repo
        }

        fn unborn_repo(&self, relative: impl AsRef<Path>) -> PathBuf {
            let repo = self.root.join(relative);
            git_ok(&self.root, &[OsStr::new("init"), repo.as_os_str()]);
            repo
        }

        fn remote_head(&self, repo: &Path, remote: &str, branch: &str) {
            let url = self.root.join(format!("{remote}-remote.git"));
            git_ok(
                repo,
                &[
                    OsStr::new("remote"),
                    OsStr::new("add"),
                    OsStr::new(remote),
                    url.as_os_str(),
                ],
            );
            let tracking = format!("refs/remotes/{remote}/{branch}");
            git_ok(
                repo,
                &[
                    OsStr::new("update-ref"),
                    OsStr::new(&tracking),
                    OsStr::new("HEAD"),
                ],
            );
            let remote_head = format!("refs/remotes/{remote}/HEAD");
            git_ok(
                repo,
                &[
                    OsStr::new("symbolic-ref"),
                    OsStr::new(&remote_head),
                    OsStr::new(&tracking),
                ],
            );
        }

        fn set_main_upstream_remote(&self, repo: &Path, remote: &str) {
            git_ok(
                repo,
                &[
                    OsStr::new("config"),
                    OsStr::new("branch.topic.remote"),
                    OsStr::new(remote),
                ],
            );
            git_ok(
                repo,
                &[
                    OsStr::new("config"),
                    OsStr::new("branch.topic.merge"),
                    OsStr::new("refs/heads/topic"),
                ],
            );
        }

        fn linked_worktree(
            &self,
            repo: &Path,
            relative: impl AsRef<Path>,
            branch: &str,
        ) -> PathBuf {
            let path = self.root.join(relative);
            git_ok(repo, &[OsStr::new("branch"), OsStr::new(branch)]);
            git_ok(
                repo,
                &[
                    OsStr::new("worktree"),
                    OsStr::new("add"),
                    path.as_os_str(),
                    OsStr::new(branch),
                ],
            );
            path
        }
    }

    impl Drop for GitFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn git_ok(cwd: &Path, args: &[&OsStr]) -> Vec<u8> {
        let output = Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .output()
            .expect("run fixture Git command");
        assert!(
            output.status.success(),
            "git {:?}: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    }

    mod admission_identity {
        use super::*;

        #[test]
        fn aliases_converge_on_common_directory_and_main_record() {
            let fixture = GitFixture::new();
            let repo = fixture.repo("main repo");
            let nested = repo.join("nested");
            std::fs::create_dir(&nested).unwrap();
            let linked = fixture.linked_worktree(&repo, "linked checkout", "linked");

            let mut inputs = vec![repo.clone(), nested, linked];
            #[cfg(unix)]
            {
                let alias = fixture.root.join("repo alias");
                std::os::unix::fs::symlink(&repo, &alias).unwrap();
                inputs.push(alias);
            }

            let snapshots: Vec<_> = inputs
                .iter()
                .map(|path| discover_repository(path).unwrap())
                .collect();
            let expected_common = snapshots[0].common_dir.clone();
            let expected_main = snapshots[0].main_worktree.clone();
            for snapshot in snapshots {
                assert_eq!(snapshot.common_dir, expected_common);
                assert_eq!(snapshot.main_worktree, expected_main);
                assert_eq!(snapshot.worktrees.first().unwrap().path, expected_main);
                assert!(snapshot.worktrees.contains(&snapshot.selected_worktree));
            }
        }

        #[test]
        fn repositories_with_the_same_basename_remain_distinct() {
            let fixture = GitFixture::new();
            let first = fixture.repo("first/same-name");
            let second = fixture.repo("second/same-name");

            let first = discover_repository(&first).unwrap();
            let second = discover_repository(&second).unwrap();
            assert_ne!(first.common_dir, second.common_dir);
            assert_ne!(first.main_worktree, second.main_worktree);
        }

        #[cfg(unix)]
        #[test]
        fn nul_inventory_preserves_spaces_and_newlines_in_paths() {
            let fixture = GitFixture::new();
            let repo = fixture.repo("unusual repo");
            let linked = fixture.linked_worktree(&repo, "linked space\nline", "unusual");

            let snapshot = discover_repository(&linked).unwrap();
            assert_eq!(
                snapshot.selected_worktree.path,
                linked.canonicalize().unwrap()
            );
            assert!(snapshot
                .worktrees
                .iter()
                .any(|record| record.path == linked.canonicalize().unwrap()));
        }

        #[test]
        fn malformed_porcelain_fails_closed() {
            let malformed = b"worktree /tmp/example\0HEAD deadbeef\0branch refs/heads/main\0";
            assert!(parse_worktree_porcelain(malformed).is_err());
        }

        #[test]
        fn invalid_inputs_are_actionable_and_non_mutating() {
            let fixture = GitFixture::new();
            let ordinary = fixture.root.join("ordinary");
            std::fs::create_dir(&ordinary).unwrap();
            let missing = fixture.root.join("missing");

            let ordinary_error = discover_repository(&ordinary).unwrap_err().to_string();
            assert!(ordinary_error.contains("Git") || ordinary_error.contains("repository"));
            let missing_error = discover_repository(&missing).unwrap_err().to_string();
            assert!(missing_error.contains("canonicalize"));
            assert!(!missing.exists());
        }
    }

    mod default_branch {
        use super::*;

        #[test]
        fn main_upstream_remote_wins_and_slash_branch_is_preserved() {
            let fixture = GitFixture::new();
            let repo = fixture.repo("preferred");
            fixture.remote_head(&repo, "origin", "origin-default");
            fixture.remote_head(&repo, "upstream", "team/default");
            fixture.set_main_upstream_remote(&repo, "upstream");

            let default = resolve_default_branch(&discover_repository(&repo).unwrap()).unwrap();
            assert_eq!(default.remote, "upstream");
            assert_eq!(default.local_branch, "team/default");
            assert_eq!(default.local_ref, "refs/heads/team/default");
            assert_eq!(default.remote_ref, "refs/remotes/upstream/team/default");
        }

        #[test]
        fn origin_is_deduplicated_fallback() {
            let fixture = GitFixture::new();
            let repo = fixture.repo("origin fallback");
            fixture.remote_head(&repo, "origin", "trunk");

            let default = resolve_default_branch(&discover_repository(&repo).unwrap()).unwrap();
            assert_eq!(default.remote, "origin");
            assert_eq!(default.local_branch, "trunk");
        }

        #[test]
        fn detached_unborn_and_no_remote_are_unavailable() {
            let fixture = GitFixture::new();

            let detached = fixture.repo("detached");
            git_ok(&detached, &[OsStr::new("checkout"), OsStr::new("--detach")]);
            assert!(matches!(
                resolve_default_branch(&discover_repository(&detached).unwrap()),
                Err(DefaultBranchUnavailable::DetachedMainHead)
            ));

            let unborn = fixture.unborn_repo("unborn");
            assert!(matches!(
                resolve_default_branch(&discover_repository(&unborn).unwrap()),
                Err(DefaultBranchUnavailable::UnbornMainHead { .. })
            ));

            let no_remote = fixture.repo("no remote");
            assert!(matches!(
                resolve_default_branch(&discover_repository(&no_remote).unwrap()),
                Err(DefaultBranchUnavailable::NoCandidate { .. })
            ));
        }

        #[test]
        fn malformed_and_dangling_remote_heads_fail_closed() {
            let fixture = GitFixture::new();
            let malformed = fixture.repo("malformed");
            fixture.remote_head(&malformed, "origin", "trunk");
            git_ok(
                &malformed,
                &[
                    OsStr::new("symbolic-ref"),
                    OsStr::new("refs/remotes/origin/HEAD"),
                    OsStr::new("refs/heads/topic"),
                ],
            );
            assert!(matches!(
                resolve_default_branch(&discover_repository(&malformed).unwrap()),
                Err(DefaultBranchUnavailable::MalformedTarget { .. })
            ));

            let dangling = fixture.repo("dangling");
            fixture.remote_head(&dangling, "origin", "trunk");
            git_ok(
                &dangling,
                &[
                    OsStr::new("update-ref"),
                    OsStr::new("-d"),
                    OsStr::new("refs/remotes/origin/trunk"),
                ],
            );
            assert!(matches!(
                resolve_default_branch(&discover_repository(&dangling).unwrap()),
                Err(DefaultBranchUnavailable::DanglingTarget { .. })
            ));
        }
    }

    mod default_worktree {
        use super::*;

        #[test]
        fn reuses_main_then_existing_linked_worktree() {
            let fixture = GitFixture::new();
            let main = fixture.repo("main default");
            fixture.remote_head(&main, "origin", "topic");
            let snapshot = discover_repository(&main).unwrap();
            let default = resolve_default_branch(&snapshot).unwrap();
            assert!(matches!(
                ensure_default_worktree(&snapshot, &default, &fixture.root.join("unused"))
                    .unwrap(),
                DefaultWorktreeOutcome::Main(record) if record.path == main.canonicalize().unwrap()
            ));

            let linked_repo = fixture.repo("linked default");
            fixture.remote_head(&linked_repo, "origin", "team/default");
            let linked = fixture.linked_worktree(&linked_repo, "external default", "team/default");
            let snapshot = discover_repository(&linked_repo).unwrap();
            let default = resolve_default_branch(&snapshot).unwrap();
            assert!(matches!(
                ensure_default_worktree(&snapshot, &default, &fixture.root.join("also unused"))
                    .unwrap(),
                DefaultWorktreeOutcome::ExistingLinked(record)
                    if record.path == linked.canonicalize().unwrap()
            ));
        }

        #[test]
        fn creates_verified_default_without_mutating_main_checkout() {
            let fixture = GitFixture::new();
            let repo = fixture.repo("creation");
            git_ok(&repo, &[OsStr::new("branch"), OsStr::new("team/default")]);
            fixture.remote_head(&repo, "origin", "team/default");
            let before_head = git_ok(&repo, &[OsStr::new("symbolic-ref"), OsStr::new("HEAD")]);
            let before_status = git_ok(
                &repo,
                &[
                    OsStr::new("status"),
                    OsStr::new("--porcelain"),
                    OsStr::new("-z"),
                ],
            );
            let before_file = std::fs::read(repo.join("tracked.txt")).unwrap();
            let before_index = std::fs::read(repo.join(".git/index")).unwrap();

            let snapshot = discover_repository(&repo).unwrap();
            let default = resolve_default_branch(&snapshot).unwrap();
            let managed = fixture.root.join("managed/default");
            let outcome = ensure_default_worktree(&snapshot, &default, &managed).unwrap();
            assert!(matches!(
                outcome,
                DefaultWorktreeOutcome::CreatedManaged(record)
                    if record.path == managed.canonicalize().unwrap()
                        && record.branch.as_deref() == Some("refs/heads/team/default")
            ));

            assert_eq!(
                git_ok(&repo, &[OsStr::new("symbolic-ref"), OsStr::new("HEAD")]),
                before_head
            );
            assert_eq!(
                git_ok(
                    &repo,
                    &[
                        OsStr::new("status"),
                        OsStr::new("--porcelain"),
                        OsStr::new("-z")
                    ],
                ),
                before_status
            );
            assert_eq!(
                std::fs::read(repo.join("tracked.txt")).unwrap(),
                before_file
            );
            assert_eq!(
                std::fs::read(repo.join(".git/index")).unwrap(),
                before_index
            );
        }

        #[test]
        fn creates_local_branch_from_exact_verified_remote_source() {
            let fixture = GitFixture::new();
            let repo = fixture.repo("remote source");
            fixture.remote_head(&repo, "origin", "team/default");
            let snapshot = discover_repository(&repo).unwrap();
            let default = resolve_default_branch(&snapshot).unwrap();
            let managed = fixture.root.join("remote-created");

            let outcome = ensure_default_worktree(&snapshot, &default, &managed).unwrap();
            assert!(matches!(outcome, DefaultWorktreeOutcome::CreatedManaged(_)));
            git_ok(
                &repo,
                &[
                    OsStr::new("rev-parse"),
                    OsStr::new("--verify"),
                    OsStr::new("refs/heads/team/default^{commit}"),
                ],
            );
        }

        #[test]
        fn rejects_unregistered_managed_path_collision() {
            let fixture = GitFixture::new();
            let repo = fixture.repo("collision");
            git_ok(&repo, &[OsStr::new("branch"), OsStr::new("trunk")]);
            fixture.remote_head(&repo, "origin", "trunk");
            let collision = fixture.root.join("collision path");
            std::fs::create_dir(&collision).unwrap();
            let snapshot = discover_repository(&repo).unwrap();
            let default = resolve_default_branch(&snapshot).unwrap();

            let error = ensure_default_worktree(&snapshot, &default, &collision)
                .unwrap_err()
                .to_string();
            assert!(error.contains("collision"));
            assert!(discover_repository(&collision).is_err());
        }
    }

    mod lifecycle {
        use super::*;

        mod removal_preflight {
            use super::*;

            fn linked_for_status(fixture: &GitFixture, label: &str) -> (PathBuf, PathBuf) {
                let repo = fixture.repo(format!("{label} repo"));
                let linked = fixture.linked_worktree(
                    &repo,
                    format!("{label} linked"),
                    &format!("{}-branch", label.replace(' ', "-")),
                );
                (repo, linked)
            }

            fn blockers(path: &Path) -> Vec<RemovalBlocker> {
                inspect_removal_status(path).expect("status inspection must be conclusive")
            }

            #[test]
            fn status_command_is_config_independent_and_complete() {
                let arguments: Vec<_> = removal_status_arguments()
                    .iter()
                    .map(|argument| argument.to_string_lossy().into_owned())
                    .collect();
                assert_eq!(
                    arguments,
                    [
                        "--no-optional-locks",
                        "status",
                        "--porcelain=v2",
                        "-z",
                        "--untracked-files=all",
                        "--ignore-submodules=none",
                        "--ignored=matching",
                    ]
                );
            }

            #[test]
            fn staged_and_unstaged_changes_are_distinct_blockers() {
                let fixture = GitFixture::new();

                let (_, staged_add) = linked_for_status(&fixture, "staged-add");
                std::fs::write(staged_add.join("added.txt"), b"added\n").unwrap();
                git_ok(&staged_add, &[OsStr::new("add"), OsStr::new("added.txt")]);
                assert!(blockers(&staged_add)
                    .iter()
                    .any(|blocker| matches!(blocker, RemovalBlocker::StagedAdd { .. })));

                let (_, staged_delete) = linked_for_status(&fixture, "staged-delete");
                git_ok(
                    &staged_delete,
                    &[OsStr::new("rm"), OsStr::new("tracked.txt")],
                );
                assert!(blockers(&staged_delete)
                    .iter()
                    .any(|blocker| matches!(blocker, RemovalBlocker::StagedDelete { .. })));

                let (_, staged_rename) = linked_for_status(&fixture, "staged-rename");
                git_ok(
                    &staged_rename,
                    &[
                        OsStr::new("mv"),
                        OsStr::new("tracked.txt"),
                        OsStr::new("renamed.txt"),
                    ],
                );
                assert!(blockers(&staged_rename)
                    .iter()
                    .any(|blocker| matches!(blocker, RemovalBlocker::StagedRename { .. })));

                let (_, unstaged_edit) = linked_for_status(&fixture, "unstaged-edit");
                std::fs::write(unstaged_edit.join("tracked.txt"), b"changed\n").unwrap();
                assert!(blockers(&unstaged_edit)
                    .iter()
                    .any(|blocker| matches!(blocker, RemovalBlocker::UnstagedModification { .. })));

                let (_, unstaged_delete) = linked_for_status(&fixture, "unstaged-delete");
                std::fs::remove_file(unstaged_delete.join("tracked.txt")).unwrap();
                assert!(blockers(&unstaged_delete)
                    .iter()
                    .any(|blocker| matches!(blocker, RemovalBlocker::UnstagedDelete { .. })));
            }

            #[test]
            fn untracked_ignored_conflicted_and_unusual_names_block() {
                let fixture = GitFixture::new();

                let (_, untracked) = linked_for_status(&fixture, "untracked");
                std::fs::create_dir(untracked.join("new dir")).unwrap();
                std::fs::write(untracked.join("new dir/file.txt"), b"new\n").unwrap();
                assert!(blockers(&untracked)
                    .iter()
                    .any(|blocker| matches!(blocker, RemovalBlocker::Untracked { .. })));

                let ignored_repo = fixture.repo("ignored repo");
                std::fs::write(ignored_repo.join(".gitignore"), b"ignored*\n").unwrap();
                git_ok(
                    &ignored_repo,
                    &[OsStr::new("add"), OsStr::new(".gitignore")],
                );
                git_ok(
                    &ignored_repo,
                    &[OsStr::new("commit"), OsStr::new("-m"), OsStr::new("ignore")],
                );
                let ignored =
                    fixture.linked_worktree(&ignored_repo, "ignored linked", "ignored-branch");
                std::fs::create_dir(ignored.join("ignored-dir")).unwrap();
                std::fs::write(ignored.join("ignored-dir/file"), b"ignored\n").unwrap();
                std::fs::write(ignored.join("ignored-file"), b"ignored\n").unwrap();
                assert!(blockers(&ignored)
                    .iter()
                    .any(|blocker| matches!(blocker, RemovalBlocker::Ignored { .. })));

                let conflict_repo = fixture.repo("conflict repo");
                let conflict =
                    fixture.linked_worktree(&conflict_repo, "conflict linked", "conflict-branch");
                std::fs::write(conflict.join("tracked.txt"), b"linked\n").unwrap();
                git_ok(
                    &conflict,
                    &[
                        OsStr::new("commit"),
                        OsStr::new("-am"),
                        OsStr::new("linked"),
                    ],
                );
                std::fs::write(conflict_repo.join("tracked.txt"), b"main\n").unwrap();
                git_ok(
                    &conflict_repo,
                    &[OsStr::new("commit"), OsStr::new("-am"), OsStr::new("main")],
                );
                let merge = Command::new("git")
                    .arg("-C")
                    .arg(&conflict)
                    .args(["merge", "topic"])
                    .output()
                    .unwrap();
                assert!(!merge.status.success());
                assert!(blockers(&conflict)
                    .iter()
                    .any(|blocker| matches!(blocker, RemovalBlocker::Conflict { .. })));

                #[cfg(unix)]
                {
                    use std::os::unix::ffi::OsStrExt;
                    let (_, unusual) = linked_for_status(&fixture, "unusual");
                    let name = OsStr::from_bytes(b"line\nspace name");
                    std::fs::write(unusual.join(name), b"unusual\n").unwrap();
                    assert!(blockers(&unusual).iter().any(|blocker| {
                        matches!(blocker, RemovalBlocker::Untracked { path } if path == b"line\nspace name")
                    }));
                }
            }

            #[test]
            fn only_empty_valid_status_is_clean_and_malformed_output_fails_closed() {
                assert_eq!(parse_removal_status(b"").unwrap(), Vec::new());
                for malformed in [
                    b"x unknown\0".as_slice(),
                    b"?\0".as_slice(),
                    b"1 M. N... too few\0".as_slice(),
                    b"2 R. N... 100644 100644 100644 a b R100 path\0".as_slice(),
                    b"1 \xff. N... 100644 100644 100644 a b path\0".as_slice(),
                    b"? unterminated".as_slice(),
                ] {
                    assert!(matches!(
                        parse_removal_status(malformed),
                        Err(InspectionError::MalformedStatus(_))
                    ));
                }
            }

            #[test]
            fn process_start_and_nonzero_status_are_indeterminate() {
                let fixture = GitFixture::new();
                let repo = fixture.repo("command failures");
                assert!(matches!(
                    inspect_removal_status_with_program(
                        &repo,
                        OsStr::new("baude-definitely-missing-git")
                    ),
                    Err(InspectionError::CommandStart { .. })
                ));

                let ordinary = fixture.root.join("not a repository");
                std::fs::create_dir(&ordinary).unwrap();
                assert!(matches!(
                    inspect_removal_status(&ordinary),
                    Err(InspectionError::GitCommand { .. })
                ));
            }
        }

        mod removal_topology {
            use super::*;
            use crate::repository::{
                CheckoutHealth, CheckoutLifecycle, CheckoutRole, PersistedPath, RepositoryState,
                RetainedSessionState, SavedCheckout,
            };

            pub(super) fn saved_checkout(
                path: &Path,
                branch: &str,
                managed: bool,
            ) -> SavedCheckout {
                let mut state = RepositoryState::default();
                let repository_key = state.allocate_repository_key().unwrap();
                let checkout_key = state.allocate_checkout_key().unwrap();
                SavedCheckout {
                    key: checkout_key,
                    repository_key,
                    role: CheckoutRole::ManagedBranch,
                    managed_by_baude: managed,
                    observed_path: PersistedPath::from_path(path),
                    observed_branch: Some(branch.to_owned()),
                    first_seen_order: state.allocate_first_seen_order().unwrap(),
                    lifecycle: CheckoutLifecycle::Active,
                    active_intent: true,
                    session: RetainedSessionState {
                        name: "managed".into(),
                        cwd: PersistedPath::from_path(path),
                        repo_root: PersistedPath::from_path(path),
                        branch: Some(branch.to_owned()),
                        is_worktree: true,
                        shell_open: false,
                        archived: false,
                        archived_by_user: false,
                        resume_id: None,
                    },
                    health: CheckoutHealth::Available,
                }
            }

            fn linked_target(
                fixture: &GitFixture,
                label: &str,
            ) -> (PathBuf, PathBuf, PathBuf, String, SavedCheckout) {
                let repo = fixture.repo(format!("{label} repo"));
                let linked = fixture.linked_worktree(
                    &repo,
                    format!("{label} linked"),
                    &format!("{}-branch", label.replace(' ', "-")),
                );
                let snapshot = discover_repository(&linked).unwrap();
                let branch = snapshot.selected_worktree.branch.clone().unwrap();
                let checkout = saved_checkout(&linked, &branch, true);
                (repo, linked, snapshot.common_dir, branch, checkout)
            }

            fn assert_blocked(safety: RemovalSafety, expected: RemovalBlocker) {
                match safety {
                    RemovalSafety::Blocked(blockers) => assert!(blockers.contains(&expected)),
                    RemovalSafety::Safe(_) => panic!("unsafe target received a removal token"),
                }
            }

            #[test]
            fn only_exact_managed_linked_topology_produces_an_opaque_target() {
                let fixture = GitFixture::new();
                let (repo, linked, common, branch, checkout) =
                    linked_target(&fixture, "safe topology");
                let expected_oid = String::from_utf8(git_ok(
                    &linked,
                    &[
                        OsStr::new("rev-parse"),
                        OsStr::new("--verify"),
                        OsStr::new("HEAD"),
                    ],
                ))
                .unwrap()
                .trim()
                .to_owned();

                let target = match inspect_removal(&common, &checkout).unwrap() {
                    RemovalSafety::Safe(target) => target,
                    RemovalSafety::Blocked(blockers) => {
                        panic!("clean target blocked: {blockers:?}")
                    }
                };
                assert_eq!(target.path(), linked.canonicalize().unwrap());
                assert_eq!(target.common_dir(), common);
                assert_eq!(target.main_worktree(), repo.canonicalize().unwrap());
                assert_eq!(target.branch_ref(), branch);
                assert_eq!(target.branch_oid(), expected_oid);
            }

            #[test]
            fn main_external_locked_detached_and_stale_facts_never_become_safe() {
                let fixture = GitFixture::new();

                let main = fixture.repo("main target");
                let main_snapshot = discover_repository(&main).unwrap();
                let main_branch = main_snapshot.selected_worktree.branch.clone().unwrap();
                let main_checkout = saved_checkout(&main, &main_branch, true);
                assert_blocked(
                    inspect_removal(&main_snapshot.common_dir, &main_checkout).unwrap(),
                    RemovalBlocker::MainWorktree,
                );

                let (_, linked, common, branch, mut checkout) =
                    linked_target(&fixture, "external target");
                checkout.managed_by_baude = false;
                assert_blocked(
                    inspect_removal(&common, &checkout).unwrap(),
                    RemovalBlocker::NotManaged,
                );

                checkout.managed_by_baude = true;
                git_ok(
                    &linked,
                    &[
                        OsStr::new("worktree"),
                        OsStr::new("lock"),
                        linked.as_os_str(),
                    ],
                );
                assert_blocked(
                    inspect_removal(&common, &checkout).unwrap(),
                    RemovalBlocker::Locked,
                );
                git_ok(
                    &linked,
                    &[
                        OsStr::new("worktree"),
                        OsStr::new("unlock"),
                        linked.as_os_str(),
                    ],
                );

                git_ok(&linked, &[OsStr::new("checkout"), OsStr::new("--detach")]);
                assert_blocked(
                    inspect_removal(&common, &checkout).unwrap(),
                    RemovalBlocker::Detached,
                );

                let (_, fresh_linked, fresh_common, fresh_branch, fresh_checkout) =
                    linked_target(&fixture, "stale target");
                assert_blocked(
                    inspect_removal(Path::new("/definitely/different/common"), &fresh_checkout)
                        .unwrap(),
                    RemovalBlocker::IdentityChanged,
                );
                let changed = saved_checkout(&fresh_linked, "refs/heads/other", true);
                assert_blocked(
                    inspect_removal(&fresh_common, &changed).unwrap(),
                    RemovalBlocker::BranchChanged,
                );

                let moved = fixture.root.join("moved outside git");
                std::fs::rename(&fresh_linked, &moved).unwrap();
                let moved_checkout = saved_checkout(&moved, &fresh_branch, true);
                assert!(inspect_removal(&fresh_common, &moved_checkout).is_err());

                assert_ne!(branch, fresh_branch);
            }

            #[test]
            fn every_submodule_row_blocks_and_malformed_or_failing_output_is_indeterminate() {
                let oid = b"0123456789012345678901234567890123456789";
                for status in *b" -+U" {
                    let mut row = vec![status];
                    row.extend_from_slice(oid);
                    row.extend_from_slice(b" modules/child (heads/main)\n");
                    assert_eq!(
                        parse_submodule_status(&row).unwrap(),
                        vec![RemovalBlocker::SubmodulePresent {
                            path: b"modules/child".to_vec(),
                            status,
                        }]
                    );
                }
                let nested = format!(
                    " {} modules/parent (heads/main)\n-{} modules/parent/nested\n",
                    String::from_utf8_lossy(oid),
                    String::from_utf8_lossy(oid)
                );
                assert_eq!(parse_submodule_status(nested.as_bytes()).unwrap().len(), 2);
                for malformed in [
                    b"x0123456789012345678901234567890123456789 path\n".as_slice(),
                    b" 0123 path\n".as_slice(),
                    b" 0123456789012345678901234567890123456789\n".as_slice(),
                    b" 0123456789012345678901234567890123456789 path".as_slice(),
                ] {
                    assert!(matches!(
                        parse_submodule_status(malformed),
                        Err(InspectionError::MalformedSubmoduleStatus(_))
                    ));
                }

                let fixture = GitFixture::new();
                let repo = fixture.repo("submodule command failures");
                assert!(matches!(
                    inspect_submodules_with_program(
                        &repo,
                        OsStr::new("baude-definitely-missing-git")
                    ),
                    Err(InspectionError::CommandStart { .. })
                ));
                let ordinary = fixture.root.join("ordinary submodule path");
                std::fs::create_dir(&ordinary).unwrap();
                assert!(matches!(
                    inspect_submodules(&ordinary),
                    Err(InspectionError::GitCommand { .. })
                ));
            }

            #[test]
            fn a_recorded_clean_submodule_blocks_real_git_removal_preflight() {
                let fixture = GitFixture::new();
                let submodule = fixture.repo("submodule source");
                let (_, linked, common, _, checkout) =
                    linked_target(&fixture, "submodule superproject");
                git_ok(
                    &linked,
                    &[
                        OsStr::new("-c"),
                        OsStr::new("protocol.file.allow=always"),
                        OsStr::new("submodule"),
                        OsStr::new("add"),
                        submodule.as_os_str(),
                        OsStr::new("modules/child"),
                    ],
                );
                git_ok(
                    &linked,
                    &[
                        OsStr::new("commit"),
                        OsStr::new("-am"),
                        OsStr::new("submodule"),
                    ],
                );

                let safety = inspect_removal(&common, &checkout).unwrap();
                match safety {
                    RemovalSafety::Blocked(blockers) => assert!(blockers.iter().any(|blocker| {
                        matches!(blocker, RemovalBlocker::SubmodulePresent { path, status: b' ' }
                            if path == b"modules/child")
                    })),
                    RemovalSafety::Safe(_) => panic!("recorded submodule received removal token"),
                }
            }
        }

        mod remove_postconditions {
            use super::*;

            fn verified_target(
                fixture: &GitFixture,
                label: &str,
            ) -> (
                PathBuf,
                PathBuf,
                String,
                String,
                crate::git::VerifiedRemovalTarget,
            ) {
                let repo = fixture.repo(format!("{label} repo"));
                let linked = fixture.linked_worktree(
                    &repo,
                    format!("{label} linked"),
                    &format!("{}-branch", label.replace(' ', "-")),
                );
                let snapshot = discover_repository(&linked).unwrap();
                let branch = snapshot.selected_worktree.branch.clone().unwrap();
                let checkout = removal_topology::saved_checkout(&linked, &branch, true);
                let target = match inspect_removal(&snapshot.common_dir, &checkout).unwrap() {
                    RemovalSafety::Safe(target) => target,
                    RemovalSafety::Blocked(blockers) => {
                        panic!("clean target blocked: {blockers:?}")
                    }
                };
                let linked = target.path().to_path_buf();
                let oid = target.branch_oid().to_owned();
                (repo, linked, branch, oid, target)
            }

            #[test]
            fn plain_remove_preserves_exact_branch_parent_and_sibling() {
                let fixture = GitFixture::new();
                let (repo, linked, branch, oid, target) =
                    verified_target(&fixture, "remove postconditions");
                let sibling = fixture
                    .linked_worktree(&repo, "preserved sibling", "sibling-branch")
                    .canonicalize()
                    .unwrap();
                let arguments = verified_remove_arguments();
                assert_eq!(
                    arguments,
                    [
                        OsStr::new("worktree"),
                        OsStr::new("remove"),
                        OsStr::new("--")
                    ]
                );
                assert!(!arguments.contains(&OsStr::new("--force")));

                let outcome = remove_verified_worktree(&target).unwrap();

                assert_eq!(outcome.removed_path(), linked);
                assert_eq!(outcome.branch_ref(), branch);
                assert_eq!(outcome.branch_oid(), oid);
                assert!(repo.is_dir());
                assert!(sibling.is_dir());
                assert!(!linked.exists());
                let fresh = discover_repository(&repo).unwrap();
                assert!(fresh.worktrees.iter().all(|record| record.path != linked));
                assert!(fresh.worktrees.iter().any(|record| record.path == sibling));
            }

            #[test]
            fn recreated_path_is_reported_and_never_recursively_deleted() {
                let fixture = GitFixture::new();
                let (_, linked, _, _, target) = verified_target(&fixture, "recreated path");

                let error = remove_verified_worktree_with_post_remove_hook(&target, || {
                    std::fs::create_dir(&linked).unwrap();
                    std::fs::write(linked.join("external-evidence"), b"keep me\n").unwrap();
                })
                .unwrap_err();

                assert!(matches!(
                    error,
                    RemoveVerifiedError::Postcondition(RemovalPostconditionFailure::PathPresent(path))
                        if path == linked
                ));
                assert_eq!(
                    std::fs::read(linked.join("external-evidence")).unwrap(),
                    b"keep me\n"
                );
            }

            #[test]
            fn externally_recreated_git_worktree_is_reported_and_preserved() {
                let fixture = GitFixture::new();
                let (repo, linked, branch, _, target) =
                    verified_target(&fixture, "recreated git worktree");
                let short_branch = branch.strip_prefix("refs/heads/").unwrap().to_owned();

                let error = remove_verified_worktree_with_post_remove_hook(&target, || {
                    git_ok(
                        &repo,
                        &[
                            OsStr::new("worktree"),
                            OsStr::new("add"),
                            OsStr::new("--"),
                            linked.as_os_str(),
                            OsStr::new(&short_branch),
                        ],
                    );
                })
                .unwrap_err();

                assert!(matches!(
                    error,
                    RemoveVerifiedError::Postcondition(
                        RemovalPostconditionFailure::InventoryStillPresent(path)
                    ) if path == linked
                ));
                assert!(linked.is_dir());
                assert!(discover_repository(&repo)
                    .unwrap()
                    .worktrees
                    .iter()
                    .any(|record| record.path == linked));
            }

            #[test]
            fn changed_branch_oid_after_git_remove_is_visible_degradation() {
                let fixture = GitFixture::new();
                let (repo, _, branch, oid, target) = verified_target(&fixture, "changed branch");
                std::fs::write(repo.join("later"), b"later\n").unwrap();
                git_ok(&repo, &[OsStr::new("add"), OsStr::new("later")]);
                git_ok(
                    &repo,
                    &[OsStr::new("commit"), OsStr::new("-m"), OsStr::new("later")],
                );
                let later = git_ok(&repo, &[OsStr::new("rev-parse"), OsStr::new("HEAD")]);
                let later = String::from_utf8(later).unwrap().trim().to_owned();

                let error = remove_verified_worktree_with_post_remove_hook(&target, || {
                    git_ok(
                        &repo,
                        &[
                            OsStr::new("branch"),
                            OsStr::new("-f"),
                            OsStr::new(branch.strip_prefix("refs/heads/").unwrap()),
                            OsStr::new(&later),
                        ],
                    );
                })
                .unwrap_err();

                assert!(matches!(
                    error,
                    RemoveVerifiedError::Postcondition(
                        RemovalPostconditionFailure::BranchChanged { expected, observed }
                    ) if expected == oid && observed == later
                ));
            }
        }

        mod branch_activation {
            use super::*;

            #[test]
            fn new_branch_from_child_starts_at_freshly_verified_default() {
                let fixture = GitFixture::new();
                let repo = fixture.repo("new branch base");
                git_ok(&repo, &[OsStr::new("branch"), OsStr::new("trunk")]);
                fixture.remote_head(&repo, "origin", "trunk");
                let child = fixture.linked_worktree(&repo, "request child", "caller-feature");
                std::fs::write(child.join("child-only"), b"different head\n").unwrap();
                git_ok(&child, &[OsStr::new("add"), OsStr::new("child-only")]);
                git_ok(
                    &child,
                    &[OsStr::new("commit"), OsStr::new("-m"), OsStr::new("child")],
                );
                let default_oid = git_ok(
                    &repo,
                    &[
                        OsStr::new("rev-parse"),
                        OsStr::new("refs/heads/trunk^{commit}"),
                    ],
                );
                let child_oid = git_ok(&child, &[OsStr::new("rev-parse"), OsStr::new("HEAD")]);
                assert_ne!(default_oid, child_oid);

                let managed = fixture.root.join("managed/new literal");
                let outcome = activate_branch(&child, "feature/literal", &managed).unwrap();
                assert!(matches!(
                    outcome,
                    BranchActivationOutcome::CreatedManaged(_)
                ));
                assert_eq!(
                    git_ok(&managed, &[OsStr::new("rev-parse"), OsStr::new("HEAD")]),
                    default_oid
                );
                assert_eq!(
                    git_ok(&managed, &[OsStr::new("symbolic-ref"), OsStr::new("HEAD")]),
                    b"refs/heads/feature/literal\n"
                );
            }

            #[test]
            fn existing_local_branch_activates_without_resetting_oid() {
                let fixture = GitFixture::new();
                let repo = fixture.repo("existing local");
                git_ok(&repo, &[OsStr::new("branch"), OsStr::new("existing")]);
                let before = git_ok(
                    &repo,
                    &[
                        OsStr::new("rev-parse"),
                        OsStr::new("refs/heads/existing^{commit}"),
                    ],
                );

                let managed = fixture.root.join("managed/existing");
                let outcome = activate_branch(&repo, "existing", &managed).unwrap();
                assert!(matches!(
                    outcome,
                    BranchActivationOutcome::ActivatedManaged(_)
                ));
                assert_eq!(
                    git_ok(&managed, &[OsStr::new("rev-parse"), OsStr::new("HEAD")]),
                    before
                );
                assert_eq!(
                    git_ok(
                        &repo,
                        &[
                            OsStr::new("rev-parse"),
                            OsStr::new("refs/heads/existing^{commit}")
                        ]
                    ),
                    before
                );
            }

            #[test]
            fn occupied_branch_reuses_inventory_record_without_second_add() {
                let fixture = GitFixture::new();
                let repo = fixture.repo("occupied");
                let external = fixture.linked_worktree(&repo, "external occupied", "occupied");
                let before = discover_repository(&repo).unwrap().worktrees.len();

                assert!(matches!(
                    classify_branch(&discover_repository(&repo).unwrap(), "occupied").unwrap(),
                    BranchActivation::Occupied { ref record, .. }
                        if record.path == external.canonicalize().unwrap()
                ));
                let outcome =
                    activate_branch(&repo, "occupied", &fixture.root.join("must-not-be-created"))
                        .unwrap();
                assert!(matches!(
                    outcome,
                    BranchActivationOutcome::Reused(record)
                        if record.path == external.canonicalize().unwrap()
                ));
                assert_eq!(discover_repository(&repo).unwrap().worktrees.len(), before);
                assert!(!fixture.root.join("must-not-be-created").exists());
            }

            #[test]
            fn remote_only_and_previous_checkout_shorthand_are_rejected() {
                let fixture = GitFixture::new();
                let repo = fixture.repo("remote only");
                git_ok(
                    &repo,
                    &[
                        OsStr::new("update-ref"),
                        OsStr::new("refs/remotes/origin/remote-only"),
                        OsStr::new("HEAD"),
                    ],
                );
                let snapshot = discover_repository(&repo).unwrap();

                assert!(matches!(
                    classify_branch(&snapshot, "remote-only"),
                    Err(BranchActivationError::RemoteOnly { .. })
                ));
                assert!(matches!(
                    classify_branch(&snapshot, "@{-1}"),
                    Err(BranchActivationError::InvalidLiteral { .. })
                ));
            }

            #[test]
            fn add_commands_are_explicit_and_never_force_reset_fetch_or_delete() {
                let path = Path::new("/tmp/literal target");
                let captured_oid = "0123456789abcdef0123456789abcdef01234567";
                let new = new_branch_add_arguments("feature/literal", path, captured_oid);
                let existing = existing_branch_add_arguments("feature/literal", path);
                let new: Vec<_> = new
                    .iter()
                    .map(|argument| argument.to_string_lossy().into_owned())
                    .collect();
                let existing: Vec<_> = existing
                    .iter()
                    .map(|argument| argument.to_string_lossy().into_owned())
                    .collect();

                assert_eq!(
                    new,
                    [
                        "worktree",
                        "add",
                        "-b",
                        "feature/literal",
                        "--",
                        "/tmp/literal target",
                        captured_oid,
                    ]
                );
                assert_eq!(
                    existing,
                    [
                        "worktree",
                        "add",
                        "--",
                        "/tmp/literal target",
                        "feature/literal",
                    ]
                );
                for forbidden in ["--force", "-B", "fetch", "delete", "remove"] {
                    assert!(!new.iter().chain(&existing).any(|arg| arg == forbidden));
                }
            }

            #[test]
            fn post_add_topology_failure_is_compensated_without_deleting_branch() {
                let fixture = GitFixture::new();
                let repo = fixture.repo("post add compensation");
                git_ok(
                    &repo,
                    &[OsStr::new("branch"), OsStr::new("post-add-failure")],
                );
                let managed = fixture.root.join("post-add-managed");
                let hook_path = managed.clone();

                let error =
                    activate_branch_with_post_add_hook(&repo, "post-add-failure", &managed, || {
                        git_ok(
                            &hook_path,
                            &[OsStr::new("checkout"), OsStr::new("--detach")],
                        );
                    })
                    .unwrap_err();

                assert!(
                    matches!(error, BranchActivationError::Verification(_)),
                    "unexpected post-add error: {error:?}"
                );
                assert!(!managed.exists());
                assert!(discover_repository(&repo)
                    .unwrap()
                    .worktrees
                    .iter()
                    .all(|record| record.path != managed));
                git_ok(
                    &repo,
                    &[
                        OsStr::new("show-ref"),
                        OsStr::new("--verify"),
                        OsStr::new("refs/heads/post-add-failure"),
                    ],
                );
            }

            #[test]
            fn post_add_compensation_refusal_returns_recoverable_topology() {
                let fixture = GitFixture::new();
                let repo = fixture.repo("post add compensation refusal");
                git_ok(
                    &repo,
                    &[OsStr::new("branch"), OsStr::new("post-add-stranded")],
                );
                let managed = fixture.root.join("post-add-stranded");
                let hook_path = managed.clone();

                let error = activate_branch_with_post_add_hook(
                    &repo,
                    "post-add-stranded",
                    &managed,
                    || {
                        std::fs::write(hook_path.join("untracked"), b"retain me\n").unwrap();
                        git_ok(
                            &hook_path,
                            &[OsStr::new("checkout"), OsStr::new("--detach")],
                        );
                    },
                )
                .unwrap_err();

                assert!(matches!(
                    error,
                    BranchActivationError::PostAddCompensationFailed {
                        ref path,
                        ref branch,
                        created_branch: false,
                        ..
                    } if path == &managed && branch == "post-add-stranded"
                ));
                assert!(managed.join("untracked").is_file());
            }
        }

        mod creation_safety {
            use super::*;

            #[test]
            fn git_rejects_malformed_literals_before_allocating_a_path() {
                let fixture = GitFixture::new();
                let repo = fixture.repo("invalid literals");
                let snapshot = discover_repository(&repo).unwrap();

                for (index, literal) in [
                    "-leading",
                    "name.lock",
                    "two..dots",
                    "@",
                    "@{-1}",
                    "with space",
                    "with\u{7f}control",
                ]
                .into_iter()
                .enumerate()
                {
                    let candidate = fixture.root.join(format!("candidate-{index}"));
                    assert!(matches!(
                        classify_branch(&snapshot, literal),
                        Err(BranchActivationError::InvalidLiteral { .. })
                    ));
                    assert!(!candidate.exists());
                }
            }

            #[test]
            fn every_filesystem_entry_blocks_a_managed_candidate() {
                let fixture = GitFixture::new();
                let repo = fixture.repo("filesystem collisions");

                for (label, make) in [("file", 0_u8), ("directory", 1_u8), ("symlink", 2_u8)] {
                    let candidate = fixture.root.join(label);
                    match make {
                        0 => std::fs::write(&candidate, b"occupied").unwrap(),
                        1 => std::fs::create_dir(&candidate).unwrap(),
                        #[cfg(unix)]
                        2 => std::os::unix::fs::symlink(&repo, &candidate).unwrap(),
                        _ => continue,
                    }
                    let error = activate_branch(&repo, "existing", &candidate).unwrap_err();
                    assert!(
                        matches!(error, BranchActivationError::PathCollision(path) if path == candidate)
                    );
                    assert!(
                        classify_branch(&discover_repository(&repo).unwrap(), "existing").is_ok()
                    );
                }
            }

            #[test]
            fn missing_but_registered_prunable_path_still_blocks_reuse() {
                let fixture = GitFixture::new();
                let repo = fixture.repo("inventory collision");
                let registered = fixture.linked_worktree(&repo, "missing registered", "registered");
                let registered = registered.canonicalize().unwrap();
                git_ok(&repo, &[OsStr::new("branch"), OsStr::new("existing")]);
                git_ok(
                    &repo,
                    &[
                        OsStr::new("worktree"),
                        OsStr::new("lock"),
                        registered.as_os_str(),
                    ],
                );
                std::fs::remove_dir_all(&registered).unwrap();

                let snapshot = discover_repository(&repo).unwrap();
                assert!(snapshot
                    .worktrees
                    .iter()
                    .any(|record| record.path == registered));
                assert!(matches!(
                    activate_branch(&repo, "existing", &registered),
                    Err(BranchActivationError::PathCollision(path)) if path == registered
                ));
            }

            #[test]
            fn durable_keys_not_labels_supply_bounded_path_identity() {
                let slash = managed_branch_worktree_path(7, 11, "feature/a");
                let dash = managed_branch_worktree_path(7, 12, "feature-a");
                let other_repository = managed_branch_worktree_path(8, 11, "feature/a");
                assert_ne!(slash, dash);
                assert_ne!(slash, other_repository);
                assert!(slash.to_string_lossy().contains("repository-7"));
                assert!(slash
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .ends_with("-11"));

                let unicode = managed_branch_worktree_path(7, 13, &"界".repeat(40));
                let component = unicode.file_name().unwrap().to_string_lossy();
                assert!(!component.is_empty());
                assert!(component.len() <= 48 + "-13".len());
            }

            #[test]
            fn lifecycle_add_argv_has_no_bypass_or_destructive_command() {
                let path = Path::new("/tmp/untrusted target");
                let commands = [
                    new_branch_add_arguments("feature/literal", path, "refs/heads/main"),
                    existing_branch_add_arguments("feature/literal", path),
                ];
                let forbidden = [
                    "--force", "-B", "prune", "repair", "clean", "reset", "stash", "fetch",
                    "delete", "-d", "-D",
                ];
                for command in commands {
                    let command: Vec<_> = command
                        .iter()
                        .map(|argument| argument.to_string_lossy().into_owned())
                        .collect();
                    for forbidden in forbidden {
                        assert!(!command.iter().any(|argument| argument == forbidden));
                    }
                }
            }

            #[test]
            fn malformed_inventory_is_a_typed_refusal() {
                let malformed = b"worktree /tmp/example\0HEAD deadbeef\0branch refs/heads/main\0";
                assert!(matches!(
                    parse_worktree_porcelain(malformed),
                    Err(super::super::super::RepositoryDiscoveryError::MalformedTopology(_))
                ));
            }
        }
    }

    mod reconciliation {
        use super::*;

        #[test]
        fn accepts_only_the_expected_common_dir_path_and_full_branch() {
            let fixture = GitFixture::new();
            let repo = fixture.repo("reconcile available");
            let snapshot = discover_repository(&repo).unwrap();
            let branch = snapshot.selected_worktree.branch.clone().unwrap();
            assert!(reconcile_checkout(
                &snapshot.common_dir,
                &snapshot.selected_worktree.path,
                Some(&branch),
            )
            .is_ok());

            let other = fixture.repo("other identity");
            assert!(matches!(
                reconcile_checkout(
                    &discover_repository(&other).unwrap().common_dir,
                    &snapshot.selected_worktree.path,
                    Some(&branch),
                ),
                Err(ReconciliationUnavailable::IdentityChanged { .. })
            ));
        }

        #[test]
        fn missing_changed_detached_and_locked_checkouts_fail_closed() {
            let fixture = GitFixture::new();
            let repo = fixture.repo("reconcile stale");
            let linked = fixture.linked_worktree(&repo, "reconcile linked", "linked");
            let expected = discover_repository(&linked).unwrap();
            let common = expected.common_dir.clone();
            let branch = expected.selected_worktree.branch.clone().unwrap();
            let linked = expected.selected_worktree.path;

            git_ok(&linked, &[OsStr::new("checkout"), OsStr::new("--detach")]);
            assert!(matches!(
                reconcile_checkout(&common, &linked, Some(&branch)),
                Err(ReconciliationUnavailable::Detached)
                    | Err(ReconciliationUnavailable::BranchChanged { .. })
            ));

            git_ok(
                &repo,
                &[
                    OsStr::new("worktree"),
                    OsStr::new("lock"),
                    linked.as_os_str(),
                ],
            );
            assert!(matches!(
                reconcile_checkout(&common, &linked, None),
                Err(ReconciliationUnavailable::LockedOrPrunable)
            ));

            let missing = fixture.root.join("missing checkout");
            assert!(matches!(
                reconcile_checkout(&common, &missing, Some(&branch)),
                Err(ReconciliationUnavailable::Missing { .. })
            ));
        }
    }

    fn parts(input: &str) -> (String, String, String, String) {
        let t = parse_clone_target(input).expect(input);
        (t.host, t.owner, t.repo, t.url)
    }

    #[test]
    fn scp_style_ssh() {
        assert_eq!(
            parts("git@github.com:poindexter12/baude.git"),
            (
                "github.com".into(),
                "poindexter12".into(),
                "baude".into(),
                "git@github.com:poindexter12/baude.git".into()
            )
        );
    }

    #[test]
    fn https_url_keeps_https() {
        let (host, owner, repo, url) = parts("https://github.com/poindexter12/baude");
        assert_eq!(host, "github.com");
        assert_eq!(owner, "poindexter12");
        assert_eq!(repo, "baude");
        assert_eq!(url, "https://github.com/poindexter12/baude.git");
    }

    #[test]
    fn browser_url_extra_segments_and_query() {
        let (_, owner, repo, _) =
            parts("https://github.com/poindexter12/baude/tree/main?tab=readme");
        assert_eq!(owner, "poindexter12");
        assert_eq!(repo, "baude");
    }

    #[test]
    fn ssh_scheme_url() {
        let (_, _, _, url) = parts("ssh://git@github.com/poindexter12/baude.git");
        assert_eq!(url, "git@github.com:poindexter12/baude.git");
    }

    #[test]
    fn shorthand_defaults_to_github_ssh() {
        let (host, owner, repo, url) = parts("poindexter12/baude");
        assert_eq!(host, "github.com");
        assert_eq!(owner, "poindexter12");
        assert_eq!(repo, "baude");
        assert_eq!(url, "git@github.com:poindexter12/baude.git");
    }

    #[test]
    fn schemeless_host_path() {
        let (host, _, _, url) = parts("github.com/poindexter12/baude");
        assert_eq!(host, "github.com");
        assert_eq!(url, "git@github.com:poindexter12/baude.git");
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_clone_target("").is_none());
        assert!(parse_clone_target("baude").is_none());
        assert!(parse_clone_target("https://github.com/").is_none());
        assert!(parse_clone_target("a/b/c").is_none());
    }
}
