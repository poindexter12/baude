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
}
