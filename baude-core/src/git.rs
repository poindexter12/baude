use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use anyhow::{anyhow, Result};

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

/// Create a worktree for `branch` under the managed directory.
/// Creates the branch if it doesn't exist; otherwise checks out the existing one.
pub fn create_worktree(repo: &Path, branch: &str) -> Result<PathBuf> {
    let repo_name = repo
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "repo".to_string());
    let dir = worktrees_base()
        .join(sanitize(&repo_name))
        .join(sanitize(branch));
    if dir.exists() {
        // Reuse an existing managed worktree for this branch.
        return Ok(dir);
    }
    std::fs::create_dir_all(dir.parent().unwrap())?;
    let dir_str = dir.to_string_lossy().to_string();

    let new_branch = git(repo, &["worktree", "add", &dir_str, "-b", branch]);
    if new_branch.is_ok() {
        return Ok(dir);
    }
    // Branch may already exist — try checking it out instead.
    git(repo, &["worktree", "add", &dir_str, branch]).map_err(|e| anyhow!("{e}"))?;
    Ok(dir)
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

pub fn is_dirty(worktree: &Path) -> bool {
    git(worktree, &["status", "--porcelain"])
        .map(|s| !s.is_empty())
        .unwrap_or(false)
}

pub fn remove_worktree(repo: &Path, worktree: &Path) -> Result<()> {
    git(repo, &["worktree", "remove", &worktree.to_string_lossy()])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        discover_repository, ensure_default_worktree, parse_clone_target, parse_worktree_porcelain,
        resolve_default_branch, DefaultBranchUnavailable, DefaultWorktreeOutcome,
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
