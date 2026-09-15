//! Read-only enumeration and classification of candidate leaked managed
//! worktree directories under the real data root.
//!
//! # Why this module refuses to be confident
//!
//! 1433 `repository-<key>` directories sit under the developer's real
//! `~/.local/share/baude/worktrees` today (#72). The two weakest signals
//! available for deciding whether one of them is garbage are each satisfied by
//! directories that are provably live:
//!
//! - **Path shape.** Four directories under `worktrees/claude/` match the
//!   managed shape exactly and are not leaks — two hold real 39-entry
//!   checkouts, and two were created by *production* runs after the
//!   containment fix landed. The shape is a necessary filter and sufficient
//!   proof of nothing.
//! - **A missing gitdir.** All 1433 candidates lack a `.git` entry at any
//!   level, live ones included. Treating that as ownership would authorize
//!   deleting every one of them — precisely what TISO-04 forbids.
//!
//! So [`Evidence::ShapeMatch`] and [`Evidence::NoGitdir`] are recorded and
//! neither can clear a candidate. [`Verdict::Indeterminate`] is the value
//! returned whenever no other branch matches, including for an empty evidence
//! list, and the module is deliberately biased toward reporting a genuine leak
//! as indeterminate rather than the reverse.
//!
//! # The authorization predicate
//!
//! Agreed at a blocking human decision checkpoint (plan 08-04 task 1,
//! option `as-proposed`) *before* it was implemented, because a `Removable`
//! verdict is the input to an irreversible deletion under the developer's real
//! data root:
//!
//! > `Removable` requires `ShapeMatch` **and** `NotReferencedByState` **and**
//! > (`Empty` **or** `GitDisownsIt`), with none of the hard blockers
//! > `ReferencedByState`, `ContainsCheckout`, `IsSymlink`, `StateUnreadable`
//! > present. Everything else is `Indeterminate`.
//!
//! [`Evidence::blocking_role`] matches exhaustively with no wildcard arm, so
//! adding a variant later is a compile error rather than a silent widening of
//! the authorization.

use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// How a persisted state record was found to reference a candidate.
///
/// Populated by plan 08-05's state cross-reference; declared here so the
/// evidence vocabulary is stated in one place.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ReferenceMatch {
    /// The saved repository key matches the candidate's `repository-<key>`
    /// segment within its own workspace's state file.
    Key,
    /// A saved path equals the candidate path exactly.
    ExactPath,
    /// A saved path lies beneath the candidate.
    Descendant,
    /// A saved path is an ancestor of the candidate.
    Ancestor,
}

/// One independent signal observed about a candidate directory.
///
/// Every variant is a fact that was *observed*, never a conclusion. The
/// conclusion is [`classify`]'s, and it names the facts that produced it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Evidence {
    /// The path is exactly `<canonical base>/<workspace>/repository-<u64>`,
    /// with the key round-tripping through `str::parse::<u64>()`.
    ///
    /// Necessary, and on its own proof of nothing — see the module docs.
    ShapeMatch,
    /// A complete inventory of every workspace's state files was read and none
    /// of them references this candidate. Carries the workspaces, the exact
    /// filenames, and the filenames found absent, so "not referenced" can never
    /// be confused with "not checked" — and so plan 08-05's prune can compare
    /// the inventory it re-derives against the one the developer approved.
    ///
    /// Emitted **only** from a complete inventory. One unreadable file, one
    /// ambiguous directory entry, or one unresolvable reference anywhere in the
    /// scan withholds this signal from every candidate.
    NotReferencedByState {
        workspaces_checked: Vec<String>,
        files_checked: Vec<String>,
        files_absent: Vec<String>,
    },
    /// No `.git` entry exists at the candidate or in any immediate child.
    ///
    /// Recorded so a developer reading the report can see it. **Inert in the
    /// decision** — every candidate in the live dataset satisfies it.
    NoGitdir,
    /// The candidate contains no non-directory entry at any level.
    Empty,
    /// A gitdir exists, `discover_repository` succeeded, and the path is
    /// genuinely absent from git's own worktree inventory. A *failed* lookup
    /// is never recorded here: it is not a disownment.
    GitDisownsIt { owning_repository: PathBuf },
    /// The candidate holds real content. Hard blocker. `entries` counts the
    /// candidate's own directory entries (what `ls` shows), not the whole
    /// subtree — the walk short-circuits on the first non-directory it finds.
    ContainsCheckout { entries: usize },
    /// Persisted workspace state references this candidate. Hard blocker, and
    /// the only positive proof of liveness available.
    ReferencedByState {
        workspace: String,
        /// The saved repository key, when the match carried one. `None` for a
        /// path-overlap match against a record with no repository key.
        repository_key: Option<u64>,
        matched: ReferenceMatch,
    },
    /// The candidate is itself a symbolic link. Hard blocker: classifying on
    /// the target's properties while a removal acted on the link is how a
    /// deletion escapes the base entirely.
    IsSymlink,
    /// A state file that could reference candidates could not be read, or the
    /// inventory of state files could not be completed. Hard blocker **across
    /// the whole scan**, not merely for its own workspace: state can reference
    /// paths in another workspace, so inferring absence from a failed read
    /// could clear a live repository parent.
    ///
    /// `workspace` is [`ANY_WORKSPACE`] when the uncertainty is not attributable
    /// to one workspace (a config directory that could not be listed, an
    /// ambiguous state-like entry, a reference path that could not be placed).
    StateUnreadable {
        source: PathBuf,
        workspace: String,
        detail: String,
    },
}

/// The `workspace` label on a [`Evidence::StateUnreadable`] that belongs to the
/// whole inventory rather than to one workspace. Not a legal workspace name —
/// [`valid_workspace_segment`] rejects it — so it can never collide with one.
pub const ANY_WORKSPACE: &str = "*";

/// Whether a blocker proves the candidate is alive or merely prevents any
/// conclusion. Both refuse removal; they differ only in what the report says.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BlockingRole {
    ProvesLive,
    PreventsConclusion,
}

impl Evidence {
    /// The exhaustive blocker table.
    ///
    /// Deliberately written with no wildcard arm: a new [`Evidence`] variant
    /// fails to compile here until someone states, explicitly, whether it
    /// blocks. That is the only place authorization can be widened, and it
    /// cannot be widened by accident.
    fn blocking_role(&self) -> Option<BlockingRole> {
        match self {
            Self::ReferencedByState { .. } | Self::ContainsCheckout { .. } => {
                Some(BlockingRole::ProvesLive)
            }
            Self::IsSymlink | Self::StateUnreadable { .. } => {
                Some(BlockingRole::PreventsConclusion)
            }
            Self::ShapeMatch
            | Self::NotReferencedByState { .. }
            | Self::NoGitdir
            | Self::Empty
            | Self::GitDisownsIt { .. } => None,
        }
    }
}

/// The signal that satisfied the predicate's third clause.
///
/// `NoGitdir` is *not* a member, and that omission is the whole point: all
/// 1433 live candidates satisfy it, so admitting it would make the predicate a
/// no-op.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ClearingSignal {
    Empty,
    GitDisownsIt { owning_repository: PathBuf },
}

/// Why a candidate was cleared, carried by [`Verdict::Removable`] instead of a
/// bare flag so plan 08-05's prune can re-derive the same facts and compare
/// them, and so the report a developer reads names the reason rather than
/// asserting a conclusion.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RemovalProof {
    /// The workspaces whose state was read to establish non-reference.
    pub workspaces_checked: Vec<String>,
    /// The clearing signal that satisfied clause 3.
    pub clearing: ClearingSignal,
    /// Every signal observed, inert ones included, in observation order.
    pub observed: Vec<Evidence>,
}

/// A three-way conclusion about one candidate. `Indeterminate` is the default
/// and every case carries the evidence that produced it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Verdict {
    /// Positively proven alive. Never removable.
    Live { evidence: Vec<Evidence> },
    /// Not proven either way. Never removable. This is the fall-through.
    Indeterminate { evidence: Vec<Evidence> },
    /// Cleared by the agreed predicate, carrying its proof.
    Removable { proof: RemovalProof },
}

/// Apply the agreed authorization predicate to one candidate's observed
/// evidence.
///
/// Blockers are consulted first and win outright, so a hard blocker refuses
/// removal no matter how many clearing signals accompany it. Only then is the
/// clearing set required, explicitly and in full. Everything else — including
/// an empty evidence list — falls through to [`Verdict::Indeterminate`].
pub fn classify(evidence: Vec<Evidence>) -> Verdict {
    // Blockers first, and they win outright. Refusing before the clearing set
    // is even looked at is what makes "a hard blocker overrides any number of
    // clearing signals" a structural property rather than a rule someone has
    // to remember when adding the next signal.
    let mut proves_live = false;
    let mut prevents_conclusion = false;
    for signal in &evidence {
        match signal.blocking_role() {
            Some(BlockingRole::ProvesLive) => proves_live = true,
            Some(BlockingRole::PreventsConclusion) => prevents_conclusion = true,
            None => {}
        }
    }
    if proves_live {
        return Verdict::Live { evidence };
    }
    if prevents_conclusion {
        return Verdict::Indeterminate { evidence };
    }

    // Then require the agreed clearing set explicitly and in full. Clause 3
    // admits exactly `Empty` and `GitDisownsIt`; `NoGitdir` is not a member.
    let shape_matched = evidence.contains(&Evidence::ShapeMatch);
    let workspaces_checked = evidence.iter().find_map(|signal| match signal {
        Evidence::NotReferencedByState {
            workspaces_checked, ..
        } => Some(workspaces_checked.clone()),
        _ => None,
    });
    let clearing = evidence.iter().find_map(|signal| match signal {
        Evidence::Empty => Some(ClearingSignal::Empty),
        Evidence::GitDisownsIt { owning_repository } => Some(ClearingSignal::GitDisownsIt {
            owning_repository: owning_repository.clone(),
        }),
        _ => None,
    });

    match (shape_matched, workspaces_checked, clearing) {
        (true, Some(workspaces_checked), Some(clearing)) => Verdict::Removable {
            proof: RemovalProof {
                workspaces_checked,
                clearing,
                observed: evidence,
            },
        },
        // Everything else falls through, including the empty evidence list.
        _ => Verdict::Indeterminate { evidence },
    }
}

/// The audit trail of a state cross-reference: which workspaces and which exact
/// filenames were consulted, and which were found genuinely absent.
///
/// Carried by [`ScanReport`] so the developer reading a preview can see the
/// inventory the verdicts rest on, and so prune can require the inventory it
/// re-derives to agree with the approved one.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct StateInventorySummary {
    /// Every workspace name whose state files were derived and consulted,
    /// sorted. Always contains [`crate::workspace::DEFAULT`].
    pub workspaces_checked: Vec<String>,
    /// Every filename consulted under the config directory, sorted.
    pub files_checked: Vec<String>,
    /// The subset of `files_checked` that was successfully determined to be
    /// absent. A *checked* absence, never an unexamined one.
    pub files_absent: Vec<String>,
    /// `true` when every consulted file produced either a parsed state or a
    /// checked absence, and every reference in every parsed state could be
    /// placed on the filesystem.
    ///
    /// `false` withholds [`Evidence::NotReferencedByState`] from **every**
    /// candidate in the scan, across every workspace (threat T-08-16).
    pub complete: bool,
}

/// One ownership claim extracted from a parsed state file.
///
/// Both kinds are *exclusions from removal*. Neither authorizes anything.
#[derive(Clone, Debug, Eq, PartialEq)]
enum StateReference {
    /// A `SavedRepository.key` or `SavedCheckout.repository_key` recorded in
    /// workspace `workspace`. Protects `<base>/<workspace>/repository-<key>`
    /// and everything beneath it.
    ///
    /// Keys are **workspace-scoped**: repository 5 in `claude` says nothing
    /// about repository 5 in `opencode`.
    RepositoryKey { workspace: String, key: u64 },
    /// A recorded absolute path, in every form it could be placed on the
    /// filesystem. Matched across **all** workspaces: state in one workspace
    /// can legitimately name a path under another's directory, and a deletion
    /// does not care which file recorded it.
    Path {
        workspace: String,
        repository_key: Option<u64>,
        forms: Vec<PathBuf>,
    },
}

/// Everything a scan learned from persisted state, plus the uncertainty it
/// could not resolve.
#[derive(Clone, Debug, Default)]
struct StateInventory {
    summary: StateInventorySummary,
    references: Vec<StateReference>,
    /// Non-empty means the inventory is INCOMPLETE. Every item is attached to
    /// every candidate, so one unreadable file in one workspace withholds
    /// clearing from the entire scan.
    uncertainty: Vec<Evidence>,
}

/// The two state kinds. The TUI and the daemon keep separate files so they
/// never clobber each other's sessions, and either can reference a candidate.
///
/// Ordered longest-prefix-first so `daemon-state-<ws>.json` is attributed to
/// `daemon-state` rather than being read as a `state` file with a mangled
/// workspace segment.
const STATE_BASES: [&str; 2] = ["daemon-state", "state"];

/// A workspace name is filesystem- and URL-safe by construction
/// (`workspace::sanitize`), so anything outside `[A-Za-z0-9_-]` in a filename
/// segment is not a workspace and the file is not a state file we can attribute.
fn valid_workspace_segment(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Read every workspace's persisted state under `config_dir` and extract the
/// ownership claims it makes.
///
/// `worktree_workspaces` are the workspace directory names observed under the
/// worktrees base. They are UNIONED with the names discovered in the config
/// directory (and with the default workspace) because either source alone is
/// incomplete: a workspace can have state and no directory, or a directory and
/// no state, and missing either class silently converts "not referenced" into
/// "not checked".
fn state_inventory(
    config_dir: &Path,
    worktree_workspaces: &std::collections::BTreeSet<String>,
) -> StateInventory {
    let mut uncertainty: Vec<Evidence> = Vec::new();
    let mut unattributable = |source: PathBuf, detail: String| {
        uncertainty.push(Evidence::StateUnreadable {
            source,
            workspace: ANY_WORKSPACE.to_string(),
            detail,
        });
    };

    // The default workspace is always consulted: it is the one whose state can
    // exist under two different filenames, and the one a fresh install writes.
    let mut workspaces = std::collections::BTreeSet::new();
    workspaces.insert(crate::workspace::DEFAULT.to_string());

    // Half one of the union: names discovered in the config directory. A
    // workspace can hold records and own no directory under the base.
    match sorted_entries(config_dir) {
        Ok(entries) => {
            for entry in entries {
                let Some(name) = entry.to_str() else {
                    unattributable(
                        config_dir.join(&entry),
                        "a config directory entry is not valid UTF-8, so it cannot be \
                         compared against any derived state filename"
                            .to_string(),
                    );
                    continue;
                };
                match classify_config_entry(name) {
                    ConfigEntry::Irrelevant => {}
                    ConfigEntry::State { workspace } => {
                        workspaces.insert(workspace);
                    }
                    ConfigEntry::Ambiguous => unattributable(
                        config_dir.join(name),
                        "a state-like entry under a name no derivation produces; its bytes \
                         could name candidates and it was not read"
                            .to_string(),
                    ),
                }
            }
        }
        Err(error) => unattributable(
            config_dir.to_path_buf(),
            format!(
                "the config directory could not be listed, so no absence below it is a \
                 checked absence: {error}"
            ),
        ),
    }

    // Half two: names observed under the worktrees base. A workspace can own a
    // directory and have written no state yet, and deriving names from the
    // config directory alone would turn "never written" into "never checked".
    for name in worktree_workspaces {
        if valid_workspace_segment(name) {
            workspaces.insert(name.clone());
        } else {
            unattributable(
                config_dir.to_path_buf(),
                format!(
                    "workspace directory {name:?} is not a name any state filename can be \
                     derived for, so its records cannot be located"
                ),
            );
        }
    }

    // Every derived filename is read on its own. There is deliberately no
    // fallback from one to another: a readable `state-<ws>.json` says nothing
    // about an unreadable `daemon-state-<ws>.json`, and treating it as though
    // it did is how a daemon-recorded checkout becomes invisible (T-08-16).
    let mut files_checked: Vec<String> = Vec::new();
    let mut files_absent: Vec<String> = Vec::new();
    let mut references: Vec<StateReference> = Vec::new();
    for workspace in &workspaces {
        let handle = crate::workspace::Workspace {
            name: workspace.clone(),
            // `backend_for` is a pure lookup. `workspace::active()` would read
            // configuration — and panics outright in support builds after 08-03.
            backend: crate::backend::backend_for(None),
            daemon_url: None,
            daemon_port: None,
        };
        for base in STATE_BASES {
            let mut names = vec![handle.state_file(base)];
            names.extend(handle.legacy_state_file(base));
            for file in names {
                files_checked.push(file.clone());
                match crate::persist::load_named_at(config_dir, &file) {
                    Ok(crate::persist::LoadOutcome::Missing) => files_absent.push(file),
                    Ok(crate::persist::LoadOutcome::Legacy(loaded))
                    | Ok(crate::persist::LoadOutcome::Current(loaded)) => collect_references(
                        workspace,
                        &loaded.state,
                        &config_dir.join(&file),
                        &mut references,
                        &mut uncertainty,
                    ),
                    Err(error) => uncertainty.push(Evidence::StateUnreadable {
                        source: config_dir.join(&file),
                        workspace: workspace.clone(),
                        detail: error.to_string(),
                    }),
                }
            }
        }
    }

    files_checked.sort();
    files_checked.dedup();
    files_absent.sort();
    files_absent.dedup();

    StateInventory {
        summary: StateInventorySummary {
            workspaces_checked: workspaces.into_iter().collect(),
            files_checked,
            files_absent,
            // One unresolved item anywhere makes the whole inventory
            // incomplete, and an incomplete inventory clears nothing.
            complete: uncertainty.is_empty(),
        },
        references,
        uncertainty,
    }
}

/// What one entry in the config directory is, for inventory purposes.
enum ConfigEntry {
    /// Not state and not state-like: it cannot hold records naming candidates.
    Irrelevant,
    /// Exactly a filename [`crate::workspace::Workspace`] derives, for this
    /// workspace.
    State { workspace: String },
    /// State-like but attributable to no derived filename, so it is never read
    /// and its contents are unknown. Uncertainty, not noise.
    Ambiguous,
}

/// Classify a config-directory entry by name alone.
///
/// The bias is deliberate and one-directional: anything that *might* hold state
/// records and is not a name this module reads becomes [`ConfigEntry::Ambiguous`],
/// which withholds clearance from the entire scan. An interrupted atomic write
/// leaves `.state-<ws>.json.tmp-<pid>-<n>` behind — six sit in the developer's
/// real config directory today — and those bytes are state.
fn classify_config_entry(name: &str) -> ConfigEntry {
    // A lock is an advisory marker holding no records at all, and production
    // holds one for as long as the TUI runs. Treating it as uncertainty would
    // make this tool useless exactly when a developer reaches for it.
    let hidden = name.starts_with('.');
    if hidden && name.ends_with(".lock") {
        return ConfigEntry::Irrelevant;
    }
    let stem = name.strip_prefix('.').unwrap_or(name);
    let Some(base) = STATE_BASES.iter().find(|base| stem.starts_with(**base)) else {
        return ConfigEntry::Irrelevant;
    };
    // A hidden entry is never a name `Workspace::state_file` produces, so even a
    // perfectly-shaped `.state-claude.json` is something else — and something
    // else holding state bytes is exactly the ambiguous case.
    if hidden {
        return ConfigEntry::Ambiguous;
    }
    let rest = &stem[base.len()..];
    if rest == ".json" {
        // The legacy un-suffixed form, which only the default workspace has.
        return ConfigEntry::State {
            workspace: crate::workspace::DEFAULT.to_string(),
        };
    }
    match rest
        .strip_prefix('-')
        .and_then(|rest| rest.strip_suffix(".json"))
    {
        Some(segment) if valid_workspace_segment(segment) => ConfigEntry::State {
            workspace: segment.to_string(),
        },
        // Includes names that merely begin with a state base — over-cautious by
        // construction, and the over-caution costs a refused removal rather
        // than an unrecoverable one.
        _ => ConfigEntry::Ambiguous,
    }
}

/// Extract every ownership claim one parsed state file makes.
///
/// Repository keys and paths are collected separately because they are matched
/// differently: a key is workspace-scoped, a path is not.
fn collect_references(
    workspace: &str,
    state: &crate::repository::RepositoryState,
    source: &Path,
    references: &mut Vec<StateReference>,
    uncertainty: &mut Vec<Evidence>,
) {
    let mut claim = |repository_key: Option<u64>,
                     persisted: &crate::repository::PersistedPath,
                     references: &mut Vec<StateReference>| {
        let path = persisted.to_path_buf();
        if !path.is_absolute() {
            // A relative recorded path cannot be placed against a canonicalized
            // candidate without inventing the root it was relative to. Refuse
            // to guess: this is uncertainty, not an absent claim.
            uncertainty.push(Evidence::StateUnreadable {
                source: source.to_path_buf(),
                workspace: ANY_WORKSPACE.to_string(),
                detail: format!(
                    "a recorded path is not absolute and cannot be placed: {}",
                    path.display()
                ),
            });
            return;
        }
        references.push(StateReference::Path {
            workspace: workspace.to_string(),
            repository_key,
            forms: reference_forms(&path),
        });
    };

    for repository in &state.repositories {
        let key = repository.key.get();
        references.push(StateReference::RepositoryKey {
            workspace: workspace.to_string(),
            key,
        });
        claim(Some(key), &repository.observed_main_worktree, references);
        claim(Some(key), &repository.observed_common_dir, references);
    }
    for checkout in &state.checkouts {
        let key = checkout.repository_key.get();
        references.push(StateReference::RepositoryKey {
            workspace: workspace.to_string(),
            key,
        });
        claim(Some(key), &checkout.observed_path, references);
        claim(Some(key), &checkout.session.cwd, references);
        claim(Some(key), &checkout.session.repo_root, references);
    }
    for standalone in &state.standalone_sessions {
        // Standalone sessions carry no repository key, so the claim they make
        // is purely positional.
        claim(None, &standalone.canonical_path, references);
    }
}

/// Every form a recorded path could take on this filesystem: the bytes as
/// recorded, and the same path with its longest existing ancestor resolved.
///
/// Both are kept because either can be the one that matches. The candidate side
/// is always canonical (the scan canonicalizes its base up front), but a
/// process that never resolved its own root persisted the unresolved form — and
/// on macOS the temporary and data roots both reach the tree through a symlink.
fn reference_forms(path: &Path) -> Vec<PathBuf> {
    let mut forms = vec![path.to_path_buf()];
    if let Some(resolved) = resolve_prefix(path) {
        if !forms.contains(&resolved) {
            forms.push(resolved);
        }
    }
    forms
}

/// How far up [`resolve_prefix`] will walk looking for an ancestor that exists.
/// Deeper than any managed path, and bounded so a pathological recorded value
/// cannot spin.
const MAX_PREFIX_WALK: usize = 64;

/// Canonicalize the longest existing ancestor of `path` and re-append the rest.
///
/// A recorded path whose leaf is gone — a checkout that was removed, the very
/// case this tool exists for — still has to be placed: it may name a directory
/// beneath a candidate that is still on disk, and a plain `canonicalize` of the
/// whole path would fail and silently read as "no claim".
fn resolve_prefix(path: &Path) -> Option<PathBuf> {
    let mut remainder: Vec<&std::ffi::OsStr> = Vec::new();
    let mut cursor = path;
    for _ in 0..MAX_PREFIX_WALK {
        if let Ok(resolved) = cursor.canonicalize() {
            let mut out = resolved;
            out.extend(remainder.iter().rev());
            return Some(out);
        }
        remainder.push(cursor.file_name()?);
        cursor = cursor.parent()?;
    }
    None
}

/// The state signals for one candidate.
///
/// Returns the matches when state claims the candidate, the single negative
/// signal when a COMPLETE inventory claims nothing, and nothing at all when the
/// inventory is incomplete — in that last case the uncertainty already attached
/// to every candidate is the honest answer, and asserting non-reference on top
/// of it would be the lie this module is built to prevent.
fn state_evidence(
    inventory: &StateInventory,
    workspace: &str,
    repository_key: u64,
    path: &Path,
) -> Vec<Evidence> {
    let mut matches: Vec<Evidence> = Vec::new();
    for reference in &inventory.references {
        match reference {
            StateReference::RepositoryKey {
                workspace: recorded,
                key,
            } => {
                // Keys are workspace-scoped, and the comparison is on the parsed
                // `u64` rather than the directory name, so `repository-1` is
                // never a prefix claim on `repository-10`.
                if recorded == workspace && *key == repository_key {
                    matches.push(Evidence::ReferencedByState {
                        workspace: recorded.clone(),
                        repository_key: Some(*key),
                        matched: ReferenceMatch::Key,
                    });
                }
            }
            StateReference::Path {
                workspace: recorded,
                repository_key,
                forms,
            } => {
                for form in forms {
                    // `Path::starts_with` compares whole components, so
                    // `repository-1` is not an ancestor of `repository-10`.
                    let matched = if form == path {
                        ReferenceMatch::ExactPath
                    } else if form.starts_with(path) {
                        ReferenceMatch::Descendant
                    } else if path.starts_with(form) {
                        ReferenceMatch::Ancestor
                    } else {
                        continue;
                    };
                    matches.push(Evidence::ReferencedByState {
                        workspace: recorded.clone(),
                        repository_key: *repository_key,
                        matched,
                    });
                }
            }
        }
    }

    if !matches.is_empty() {
        // One record commonly makes the same claim through several fields (a
        // checkout's `observed_path` and its session `cwd` are the same
        // directory), and both recorded forms of one path can match.
        sort_evidence(&mut matches);
        matches.dedup();
        return matches;
    }
    if !inventory.summary.complete {
        return Vec::new();
    }
    vec![Evidence::NotReferencedByState {
        workspaces_checked: inventory.summary.workspaces_checked.clone(),
        files_checked: inventory.summary.files_checked.clone(),
        files_absent: inventory.summary.files_absent.clone(),
    }]
}

/// The two roots a scan reads. Both are explicit so filesystem tests can pass
/// synthetic roots and never resolve a developer directory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScanRoots {
    /// The managed-worktree root to enumerate.
    pub worktrees_base: PathBuf,
    /// The config directory holding every workspace's state files. Read by
    /// plan 08-05's state cross-reference; enumeration itself never opens it.
    pub config_dir: PathBuf,
}

/// One shaped directory found under the worktrees root, with the conclusion
/// drawn about it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Candidate {
    /// The candidate's path, resolved under the canonicalized base.
    pub path: PathBuf,
    /// The workspace segment it sits under.
    pub workspace: String,
    /// The `repository-<key>` key, parsed as a `u64`.
    pub repository_key: u64,
    /// The verdict, which carries the evidence that produced it.
    pub verdict: Verdict,
}

/// The output of a scan. This is the tool's *only* output: nothing is created,
/// modified or removed to produce it (D-16).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScanReport {
    /// The canonicalized base every candidate was resolved under.
    pub base: PathBuf,
    pub candidates: Vec<Candidate>,
    /// The state files and workspaces the cross-reference actually consulted.
    /// A developer reading a preview can see the inventory the verdicts rest
    /// on; prune requires the inventory it re-derives to agree with it.
    pub state_inventory: StateInventorySummary,
}

/// A scan cannot start. Per-candidate problems are evidence, not errors — only
/// a base that cannot be read at all stops the scan.
#[derive(Debug)]
pub enum ScanError {
    BaseUnreadable { path: PathBuf, detail: String },
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BaseUnreadable { path, detail } => {
                write!(
                    f,
                    "managed worktree root {} could not be read: {detail}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for ScanError {}

/// Scan the developer's real managed-worktree root.
///
/// The production wrapper, and the only place a real root is resolved. Every
/// test calls [`scan_at`] with synthetic roots instead.
pub fn scan() -> Result<ScanReport, ScanError> {
    scan_at(&ScanRoots {
        worktrees_base: crate::git::real_worktrees_base(),
        config_dir: crate::persist::config_dir(),
    })
}

/// Enumerate and classify candidates under an explicit worktrees root.
///
/// Read-only, unconditionally: no temporary file, no lock, no probe directory,
/// not even to test writability.
pub fn scan_at(roots: &ScanRoots) -> Result<ScanReport, ScanError> {
    // Canonicalize the base once, up front. Every candidate is then required to
    // stay under this value, so no link or `..` segment discovered during the
    // walk can move the scan somewhere else.
    let base = match roots.worktrees_base.canonicalize() {
        Ok(base) => base,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // No worktrees root means no candidates, so there is nothing for a
            // state cross-reference to exclude. The inventory is deliberately
            // not read: it would name workspaces no candidate was compared
            // against, which is the "checked" claim this module exists to keep
            // honest.
            return Ok(ScanReport {
                base: roots.worktrees_base.clone(),
                candidates: Vec::new(),
                state_inventory: StateInventorySummary::default(),
            });
        }
        Err(error) => {
            return Err(ScanError::BaseUnreadable {
                path: roots.worktrees_base.clone(),
                detail: error.to_string(),
            });
        }
    };

    // The tree is exactly three levels — base, workspace, `repository-<key>`,
    // optional child — so it is walked with an explicit `read_dir` per level
    // rather than a generic recursive walker. A generic walker would weaken the
    // shape check, which is the first filter and the only thing bounding how
    // much of the filesystem this tool ever looks at.
    //
    // Pass one collects filesystem evidence and the workspace names actually
    // present. Pass two adds state evidence, which cannot run first: the state
    // inventory's expected-filename set is the UNION of the workspaces seen here
    // with the ones discovered in the config directory.
    let mut observed: Vec<(PathBuf, String, u64, Vec<Evidence>)> = Vec::new();
    let mut workspaces = std::collections::BTreeSet::new();
    for workspace_entry in sorted_entries(&base).map_err(|error| ScanError::BaseUnreadable {
        path: base.clone(),
        detail: error.to_string(),
    })? {
        let workspace_path = base.join(&workspace_entry);
        // Non-following metadata: `is_dir()` is false for a symlink here, so a
        // symlinked workspace is skipped rather than descended into.
        let Ok(metadata) = std::fs::symlink_metadata(&workspace_path) else {
            continue;
        };
        if !metadata.is_dir() {
            continue;
        }
        let Some(workspace) = workspace_entry.to_str().map(str::to_owned) else {
            continue;
        };
        workspaces.insert(workspace.clone());
        let Ok(entries) = sorted_entries(&workspace_path) else {
            continue;
        };
        for entry in entries {
            let Some(name) = entry.to_str() else {
                continue;
            };
            // A parse failure — including a `u64` overflow and a zero-padded
            // key that does not round-trip — is a shape mismatch to skip, never
            // an error that fails the scan.
            let Some(repository_key) = repository_key(name) else {
                continue;
            };
            let path = workspace_path.join(name);
            let Some(evidence) = candidate_evidence(&base, &path) else {
                continue;
            };
            observed.push((path, workspace.clone(), repository_key, evidence));
        }
    }

    // Pass two. The inventory is read ONCE for the whole scan, not per
    // candidate: a single read is what makes "these workspaces were checked"
    // one fact that every verdict in the report shares, and what lets one
    // unreadable file withhold clearing from all of them at once.
    let inventory = state_inventory(&roots.config_dir, &workspaces);

    let mut candidates: Vec<Candidate> = observed
        .into_iter()
        .map(|(path, workspace, repository_key, mut evidence)| {
            evidence.extend(state_evidence(
                &inventory,
                &workspace,
                repository_key,
                &path,
            ));
            evidence.extend(inventory.uncertainty.iter().cloned());
            sort_evidence(&mut evidence);
            Candidate {
                path,
                workspace,
                repository_key,
                verdict: classify(evidence),
            }
        })
        .collect();
    // Sorted by (workspace, key) rather than by directory-iteration order, so
    // two scans of an unchanged tree compare and serialize identically.
    candidates.sort_by(|left, right| {
        (&left.workspace, left.repository_key).cmp(&(&right.workspace, right.repository_key))
    });

    Ok(ScanReport {
        base,
        candidates,
        state_inventory: inventory.summary,
    })
}

/// Canonical evidence ordering, so a report round-trip and a prune-time
/// re-derivation compare by *content* rather than by the order two filesystem
/// walks happened to observe things in.
fn sort_evidence(evidence: &mut [Evidence]) {
    evidence.sort_by_key(|signal| (signal.rank(), format!("{signal:?}")));
}

impl Evidence {
    /// Stable ordering rank. Paired with the `Debug` rendering as a tiebreak so
    /// two signals of the same kind (two state references, two unreadable
    /// files) also order deterministically.
    fn rank(&self) -> u8 {
        match self {
            Self::ShapeMatch => 0,
            Self::ReferencedByState { .. } => 1,
            Self::NotReferencedByState { .. } => 2,
            Self::StateUnreadable { .. } => 3,
            Self::IsSymlink => 4,
            Self::ContainsCheckout { .. } => 5,
            Self::Empty => 6,
            Self::NoGitdir => 7,
            Self::GitDisownsIt { .. } => 8,
        }
    }
}

/// Directory entry names, sorted, so a report is stable across runs.
fn sorted_entries(path: &Path) -> std::io::Result<Vec<OsString>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(path)? {
        names.push(entry?.file_name());
    }
    names.sort();
    Ok(names)
}

/// The `repository-<u64>` key, or `None` when the name is not that shape.
///
/// The composed name has to round-trip, so `repository-007` and `repository-+7`
/// are mismatches rather than aliases for key 7 — only what
/// [`crate::git::managed_default_worktree_path`] composes is a candidate.
fn repository_key(name: &str) -> Option<u64> {
    let key: u64 = name.strip_prefix("repository-")?.parse().ok()?;
    (format!("repository-{key}") == name).then_some(key)
}

/// Classify one shaped path, or `None` when it is not a candidate at all.
fn candidate_evidence(base: &Path, path: &Path) -> Option<Vec<Evidence>> {
    // Non-following metadata for every classification decision. The following
    // variant, and the plain existence check, both traverse symlinks: a
    // symlinked candidate pointing outside the base would be classified on its
    // target's properties while a later removal acted on the link, which is how
    // a deletion escapes the base entirely.
    let metadata = std::fs::symlink_metadata(path).ok()?;
    let mut evidence = vec![Evidence::ShapeMatch];
    if metadata.file_type().is_symlink() {
        // Recorded and refused here, without ever resolving the target. This
        // mirrors the refusal already on the verified-removal path.
        evidence.push(Evidence::IsSymlink);
        return Some(evidence);
    }
    if !metadata.is_dir() {
        // A regular file that happens to carry the shape is not a candidate.
        return None;
    }
    // Containment: a resolved path that left the canonical base is refused
    // outright rather than reported.
    let resolved = path.canonicalize().ok()?;
    if !resolved.starts_with(base) {
        return None;
    }

    evidence.extend(contents_evidence(path));
    evidence.extend(gitdir_evidence(path));
    Some(evidence)
}

/// How deep the emptiness walk goes before giving up. The managed tree is three
/// levels; anything deeper is treated as non-empty, which fails closed.
const MAX_EMPTY_DEPTH: u32 = 16;

/// `Empty` when the candidate holds no non-directory entry at any level,
/// `ContainsCheckout` when it does, and nothing at all when the directory could
/// not be read — an unreadable candidate stays [`Verdict::Indeterminate`]
/// rather than being labelled either way.
fn contents_evidence(path: &Path) -> Option<Evidence> {
    let entries = sorted_entries(path).ok()?;
    if entries.is_empty() {
        return Some(Evidence::Empty);
    }
    match empty_at_every_level(path, MAX_EMPTY_DEPTH) {
        Ok(true) => Some(Evidence::Empty),
        Ok(false) => Some(Evidence::ContainsCheckout {
            entries: entries.len(),
        }),
        Err(_) => None,
    }
}

/// Short-circuits on the first non-directory entry, so a populated checkout
/// costs one `read_dir` rather than a full subtree walk.
fn empty_at_every_level(path: &Path, depth: u32) -> std::io::Result<bool> {
    if depth == 0 {
        return Ok(false);
    }
    for entry in std::fs::read_dir(path)? {
        let child = entry?.path();
        let metadata = std::fs::symlink_metadata(&child)?;
        // A symlink is an entry like any other: `is_dir()` is false for it
        // here, so the tree is correctly not empty and is never followed.
        if !metadata.is_dir() {
            return Ok(false);
        }
        if !empty_at_every_level(&child, depth - 1)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// `NoGitdir` when no `.git` entry exists at the candidate or in any immediate
/// child, `GitDisownsIt` when git ran and did not list the path, and nothing at
/// all when the lookup failed — a failed command is not a disownment.
fn gitdir_evidence(path: &Path) -> Option<Evidence> {
    let Some(holder) = gitdir_holder(path) else {
        return Some(Evidence::NoGitdir);
    };
    let resolved = holder.canonicalize().ok()?;
    // `discover_repository` is the wrong instrument here: it requires its input
    // to be a member of the inventory and errors otherwise, which is precisely
    // the disowned case. The inventory is therefore read directly.
    let inventory = crate::git::worktree_inventory(&resolved).ok()?;
    if inventory.iter().any(|record| record.path == resolved) {
        return None;
    }
    let owning_repository = inventory.first()?.path.clone();
    Some(Evidence::GitDisownsIt { owning_repository })
}

/// The candidate itself or an immediate child holding a `.git` entry. The
/// managed shape puts the gitdir on the checkout child
/// (`repository-<key>/primary-<key>/.git`), so two levels is the whole search.
fn gitdir_holder(path: &Path) -> Option<PathBuf> {
    if std::fs::symlink_metadata(path.join(".git")).is_ok() {
        return Some(path.to_path_buf());
    }
    for child in sorted_entries(path).ok()? {
        let child = path.join(child);
        let Ok(metadata) = std::fs::symlink_metadata(&child) else {
            continue;
        };
        if metadata.is_dir() && std::fs::symlink_metadata(child.join(".git")).is_ok() {
            return Some(child);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspaces() -> Vec<String> {
        vec!["claude".to_string(), "opencode".to_string()]
    }

    fn not_referenced() -> Evidence {
        Evidence::NotReferencedByState {
            workspaces_checked: workspaces(),
            files_checked: vec![
                "daemon-state-claude.json".to_string(),
                "state-claude.json".to_string(),
            ],
            files_absent: vec!["daemon-state-claude.json".to_string()],
        }
    }

    /// The authorization predicate, asserted directly over synthetic evidence
    /// with no filesystem involved. These are the cheapest and most direct
    /// statement of the TISO-04 wording, and they are meant to be readable as
    /// such by someone auditing the rule later.
    mod verdict {
        use super::*;

        #[test]
        fn shape_alone_is_indeterminate() {
            assert_eq!(
                classify(vec![Evidence::ShapeMatch]),
                Verdict::Indeterminate {
                    evidence: vec![Evidence::ShapeMatch]
                }
            );
        }

        /// The literal TISO-04 prohibition: a missing gitdir is the single most
        /// dangerous signal in the dataset, because all 1433 candidates satisfy
        /// it — live ones included.
        #[test]
        fn shape_plus_a_missing_gitdir_is_indeterminate() {
            let evidence = vec![Evidence::ShapeMatch, Evidence::NoGitdir];
            assert_eq!(
                classify(evidence.clone()),
                Verdict::Indeterminate { evidence }
            );
        }

        /// Even with the state cross-reference satisfied, a missing gitdir
        /// cannot stand in for the clearing signal.
        #[test]
        fn a_missing_gitdir_cannot_stand_in_for_the_clearing_signal() {
            let evidence = vec![Evidence::ShapeMatch, not_referenced(), Evidence::NoGitdir];
            assert_eq!(
                classify(evidence.clone()),
                Verdict::Indeterminate { evidence }
            );
        }

        #[test]
        fn an_empty_evidence_list_is_indeterminate() {
            assert_eq!(
                classify(Vec::new()),
                Verdict::Indeterminate {
                    evidence: Vec::new()
                }
            );
        }

        #[test]
        fn the_agreed_evidence_set_clears_a_candidate() {
            let verdict = classify(vec![
                Evidence::ShapeMatch,
                not_referenced(),
                Evidence::NoGitdir,
                Evidence::Empty,
            ]);
            let Verdict::Removable { proof } = verdict else {
                panic!("the full agreed evidence set must clear: {verdict:?}");
            };
            assert_eq!(proof.clearing, ClearingSignal::Empty);
            assert_eq!(proof.workspaces_checked, workspaces());
        }

        /// `Removable` carries its proof rather than a bare boolean, so plan
        /// 08-05's re-verification has something concrete to re-derive.
        #[test]
        fn removable_carries_the_evidence_that_cleared_it() {
            let evidence = vec![
                Evidence::ShapeMatch,
                not_referenced(),
                Evidence::NoGitdir,
                Evidence::Empty,
            ];
            let Verdict::Removable { proof } = classify(evidence.clone()) else {
                panic!("the full agreed evidence set must clear");
            };
            assert_eq!(proof.observed, evidence);
            assert!(proof.observed.contains(&Evidence::NoGitdir));
        }

        #[test]
        fn git_disownment_is_an_accepted_clearing_signal() {
            let owner = PathBuf::from("/fixture/repo");
            let verdict = classify(vec![
                Evidence::ShapeMatch,
                not_referenced(),
                Evidence::GitDisownsIt {
                    owning_repository: owner.clone(),
                },
            ]);
            let Verdict::Removable { proof } = verdict else {
                panic!("git disownment is clause 3's second member");
            };
            assert_eq!(
                proof.clearing,
                ClearingSignal::GitDisownsIt {
                    owning_repository: owner
                }
            );
        }

        #[test]
        fn a_shape_match_without_the_state_check_is_indeterminate() {
            let evidence = vec![Evidence::ShapeMatch, Evidence::Empty];
            assert_eq!(
                classify(evidence.clone()),
                Verdict::Indeterminate { evidence }
            );
        }

        #[test]
        fn a_state_reference_blocks_every_clearing_signal() {
            let verdict = classify(vec![
                Evidence::ShapeMatch,
                not_referenced(),
                Evidence::Empty,
                Evidence::GitDisownsIt {
                    owning_repository: PathBuf::from("/fixture/repo"),
                },
                Evidence::ReferencedByState {
                    workspace: "claude".to_string(),
                    repository_key: Some(5),
                    matched: ReferenceMatch::Key,
                },
            ]);
            assert!(
                matches!(verdict, Verdict::Live { .. }),
                "a referenced candidate is live: {verdict:?}"
            );
        }

        #[test]
        fn a_contained_checkout_blocks_every_clearing_signal() {
            let verdict = classify(vec![
                Evidence::ShapeMatch,
                not_referenced(),
                Evidence::Empty,
                Evidence::ContainsCheckout { entries: 39 },
            ]);
            assert!(
                matches!(verdict, Verdict::Live { .. }),
                "a populated candidate is live: {verdict:?}"
            );
        }

        #[test]
        fn a_symlink_blocks_every_clearing_signal() {
            let verdict = classify(vec![
                Evidence::ShapeMatch,
                not_referenced(),
                Evidence::Empty,
                Evidence::IsSymlink,
            ]);
            assert!(
                matches!(verdict, Verdict::Indeterminate { .. }),
                "a symlink is refused outright: {verdict:?}"
            );
        }

        #[test]
        fn an_unreadable_state_file_blocks_every_clearing_signal() {
            let verdict = classify(vec![
                Evidence::ShapeMatch,
                not_referenced(),
                Evidence::Empty,
                Evidence::StateUnreadable {
                    source: PathBuf::from("/fixture/config/state-claude.json"),
                    workspace: "claude".to_string(),
                    detail: "malformed".to_string(),
                },
            ]);
            assert!(
                matches!(verdict, Verdict::Indeterminate { .. }),
                "an unreadable inventory blocks clearing: {verdict:?}"
            );
        }
    }

    use crate::persist::StateFile;
    use crate::repository::{
        CheckoutLifecycle, CheckoutRole, PersistedPath, RepositoryHealth, RepositoryState,
        RetainedSessionState, RetainedStandaloneSessionState, SavedCheckout, SavedRepository,
        SavedStandaloneSession, StandaloneLifecycle,
    };
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

    /// A tree shaped exactly like the real one, under a unique temp root.
    /// Same convention as `git::tests::GitFixture`: an atomic counter in the
    /// root name so parallel cases cannot collide, and a `Drop` that cleans
    /// up and swallows its errors.
    struct ScanFixture {
        root: PathBuf,
    }

    impl ScanFixture {
        fn new() -> Self {
            let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir()
                .join(format!("baude-scan-test-{}-{sequence}", std::process::id()));
            std::fs::create_dir(&root).expect("create unique scan fixture root");
            std::fs::create_dir(root.join("worktrees")).expect("create fixture worktrees base");
            std::fs::create_dir(root.join("config")).expect("create fixture config dir");
            Self { root }
        }

        fn base(&self) -> PathBuf {
            self.root.join("worktrees")
        }

        fn config(&self) -> PathBuf {
            self.root.join("config")
        }

        fn roots(&self) -> ScanRoots {
            ScanRoots {
                worktrees_base: self.base(),
                config_dir: self.config(),
            }
        }

        /// Create `<base>/<workspace>/<relative>` and return it.
        fn dir(&self, workspace: &str, relative: &str) -> PathBuf {
            let path = self.base().join(workspace).join(relative);
            std::fs::create_dir_all(&path).expect("create fixture directory");
            path
        }

        fn workspace(&self, workspace: &str) -> PathBuf {
            let path = self.base().join(workspace);
            std::fs::create_dir_all(&path).expect("create fixture workspace");
            path
        }

        /// A real single-commit repository, outside the worktrees base.
        fn git_repo(&self, name: &str) -> PathBuf {
            let repo = self.root.join(name);
            std::fs::create_dir_all(&repo).expect("create fixture repo dir");
            git_ok(&repo, &["init", "-q", "."]);
            git_ok(&repo, &["config", "user.name", "Baude Test"]);
            git_ok(&repo, &["config", "user.email", "baude@example.invalid"]);
            std::fs::write(repo.join("tracked.txt"), b"fixture\n").expect("write fixture file");
            git_ok(&repo, &["add", "tracked.txt"]);
            git_ok(&repo, &["commit", "-q", "-m", "fixture"]);
            repo.canonicalize().expect("canonicalize fixture repo")
        }
    }

    impl Drop for ScanFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn git_ok(cwd: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .output()
            .expect("run fixture git command");
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn scan_ok(fixture: &ScanFixture) -> ScanReport {
        scan_at(&fixture.roots()).expect("scan a synthetic fixture root")
    }

    fn reported(report: &ScanReport) -> Vec<String> {
        let mut names: Vec<String> = report
            .candidates
            .iter()
            .map(|found| {
                format!(
                    "{}/{}",
                    found.workspace,
                    found
                        .path
                        .file_name()
                        .expect("candidate has a final segment")
                        .to_string_lossy()
                )
            })
            .collect();
        names.sort();
        names
    }

    fn candidate<'a>(report: &'a ScanReport, workspace: &str, name: &str) -> &'a Candidate {
        report
            .candidates
            .iter()
            .find(|found| found.workspace == workspace && found.path.ends_with(name))
            .unwrap_or_else(|| panic!("candidate {workspace}/{name} missing from {report:?}"))
    }

    fn evidence(found: &Candidate) -> &[Evidence] {
        match &found.verdict {
            Verdict::Live { evidence } | Verdict::Indeterminate { evidence } => evidence,
            Verdict::Removable { proof } => &proof.observed,
        }
    }

    /// Path, type, length and mtime for every entry beneath `root`.
    /// Reading a directory touches atime, never mtime, so a scan that
    /// writes nothing leaves this value identical.
    fn tree_snapshot(root: &Path) -> Vec<String> {
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let entries = std::fs::read_dir(&dir).expect("read fixture directory");
            for entry in entries {
                let path = entry.expect("fixture directory entry").path();
                let meta = std::fs::symlink_metadata(&path).expect("stat fixture entry");
                let modified = meta
                    .modified()
                    .map(|time| format!("{time:?}"))
                    .unwrap_or_else(|error| format!("{error}"));
                out.push(format!(
                    "{}|{:?}|{}|{modified}",
                    path.display(),
                    meta.file_type(),
                    meta.len()
                ));
                if meta.is_dir() {
                    stack.push(path);
                }
            }
        }
        out.sort();
        out
    }

    /// Persisted-state fixtures.
    ///
    /// State files are written with a plain `fs::write` of the serialized
    /// `StateFile` rather than through `persist::save_current_at`, which calls
    /// `hold_state_lock` and would leave a `.state-<ws>.json.lock` in the very
    /// directory these cases assert is untouched.
    impl ScanFixture {
        fn write_state(&self, file: &str, state: RepositoryState) {
            let bytes =
                serde_json::to_vec_pretty(&StateFile::new(state)).expect("serialize fixture state");
            std::fs::write(self.config().join(file), bytes).expect("write fixture state file");
        }

        fn write_raw(&self, file: &str, bytes: &[u8]) {
            std::fs::write(self.config().join(file), bytes).expect("write raw fixture file");
        }

        /// A directory outside the worktrees base, standing in for a repository
        /// the developer keeps in `~/Code` — the shape the two real
        /// `claude/repository-1` and `repository-5` records have.
        fn external(&self, name: &str) -> PathBuf {
            let path = self.root.join("external").join(name);
            std::fs::create_dir_all(&path).expect("create external fixture path");
            path.canonicalize().expect("canonicalize external path")
        }
    }

    /// Builds a `RepositoryState` that satisfies `RepositoryState::validate`,
    /// so every fixture exercises the same strict loader production uses.
    struct StateBuilder {
        state: RepositoryState,
    }

    impl StateBuilder {
        fn new() -> Self {
            Self {
                state: RepositoryState::default(),
            }
        }

        fn order(&mut self) -> u64 {
            let order = self.state.next_first_seen_order;
            self.state.next_first_seen_order += 1;
            order
        }

        /// A repository recorded under an EXACT key, whose observed main
        /// worktree is external to the managed base.
        fn repository(mut self, key: u64, main_worktree: &Path) -> Self {
            assert!(
                key >= self.state.next_repository_key,
                "fixture repository keys are allocated in ascending order"
            );
            self.state.next_repository_key = key;
            let key = self
                .state
                .allocate_repository_key()
                .expect("allocate fixture repository key");
            let order = self.order();
            self.state.repositories.push(SavedRepository {
                key,
                observed_common_dir: PersistedPath::from_path(&main_worktree.join(".git")),
                observed_main_worktree: PersistedPath::from_path(main_worktree),
                first_seen_order: order,
                health: RepositoryHealth::Available,
            });
            self
        }

        /// A checkout of `repository_key` at `path`. `managed` is
        /// `managed_by_baude`; the real repo-1/repo-5 records carry `false`,
        /// which must not weaken the protection their key supplies.
        fn checkout(mut self, repository_key: u64, path: &Path, managed: bool) -> Self {
            let repository = self
                .state
                .repositories
                .iter()
                .find(|candidate| candidate.key.get() == repository_key)
                .cloned()
                .expect("fixture checkout references a recorded repository");
            let main = repository.observed_main_worktree.to_path_buf();
            let key = self
                .state
                .allocate_checkout_key()
                .expect("allocate fixture checkout key");
            let order = self.order();
            let role = if path == main {
                CheckoutRole::Main
            } else {
                CheckoutRole::ManagedBranch
            };
            self.state.checkouts.push(SavedCheckout::new(
                key,
                repository.key,
                role,
                managed,
                PersistedPath::from_path(path),
                Some("main".to_string()),
                order,
                CheckoutLifecycle::Inactive,
                RetainedSessionState {
                    name: "fixture".to_string(),
                    cwd: PersistedPath::from_path(path),
                    repo_root: repository.observed_main_worktree.clone(),
                    branch: Some("main".to_string()),
                    is_worktree: path != main,
                    shell_open: false,
                    archived: false,
                    archived_by_user: false,
                    resume_id: None,
                },
            ));
            self
        }

        /// A non-Git standalone session. Its `canonical_path` is an ownership
        /// claim exactly like a checkout's `observed_path`.
        fn standalone(mut self, path: &Path) -> Self {
            let key = self
                .state
                .allocate_standalone_key()
                .expect("allocate fixture standalone key");
            let order = self.order();
            self.state
                .standalone_sessions
                .push(SavedStandaloneSession::new(
                    key,
                    PersistedPath::from_path(path),
                    order,
                    StandaloneLifecycle::Inactive,
                    None,
                    RetainedStandaloneSessionState {
                        name: "fixture".to_string(),
                        shell_open: false,
                        archived: false,
                        archived_by_user: false,
                        resume_id: None,
                        ever_launched: false,
                    },
                ));
            self
        }

        fn build(self) -> RepositoryState {
            self.state
        }
    }

    /// Enumeration and filesystem classification, always against a synthetic
    /// tree passed through [`ScanRoots`]. No case here calls [`scan`] or
    /// resolves a developer root.
    mod enumeration {
        use super::*;

        #[test]
        fn returns_one_candidate_per_shaped_directory() {
            let fixture = ScanFixture::new();
            fixture.dir("claude", "repository-1");
            fixture.dir("claude", "repository-42");
            fixture.dir("opencode", "repository-7");
            // Shape mismatches. Each is skipped, never reported as an error.
            fixture.dir("claude", "repository-notanumber");
            fixture.dir("claude", "repository-18446744073709551616"); // u64::MAX + 1
            fixture.dir("claude", "repository-007"); // does not round-trip
            fixture.dir("claude", "notrepository-3");
            // A third-level directory that matches the shape: the walk is
            // exactly three levels deep and must not descend into it.
            fixture.dir("claude", "repository-42/repository-99");
            std::fs::write(
                fixture.base().join("claude").join("repository-9"),
                b"a file, not a directory",
            )
            .expect("write shaped file");
            std::fs::write(fixture.base().join("loose.json"), b"{}").expect("write loose file");

            let report = scan_ok(&fixture);

            assert_eq!(
                reported(&report),
                vec![
                    "claude/repository-1",
                    "claude/repository-42",
                    "opencode/repository-7",
                ]
            );
            assert_eq!(
                candidate(&report, "claude", "repository-42").repository_key,
                42
            );
            assert_eq!(
                candidate(&report, "opencode", "repository-7").repository_key,
                7
            );
        }

        #[test]
        fn an_empty_candidate_is_recorded_as_empty() {
            let fixture = ScanFixture::new();
            fixture.dir("claude", "repository-1");
            fixture.dir("claude", "repository-2/primary-2"); // empty at every level

            let report = scan_ok(&fixture);

            for name in ["repository-1", "repository-2"] {
                let found = candidate(&report, "claude", name);
                assert!(
                    evidence(found).contains(&Evidence::Empty),
                    "{name} is empty at every level: {found:?}"
                );
                assert!(
                    evidence(found).contains(&Evidence::NoGitdir),
                    "{name} has no gitdir: {found:?}"
                );
            }
        }

        #[test]
        fn a_candidate_with_entries_contains_a_checkout() {
            let fixture = ScanFixture::new();
            let path = fixture.dir("claude", "repository-2/primary-2");
            std::fs::write(path.join("tracked.txt"), b"real work\n").expect("write checkout file");
            fixture.dir("claude", "repository-2/feature-3");

            let report = scan_ok(&fixture);
            let found = candidate(&report, "claude", "repository-2");

            assert!(
                evidence(found).contains(&Evidence::ContainsCheckout { entries: 2 }),
                "two entries, one of them populated: {found:?}"
            );
            assert!(
                matches!(found.verdict, Verdict::Live { .. }),
                "a populated candidate is live: {found:?}"
            );
        }

        #[cfg(unix)]
        #[test]
        fn a_symlink_candidate_is_recorded_and_never_removable() {
            let fixture = ScanFixture::new();
            let outside = fixture.root.join("outside");
            std::fs::create_dir_all(&outside).expect("create link target");
            fixture.workspace("claude");
            std::os::unix::fs::symlink(
                &outside,
                fixture.base().join("claude").join("repository-3"),
            )
            .expect("create symlinked candidate");

            let report = scan_ok(&fixture);
            let found = candidate(&report, "claude", "repository-3");

            assert!(
                evidence(found).contains(&Evidence::IsSymlink),
                "the link itself is recorded: {found:?}"
            );
            assert!(
                matches!(found.verdict, Verdict::Indeterminate { .. }),
                "a symlink is refused outright: {found:?}"
            );
            // Classified on the link, never on its target: the empty target
            // must not supply a clearing signal.
            assert!(
                !evidence(found).contains(&Evidence::Empty),
                "the target's emptiness must not leak into the verdict: {found:?}"
            );
        }

        #[cfg(unix)]
        #[test]
        fn a_symlinked_workspace_is_not_descended() {
            let fixture = ScanFixture::new();
            let outside = fixture.root.join("outside");
            std::fs::create_dir_all(outside.join("repository-4")).expect("create link target");
            std::os::unix::fs::symlink(&outside, fixture.base().join("elsewhere"))
                .expect("create symlinked workspace");

            let report = scan_ok(&fixture);

            assert!(
                report.candidates.is_empty(),
                "a symlinked workspace is not descended: {report:?}"
            );
        }

        #[test]
        fn git_disownment_is_recorded_only_when_git_actually_spoke() {
            let fixture = ScanFixture::new();
            let repo = fixture.git_repo("repo");
            let disowned = fixture.dir("claude", "repository-8");
            std::fs::write(
                disowned.join(".git"),
                format!("gitdir: {}/.git\n", repo.display()),
            )
            .expect("write orphan gitdir file");

            let report = scan_ok(&fixture);
            let found = candidate(&report, "claude", "repository-8");

            assert!(
                evidence(found).contains(&Evidence::GitDisownsIt {
                    owning_repository: repo
                }),
                "git's inventory does not list this path: {found:?}"
            );
            assert!(
                !evidence(found).contains(&Evidence::NoGitdir),
                "a gitdir is present: {found:?}"
            );
        }

        #[test]
        fn a_failed_git_lookup_is_not_a_disownment() {
            let fixture = ScanFixture::new();
            let path = fixture.dir("claude", "repository-5");
            std::fs::write(
                path.join(".git"),
                b"gitdir: /nonexistent/baude-08-04/.git\n",
            )
            .expect("write broken gitdir file");

            let report = scan_ok(&fixture);
            let found = candidate(&report, "claude", "repository-5");

            assert!(
                !evidence(found)
                    .iter()
                    .any(|signal| matches!(signal, Evidence::GitDisownsIt { .. })),
                "a failed lookup is not a disownment: {found:?}"
            );
            assert!(
                !matches!(found.verdict, Verdict::Removable { .. }),
                "nothing clears on a failed lookup: {found:?}"
            );
        }

        /// Successor to plan 08-04's `no_scanned_candidate_is_removable_without_
        /// state_evidence`, which held only because the scanner emitted no state
        /// signal at all. Plan 08-05 supplies one, so the guard is restated at
        /// the level that still matters: the state check is never *skipped*.
        /// Every candidate carries a reference, a checked non-reference, or the
        /// uncertainty that prevented both — and the second conjunct of the
        /// predicate can never be satisfied by silence.
        #[test]
        fn every_scanned_candidate_carries_a_state_signal() {
            let fixture = ScanFixture::new();
            fixture.dir("claude", "repository-1");
            fixture.dir("claude", "repository-2/primary-2");
            fixture.dir("opencode", "repository-3");

            let report = scan_ok(&fixture);

            assert_eq!(report.candidates.len(), 3);
            for found in &report.candidates {
                assert!(
                    evidence(found).iter().any(|signal| matches!(
                        signal,
                        Evidence::ReferencedByState { .. }
                            | Evidence::NotReferencedByState { .. }
                            | Evidence::StateUnreadable { .. }
                    )),
                    "the state cross-reference is never skipped: {found:?}"
                );
            }
        }

        /// D-16: the scan's only output is a report. Asserted by comparing the
        /// entries and modification times of both synthetic roots before and
        /// after — a created lock, temp file or probe directory fails this.
        #[test]
        fn a_scan_leaves_both_roots_unchanged() {
            let fixture = ScanFixture::new();
            fixture.dir("claude", "repository-1");
            let populated = fixture.dir("claude", "repository-2/primary-2");
            std::fs::write(populated.join("tracked.txt"), b"real work\n").expect("write file");
            let broken = fixture.dir("opencode", "repository-3");
            std::fs::write(
                broken.join(".git"),
                b"gitdir: /nonexistent/baude-08-04/.git\n",
            )
            .expect("write broken gitdir file");
            #[cfg(unix)]
            std::os::unix::fs::symlink(
                fixture.root.join("outside"),
                fixture.base().join("claude").join("repository-4"),
            )
            .expect("create symlinked candidate");

            let base_before = tree_snapshot(&fixture.base());
            let config_before = tree_snapshot(&fixture.config());

            let report = scan_ok(&fixture);
            assert!(!report.candidates.is_empty(), "the fixture has candidates");

            assert_eq!(
                tree_snapshot(&fixture.base()),
                base_before,
                "the worktrees root must be byte-for-byte unchanged"
            );
            assert_eq!(
                tree_snapshot(&fixture.config()),
                config_before,
                "the config root must be untouched — no state lock, no temp file"
            );
        }

        #[test]
        fn a_missing_base_yields_an_empty_report() {
            let fixture = ScanFixture::new();
            let roots = ScanRoots {
                worktrees_base: fixture.root.join("never-created"),
                config_dir: fixture.config(),
            };

            let report = scan_at(&roots).expect("a missing root is not an error");

            assert!(report.candidates.is_empty());
        }
    }

    /// The persisted-state cross-reference: the only *negative* ownership
    /// evidence the scan has, and the only signal that can clear a candidate.
    ///
    /// Every case here writes its state files into a fixture config directory.
    /// Nothing in this module resolves [`crate::persist::config_dir`], and
    /// nothing writes through [`crate::persist`] — a `save` would take the state
    /// lock and drop a `.state-<ws>.json.lock` into the directory the read-only
    /// cases assert is untouched.
    mod state {
        use super::*;
        use crate::workspace::DEFAULT;

        /// Only the signals that speak about persisted state.
        fn state_signals(found: &Candidate) -> Vec<Evidence> {
            evidence(found)
                .iter()
                .filter(|signal| {
                    matches!(
                        signal,
                        Evidence::ReferencedByState { .. }
                            | Evidence::NotReferencedByState { .. }
                            | Evidence::StateUnreadable { .. }
                    )
                })
                .cloned()
                .collect()
        }

        fn references(found: &Candidate) -> Vec<(String, Option<u64>, ReferenceMatch)> {
            evidence(found)
                .iter()
                .filter_map(|signal| match signal {
                    Evidence::ReferencedByState {
                        workspace,
                        repository_key,
                        matched,
                    } => Some((workspace.clone(), *repository_key, *matched)),
                    _ => None,
                })
                .collect()
        }

        fn is_removable(found: &Candidate) -> bool {
            matches!(found.verdict, Verdict::Removable { .. })
        }

        fn is_live(found: &Candidate) -> bool {
            matches!(found.verdict, Verdict::Live { .. })
        }

        /// A repository recorded in `workspace` under `key`, with its real
        /// checkout outside the managed base — the shape the developer's two
        /// surviving `claude` repository records actually have.
        fn external_repository(fixture: &ScanFixture, key: u64, name: &str) -> RepositoryState {
            let main = fixture.external(name);
            StateBuilder::new().repository(key, &main).build()
        }

        // ---- repository-key ownership ------------------------------------

        #[test]
        fn a_recorded_repository_key_proves_its_candidate_live() {
            let fixture = ScanFixture::new();
            fixture.dir("claude", "repository-7");
            fixture.write_state(
                "state-claude.json",
                external_repository(&fixture, 7, "repo-a"),
            );

            let report = scan_ok(&fixture);
            let found = candidate(&report, "claude", "repository-7");

            assert!(
                is_live(found),
                "a recorded repository key is positive proof of liveness: {found:?}"
            );
            assert!(
                references(found).contains(&("claude".to_string(), Some(7), ReferenceMatch::Key)),
                "the key match must be named in the evidence: {found:?}"
            );
        }

        /// Keys are allocated per workspace, so repository 7 in `opencode` says
        /// nothing at all about repository 7 in `claude`. Reading them as one
        /// namespace would be a false *protection*, which is the safe direction —
        /// but it would also mask a genuine leak forever.
        #[test]
        fn a_repository_key_is_scoped_to_its_own_workspace() {
            let fixture = ScanFixture::new();
            fixture.dir("claude", "repository-7");
            fixture.workspace("opencode");
            fixture.write_state(
                "state-opencode.json",
                external_repository(&fixture, 7, "repo-a"),
            );

            let report = scan_ok(&fixture);
            let found = candidate(&report, "claude", "repository-7");

            assert!(
                references(found).is_empty(),
                "another workspace's key 7 is not a claim on claude/repository-7: {found:?}"
            );
            assert!(
                is_removable(found),
                "an empty, unreferenced, shaped directory clears: {found:?}"
            );
        }

        /// `repository-1` and `repository-10` differ by a suffix, and a prefix
        /// comparison anywhere in the key path would silently protect 1433
        /// directories behind one live record.
        #[test]
        fn a_recorded_key_does_not_protect_a_key_that_merely_starts_with_it() {
            let fixture = ScanFixture::new();
            fixture.dir("claude", "repository-1");
            fixture.dir("claude", "repository-10");
            fixture.write_state(
                "state-claude.json",
                external_repository(&fixture, 1, "repo-a"),
            );

            let report = scan_ok(&fixture);

            assert!(is_live(candidate(&report, "claude", "repository-1")));
            assert!(
                is_removable(candidate(&report, "claude", "repository-10")),
                "key 1 is not a claim on key 10: {:?}",
                candidate(&report, "claude", "repository-10")
            );
        }

        // ---- path ownership ----------------------------------------------

        /// A standalone session carries no repository key, so this case isolates
        /// the path rule from the key rule entirely.
        #[test]
        fn an_exactly_recorded_path_proves_its_candidate_live() {
            let fixture = ScanFixture::new();
            let candidate_path = fixture.dir("claude", "repository-9");
            fixture.write_state(
                "state-claude.json",
                StateBuilder::new().standalone(&candidate_path).build(),
            );

            let report = scan_ok(&fixture);
            let found = candidate(&report, "claude", "repository-9");

            assert!(is_live(found), "{found:?}");
            assert!(
                references(found).contains(&(
                    "claude".to_string(),
                    None,
                    ReferenceMatch::ExactPath
                )),
                "an exact path match carries no repository key: {found:?}"
            );
        }

        /// The managed shape puts the checkout one level *inside* the candidate,
        /// so the recorded path is a descendant of the directory being judged.
        /// This is the single most important path case: the candidate itself is
        /// dirs-only and therefore reads as `Empty`.
        #[test]
        fn a_checkout_recorded_beneath_a_candidate_proves_it_live() {
            let fixture = ScanFixture::new();
            let checkout = fixture.dir("claude", "repository-9/primary-9");
            let main = fixture.external("repo-a");
            fixture.write_state(
                "state-claude.json",
                StateBuilder::new()
                    .repository(1, &main)
                    .checkout(1, &checkout, true)
                    .build(),
            );

            let report = scan_ok(&fixture);
            let found = candidate(&report, "claude", "repository-9");

            assert!(
                is_live(found),
                "an empty-looking parent holding a recorded checkout is live: {found:?}"
            );
            assert!(
                references(found).contains(&(
                    "claude".to_string(),
                    Some(1),
                    ReferenceMatch::Descendant
                )),
                "{found:?}"
            );
        }

        /// A recorded path that *contains* the candidate. Defensive rather than
        /// load-bearing in production, and cheap: whichever direction the overlap
        /// runs, a deletion would destroy recorded state.
        #[test]
        fn a_path_recorded_above_a_candidate_proves_it_live() {
            let fixture = ScanFixture::new();
            fixture.dir("claude", "repository-9");
            let workspace_dir = fixture.workspace("claude");
            fixture.write_state(
                "state-claude.json",
                StateBuilder::new().standalone(&workspace_dir).build(),
            );

            let report = scan_ok(&fixture);
            let found = candidate(&report, "claude", "repository-9");

            assert!(is_live(found), "{found:?}");
            assert!(
                references(found).contains(&("claude".to_string(), None, ReferenceMatch::Ancestor)),
                "{found:?}"
            );
        }

        /// Path claims are matched across every workspace's state. A path is a
        /// path: the deletion does not care which file recorded it, and one
        /// workspace's state can legitimately name a directory under another's.
        #[test]
        fn a_path_recorded_in_another_workspaces_state_still_protects() {
            let fixture = ScanFixture::new();
            let candidate_path = fixture.dir("claude", "repository-9");
            fixture.workspace("opencode");
            fixture.write_state(
                "state-opencode.json",
                StateBuilder::new().standalone(&candidate_path).build(),
            );

            let report = scan_ok(&fixture);
            let found = candidate(&report, "claude", "repository-9");

            assert!(
                is_live(found),
                "path ownership crosses workspaces even though keys do not: {found:?}"
            );
            assert!(
                references(found).contains(&(
                    "opencode".to_string(),
                    None,
                    ReferenceMatch::ExactPath
                )),
                "{found:?}"
            );
        }

        /// The scan canonicalizes its base; persisted paths were recorded
        /// through whatever form the process saw at the time. On macOS the
        /// fixture root itself reaches the tree through `/var -> /private/var`,
        /// so comparing the recorded bytes alone would miss every match.
        #[test]
        fn a_recorded_path_reaching_the_tree_through_a_symlink_still_matches() {
            let fixture = ScanFixture::new();
            fixture.dir("claude", "repository-9");
            // Deliberately NOT canonicalized: the uncanonical form is what a
            // process that never resolved its own root would have persisted.
            let uncanonical = fixture.base().join("claude").join("repository-9");
            fixture.write_state(
                "state-claude.json",
                StateBuilder::new().standalone(&uncanonical).build(),
            );

            let report = scan_ok(&fixture);
            let found = candidate(&report, "claude", "repository-9");

            assert!(
                is_live(found),
                "the recorded path must be resolved before comparison: {found:?}"
            );
        }

        /// A path recorded for a directory that no longer exists still has to be
        /// placed: its leaf cannot be canonicalized, but its surviving ancestor
        /// can, and the candidate it names may still be on disk beneath it.
        #[test]
        fn a_recorded_path_whose_leaf_is_gone_is_still_placed() {
            let fixture = ScanFixture::new();
            fixture.dir("claude", "repository-9");
            let missing = fixture
                .base()
                .join("claude")
                .join("repository-9")
                .join("primary-9");
            fixture.write_state(
                "state-claude.json",
                StateBuilder::new().standalone(&missing).build(),
            );

            let report = scan_ok(&fixture);
            let found = candidate(&report, "claude", "repository-9");

            assert!(
                is_live(found),
                "a vanished checkout still proves its parent was in use: {found:?}"
            );
            assert!(
                references(found).contains(&(
                    "claude".to_string(),
                    None,
                    ReferenceMatch::Descendant
                )),
                "{found:?}"
            );
        }

        // ---- inventory completeness --------------------------------------

        /// T-08-16, stated directly. A file that could not be read is not a file
        /// that said "no", and the uncertainty is global: state in one workspace
        /// can name a path in another, so a failed read in `opencode` must
        /// withhold clearance from `claude` too.
        #[test]
        fn one_unreadable_state_file_withholds_clearance_from_every_candidate() {
            let fixture = ScanFixture::new();
            fixture.dir("claude", "repository-9");
            fixture.workspace("opencode");
            fixture.dir("opencode", "repository-3");
            fixture.write_raw("state-opencode.json", b"{ this is not json");

            let report = scan_ok(&fixture);

            assert!(!report.state_inventory.complete);
            for found in &report.candidates {
                assert!(
                    !is_removable(found),
                    "an incomplete inventory clears nothing: {found:?}"
                );
                assert!(
                    state_signals(found)
                        .iter()
                        .any(|signal| matches!(signal, Evidence::StateUnreadable { .. })),
                    "the uncertainty must be attached to every candidate: {found:?}"
                );
            }
        }

        /// Every derived filename is read on its own. A readable
        /// `state-claude.json` must not be allowed to stand in for an unreadable
        /// `daemon-state-claude.json`: falling back is how a daemon-recorded
        /// checkout becomes invisible.
        #[test]
        fn a_readable_state_file_does_not_excuse_an_unreadable_sibling() {
            let fixture = ScanFixture::new();
            fixture.dir("claude", "repository-9");
            fixture.write_state("state-claude.json", RepositoryState::default());
            fixture.write_raw("daemon-state-claude.json", b"\x00\x01 not json at all");

            let report = scan_ok(&fixture);
            let found = candidate(&report, "claude", "repository-9");

            assert!(!report.state_inventory.complete);
            assert!(
                !is_removable(found),
                "the sibling read failed, so nothing was proven: {found:?}"
            );
        }

        /// An interrupted atomic write leaves `.state-<ws>.json.tmp-<pid>-<n>`
        /// behind — six of them sit in the developer's real config directory
        /// today. Those bytes are state and can name candidates, so an entry
        /// that is state-like but not attributable to a derived filename makes
        /// the inventory incomplete rather than being quietly ignored.
        #[test]
        fn an_orphaned_temp_state_file_makes_the_inventory_incomplete() {
            let fixture = ScanFixture::new();
            fixture.dir("claude", "repository-9");
            fixture.write_raw(".state-claude.json.tmp-9999-1", b"{}");

            let report = scan_ok(&fixture);

            assert!(
                !report.state_inventory.complete,
                "an unattributable state-like entry is uncertainty: {report:?}"
            );
            assert!(!is_removable(candidate(&report, "claude", "repository-9")));
        }

        /// A lock file is an advisory marker with no state in it, so it must not
        /// poison the inventory. Production holds one whenever the TUI is
        /// running; treating it as uncertainty would make the tool useless
        /// exactly when a developer reaches for it.
        #[test]
        fn a_state_lock_file_does_not_make_the_inventory_incomplete() {
            let fixture = ScanFixture::new();
            fixture.dir("claude", "repository-9");
            fixture.write_state("state-claude.json", RepositoryState::default());
            fixture.write_raw(".state-claude.json.lock", b"");

            let report = scan_ok(&fixture);

            assert!(report.state_inventory.complete, "{report:?}");
            assert!(is_removable(candidate(&report, "claude", "repository-9")));
        }

        #[test]
        fn an_unrelated_config_file_does_not_make_the_inventory_incomplete() {
            let fixture = ScanFixture::new();
            fixture.dir("claude", "repository-9");
            fixture.write_state("state-claude.json", RepositoryState::default());
            fixture.write_raw("config.toml", b"theme = \"dark\"\n");
            std::fs::create_dir(fixture.config().join("logs")).expect("create logs dir");

            let report = scan_ok(&fixture);

            assert!(report.state_inventory.complete, "{report:?}");
            assert!(is_removable(candidate(&report, "claude", "repository-9")));
        }

        /// A config directory that does not exist is not a checked absence of
        /// every state file — it means the scan is pointed somewhere other than
        /// where state lives. Fail closed.
        #[test]
        fn a_missing_config_directory_withholds_clearance() {
            let fixture = ScanFixture::new();
            fixture.dir("claude", "repository-9");
            let roots = ScanRoots {
                worktrees_base: fixture.base(),
                config_dir: fixture.root.join("never-created"),
            };

            let report = scan_at(&roots).expect("a missing config dir is evidence, not an error");

            assert!(!report.state_inventory.complete);
            assert!(!is_removable(candidate(&report, "claude", "repository-9")));
        }

        // ---- which files get consulted -----------------------------------

        /// The union's first half: a workspace with persisted state but no
        /// directory under the worktrees base.
        #[test]
        fn a_workspace_known_only_from_the_config_directory_is_checked() {
            let fixture = ScanFixture::new();
            fixture.dir("claude", "repository-9");
            fixture.write_state("state-claude.json", RepositoryState::default());
            fixture.write_state("state-opencode.json", RepositoryState::default());

            let report = scan_ok(&fixture);

            assert!(
                report
                    .state_inventory
                    .workspaces_checked
                    .contains(&"opencode".to_string()),
                "{:?}",
                report.state_inventory
            );
            assert!(
                report
                    .state_inventory
                    .files_checked
                    .contains(&"daemon-state-opencode.json".to_string()),
                "both state bases are derived for a discovered workspace: {:?}",
                report.state_inventory
            );
        }

        /// The union's second half: a workspace with a directory under the base
        /// but no state file yet. Deriving names from the config directory alone
        /// would turn "never written" into "never checked".
        #[test]
        fn a_workspace_known_only_from_the_worktrees_base_is_checked() {
            let fixture = ScanFixture::new();
            fixture.dir("codex", "repository-9");
            fixture.write_state("state-claude.json", RepositoryState::default());

            let report = scan_ok(&fixture);

            assert!(
                report
                    .state_inventory
                    .workspaces_checked
                    .contains(&"codex".to_string()),
                "{:?}",
                report.state_inventory
            );
            assert!(
                report
                    .state_inventory
                    .files_absent
                    .contains(&"state-codex.json".to_string()),
                "its absent file is a CHECKED absence: {:?}",
                report.state_inventory
            );
        }

        #[test]
        fn the_default_workspace_is_always_checked() {
            let fixture = ScanFixture::new();
            fixture.dir("codex", "repository-9");

            let report = scan_ok(&fixture);

            assert!(
                report
                    .state_inventory
                    .workspaces_checked
                    .contains(&DEFAULT.to_string()),
                "{:?}",
                report.state_inventory
            );
        }

        /// The default workspace kept unsuffixed filenames for backward
        /// compatibility, and `Workspace::legacy_state_file` is the only place
        /// that knows it. A reference living there must protect exactly as well.
        #[test]
        fn the_legacy_unsuffixed_state_file_is_read_for_the_default_workspace() {
            let fixture = ScanFixture::new();
            fixture.dir(DEFAULT, "repository-7");
            fixture.write_state("state.json", external_repository(&fixture, 7, "repo-a"));

            let report = scan_ok(&fixture);
            let found = candidate(&report, DEFAULT, "repository-7");

            assert!(
                report
                    .state_inventory
                    .files_checked
                    .contains(&"state.json".to_string()),
                "{:?}",
                report.state_inventory
            );
            assert!(
                is_live(found),
                "a legacy-file reference protects exactly as well: {found:?}"
            );
        }

        /// ...and only for the default workspace. Deriving a bare `state.json`
        /// for `opencode` would attribute one workspace's records to another.
        #[test]
        fn no_legacy_file_is_derived_for_a_non_default_workspace() {
            let fixture = ScanFixture::new();
            fixture.dir("opencode", "repository-9");
            fixture.write_state("state-opencode.json", RepositoryState::default());
            fixture.write_state("state-claude.json", RepositoryState::default());

            let report = scan_ok(&fixture);
            let expected = vec![
                "daemon-state-claude.json".to_string(),
                "daemon-state-opencode.json".to_string(),
                "daemon-state.json".to_string(),
                "state-claude.json".to_string(),
                "state-opencode.json".to_string(),
                "state.json".to_string(),
            ];

            assert_eq!(
                report.state_inventory.files_checked, expected,
                "exactly two bases per workspace, plus the default's two legacy names"
            );
        }

        // ---- what the negative signal carries ----------------------------

        #[test]
        fn not_referenced_names_the_inventory_it_rests_on() {
            let fixture = ScanFixture::new();
            fixture.dir("claude", "repository-9");
            fixture.write_state("state-claude.json", RepositoryState::default());

            let report = scan_ok(&fixture);
            let found = candidate(&report, "claude", "repository-9");
            let signals = state_signals(found);
            let [Evidence::NotReferencedByState {
                workspaces_checked,
                files_checked,
                files_absent,
            }] = signals.as_slice()
            else {
                panic!("expected exactly one negative state signal: {signals:?}");
            };

            assert_eq!(workspaces_checked, &vec![DEFAULT.to_string()]);
            assert_eq!(
                files_checked,
                &vec![
                    "daemon-state-claude.json".to_string(),
                    "daemon-state.json".to_string(),
                    "state-claude.json".to_string(),
                    "state.json".to_string(),
                ],
                "sorted, so two reports of an unchanged tree compare equal"
            );
            assert_eq!(
                files_absent,
                &vec![
                    "daemon-state-claude.json".to_string(),
                    "daemon-state.json".to_string(),
                    "state.json".to_string(),
                ],
                "the absences are enumerated, never left implicit"
            );
        }

        /// The inventory is read once for the whole scan, so every candidate
        /// rests on the identical fact. Two candidates disagreeing about what
        /// was checked would mean the cross-reference ran per candidate and
        /// could observe the directory mid-change.
        #[test]
        fn every_candidate_rests_on_one_shared_inventory() {
            let fixture = ScanFixture::new();
            fixture.dir("claude", "repository-9");
            fixture.dir("claude", "repository-10");
            fixture.dir("opencode", "repository-2");
            fixture.write_state("state-claude.json", RepositoryState::default());

            let report = scan_ok(&fixture);
            let signals: Vec<_> = report.candidates.iter().map(state_signals).collect();

            assert_eq!(signals.len(), 3);
            assert!(
                signals.windows(2).all(|pair| pair[0] == pair[1]),
                "one inventory, one answer: {signals:?}"
            );
        }

        /// The read-only contract (D-16) extended to the config directory now
        /// that it is actually opened. No lock, no temp file, no mtime change.
        #[test]
        fn the_state_cross_reference_writes_nothing() {
            let fixture = ScanFixture::new();
            fixture.dir("claude", "repository-9");
            fixture.dir("opencode", "repository-2");
            fixture.write_state(
                "state-claude.json",
                external_repository(&fixture, 9, "repo-a"),
            );
            fixture.write_state("state-opencode.json", RepositoryState::default());
            fixture.write_raw(".state-claude.json.lock", b"");

            let before = tree_snapshot(&fixture.config());
            let report = scan_ok(&fixture);
            assert!(!report.candidates.is_empty());

            assert_eq!(
                tree_snapshot(&fixture.config()),
                before,
                "reading state must not take the lock or leave a temp file"
            );
        }
    }
}
