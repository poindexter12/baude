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
use std::path::PathBuf;

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
    /// of them references this candidate. Carries the workspaces actually
    /// checked so "not referenced" can never be confused with "not checked".
    NotReferencedByState { workspaces_checked: Vec<String> },
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
    /// A state file that could reference candidates could not be read. Hard
    /// blocker **across the whole scan**, not merely for its own workspace:
    /// state can reference paths in another workspace, so inferring absence
    /// from a failed read could clear a live repository parent.
    StateUnreadable { source: PathBuf, workspace: String },
}

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
        Evidence::NotReferencedByState { workspaces_checked } => Some(workspaces_checked.clone()),
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
    todo!(
        "plan 08-04 task 3 enumerates {} read-only",
        roots.worktrees_base.display()
    )
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
                },
            ]);
            assert!(
                matches!(verdict, Verdict::Indeterminate { .. }),
                "an unreadable inventory blocks clearing: {verdict:?}"
            );
        }
    }

    /// Enumeration and filesystem classification, always against a synthetic
    /// tree passed through [`ScanRoots`]. No case here calls [`scan`] or
    /// resolves a developer root.
    mod enumeration {
        use super::*;
        use std::path::Path;
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

        /// Until plan 08-05 supplies the state cross-reference there is no
        /// source of `NotReferencedByState`, so no scanned candidate can be
        /// cleared — by construction, not by luck.
        #[test]
        fn no_scanned_candidate_is_removable_without_state_evidence() {
            let fixture = ScanFixture::new();
            fixture.dir("claude", "repository-1");
            fixture.dir("claude", "repository-2/primary-2");
            fixture.dir("opencode", "repository-3");

            let report = scan_ok(&fixture);

            assert_eq!(report.candidates.len(), 3);
            for found in &report.candidates {
                assert!(
                    !matches!(found.verdict, Verdict::Removable { .. }),
                    "scan cannot clear without state evidence: {found:?}"
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
}
