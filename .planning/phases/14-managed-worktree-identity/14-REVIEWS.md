---
phase: 14
reviewers: [codex]
reviewed_at: 2026-09-20T17:44:51Z
plans_reviewed: [14-01-PLAN.md,14-02-PLAN.md,14-03-PLAN.md]
models:
  codex: "unknown"
model_sources:
  codex: "unknown"
---

# Cross-AI Plan Review — Phase 14

<!-- gsd:plan-revision-conflicts:begin -->
## Plan-Revision Conflicts
<!-- gsd:plan-revision-conflicts:end -->

# Cross-AI Plan Review

Overall verdict: **revision required before execution**. The digest/marker direction is sound, but the plans do not yet define the central mapping from durable repository identity to the physical managed-directory key. As written, legacy in-place reuse, state-reset recovery, collision fallback, and safe scan/prune behavior are not implementable end to end.

## 14-01

### Summary

The plan identifies the correct stable identity—Git’s canonical common directory—and puts the primitive in `baude-core`, where both frontends can share it. However, the proposed marker representation, dependency list, collision semantics, and tests are incomplete. Most importantly, a fixed 48-bit digest cannot by itself satisfy “never collide,” and the plan does not define the fallback key produced after detecting such a collision.

### Strengths

- The identity source agrees with existing Git discovery. `discover_repository` canonicalizes both the input and `--git-common-dir`, then stores the canonical common directory in `RepositorySnapshot` (`baude-core/src/git.rs:321-353`, `baude-core/src/git.rs:388-394`).
- Putting digest calculation and markers in `baude-core` is the correct ownership boundary. Both TUI and daemon already consume shared lifecycle/path code; for example, both call `lifecycle::prepare_activation` (`baude/src/app.rs:2278-2283`, `bauded/src/manager.rs:869-875`).
- Atomic marker writes are appropriate. The codebase already has a strong implementation pattern using `create_new`, flush, `sync_all`, atomic rename, and directory sync (`baude-core/src/persist.rs:631-679`).
- Marker-based recovery is a useful complement to state because current state identity is only an incrementing `RepositoryKey(u64)` (`baude-core/src/repository.rs:11-18`, `baude-core/src/repository.rs:653-663`).

### Concerns

- **HIGH — Required hash dependency is absent from the plan.** `sha2` exists only in `bauded`, not `baude-core`; the proposed implementation cannot compile without modifying dependency manifests (`bauded/Cargo.toml:19-27`, `baude-core/Cargo.toml:18-28`).
- **HIGH — `canonical_common_dir: String` loses valid Unix repository identities.** The project deliberately persists paths as raw bytes because Unix paths need not be UTF-8 (`baude-core/src/repository.rs:38-55`). A `String` marker cannot round-trip every identity accepted by `discover_repository`.
- **HIGH — Collision recovery is undefined.** Twelve hexadecimal characters provide only 48 bits. Detection can prevent unsafe adoption, but the stated “newcomer is allocated a distinct digest-keyed directory” needs an explicit deterministic fallback such as a longer digest. Current physical paths accept exactly one repository-key component (`baude-core/src/git.rs:1779-1784`, `baude-core/src/git.rs:1815-1818`); no fallback representation exists.
- **HIGH — Atomic replacement does not provide an atomic ownership claim.** TUI and daemon intentionally use different state locks, so they may operate concurrently (`baude/src/main.rs:351-357`, `bauded/src/main.rs:174-191`). A read-compare-write marker sequence can race, and rename-based replacement could overwrite the competing owner. First ownership must use `create_new` or a shared per-repository lock.
- **MEDIUM — The threat mitigations are inaccurate.** Canonicalizing the parent directory does not prevent the marker itself from being a symlink. Likewise, `serde_json` parsing does not bound input size. The current project’s safe file-write pattern explicitly owns a unique temporary file and synchronizes the result (`baude-core/src/persist.rs:637-679`).
- **MEDIUM — The tests are not an end-to-end tracer for the requirements.** “Two repositories with the same common dir” actually describes two worktrees of one repository, because common-dir equality is already the repository identity used for deduplication (`baude-core/src/lifecycle.rs:1311-1319`). The proposed tests omit distinct repositories, a forced truncated-digest collision, non-UTF-8 paths, concurrent claims, scheme-version handling, and interrupted writes.

### Suggestions

- Add `sha2` to the workspace/`baude-core` dependencies and include the affected `Cargo.toml` files in `files_modified`.
- Store the common directory losslessly using `PersistedPath` or encoded raw bytes, not `String`.
- Introduce a typed physical identity such as `ManagedRepositoryDirKey`, separate from the numeric in-memory `RepositoryKey`.
- Specify collision fallback now: for example, 12 hex normally, then progressively extend the same SHA-256 digest under a race-safe ownership claim.
- Define the marker filename, strict schema/version behavior, maximum accepted size, symlink policy, permissions, idempotency, and exact atomic-claim algorithm.
- Replace the three unit tests with a tracer that resolves a physical key, claims a marker, restarts from empty state, and resolves the same directory again.

### Risk Assessment

**HIGH.** The primitive is directionally correct, but its current data representation excludes valid repositories and its collision/claim protocol cannot uphold WTID-01 under either digest collision or concurrent TUI/daemon activity.

## 14-02

### Summary

This plan reaches the correct integration areas, but it skips the most important abstraction: resolving a canonical repository identity to either its existing legacy directory or its new digest directory. Merely changing path functions from `u64` to `&str` does not provide that mapping. The proposed scanner change also affects a safety-critical prune protocol much more broadly than the task describes.

### Strengths

- It correctly targets the current collision source. Managed paths use workspace-scoped numeric repository keys (`baude-core/src/git.rs:1776-1783`), while each state file independently starts allocation at one (`baude-core/src/repository.rs:490-500`).
- Preserving the existing child naming scheme limits migration scope; only the repository-directory component needs a new physical identity (`baude-core/src/git.rs:1779-1784`, `baude-core/src/git.rs:1799-1818`).
- Supporting strict round-trip parsing for both decimal and digest names follows the scanner’s existing anti-aliasing rule for names such as `repository-007` (`baude-core/src/worktree_scan.rs:1007-1015`).
- Centralizing branch path selection in lifecycle code is valuable because TUI and daemon already converge at `prepare_activation` (`baude/src/app.rs:2278-2283`, `bauded/src/manager.rs:869-875`).

### Concerns

- **HIGH — There is no legacy/digest directory resolver.** Durable repositories contain a numeric runtime key and canonical paths, but no physical managed-directory key (`baude-core/src/repository.rs:306-314`). `ensure_repository` returns only that numeric key (`baude-core/src/lifecycle.rs:1307-1340`). Changing its return to `(RepositoryKey, CollisionReport)` still gives callers no way to choose an existing `repository-7` rather than `repository-<digest>`.
- **HIGH — State-reset recovery is therefore unimplemented.** After reset, `RepositoryState` starts again at key one (`baude-core/src/repository.rs:490-500`). Marker adoption may label a legacy directory, but no planned API searches markers by common-dir and returns the matching physical key to default or branch path composition.
- **HIGH — Collision continuation cannot work with the proposed return type.** `prepare_activation` currently derives the path directly from `repository.get()` (`baude-core/src/lifecycle.rs:1349-1360`). A collision report does not contain or install a replacement path key, so “admission continues in a distinct directory” has no mechanism.
- **HIGH — The scanner change is much larger than described because scan reports authorize pruning.** Repository keys occur in state references, evidence, candidates, prune outcomes, record validation, sorting, and tests (`baude-core/src/worktree_scan.rs:542-600`, `baude-core/src/worktree_scan.rs:647-699`, `baude-core/src/worktree_scan.rs:1244-1251`, `baude-core/src/worktree_scan.rs:1401-1439`). Converting only the parser and `Candidate` will either fail to compile or lose state-key protection for legacy directories.
- **HIGH — The report format must be versioned.** `ScanReport` explicitly requires a version bump whenever field meanings change, and prune refuses mismatched versions (`baude-core/src/worktree_scan.rs:773-798`, `baude-core/src/worktree_scan.rs:1283-1290`). The plan changes `repository_key` from a number to a string without mentioning `REPORT_FORMAT_VERSION`.
- **HIGH — A marker file changes deletion classification.** The scanner treats any non-directory anywhere below an otherwise empty candidate as checkout content (`baude-core/src/worktree_scan.rs:1052-1067`, `baude-core/src/worktree_scan.rs:1072-1088`). Writing a marker into an empty legacy directory would make it non-empty and potentially prevent legitimate cleanup unless marker-aware evidence is designed explicitly.
- **HIGH — “Discover owner from any checkout” is unsafe for already-collided directories.** The scanner can locate a `.git` holder in an immediate child (`baude-core/src/worktree_scan.rs:1094-1126`), but adoption must inspect all checkout children and require unanimous common-dir identity. Picking the first can assign ownership incorrectly.
- **MEDIUM — Silent adoption errors conflict with safe migration.** Startup restoration currently reports lifecycle recovery errors to the user (`baude/src/app.rs:1276-1289`, `bauded/src/manager.rs:490-515`). Silently ignoring ambiguous or failed ownership discovery can leave paths unresolved while presenting startup as successful.
- **MEDIUM — Eager startup scanning threatens the milestone’s first-frame goal.** `start_workspace` runs before terminal setup and before `App::restore` (`baude/src/main.rs:351-371`, `baude/src/main.rs:423-430`), and before daemon restore (`bauded/src/main.rs:190-215`). Walking every managed directory and invoking Git for unmarked legacy entries belongs behind measurement or a lazy/admission scan.
- **MEDIUM — The file manifest contradicts the tasks.** The task says it updates callers in `workspace.rs`, `app.rs`, and `bauded/api.rs`, all of which contain direct calls (`baude-core/src/workspace.rs:522`, `baude/src/app.rs:1993`, `bauded/src/api.rs:868`), but those files are absent from this plan’s `files_modified`. `app.rs` is instead assigned to the next wave.

### Suggestions

- Make Plan 14-02 revolve around one API such as `resolve_managed_repository_dir(common_dir, workspace) -> ResolvedManagedDirectory`.
- Have the result carry the selected physical key, ownership source, migration action, and optional collision report.
- Resolve in this order: matching marker, unanimous legacy checkout discovery, unclaimed preferred digest path, deterministic extended-digest fallback.
- Pass the resolved physical key explicitly into both default-checkout and branch activation path composition; do not derive it from `RepositoryKey`.
- Treat scanner/pruner compatibility as a full subtask: string key propagation, numeric state-reference semantics, report-version bump, marker-aware contents evidence, forged-report validation, and prune regression tests.
- Make adoption lazy or cached, and add a measured startup budget if it remains in `start_workspace`.
- Add TestRedirect integration tests for state deletion/recreation, TUI/daemon parity, legacy continuation, ambiguous legacy contents, forced digest collision, concurrent claims, and scan→preview→prune re-verification.

### Risk Assessment

**HIGH.** This plan currently cannot achieve WTID-01 or WTID-02 because it changes path parameter types without establishing how repository identity resolves to a physical directory. It also risks regressions in the destructive scan/prune safety boundary.

## 14-03

### Summary

The final plan correctly identifies the user-facing integration surfaces, but it does not meet the stated collision UX or scan ownership requirements. Logging a collision in the daemon is not equivalent to returning ownership and a resolution to the remote user, and the current scan model reports repository directories rather than individual managed checkouts.

### Strengths

- Moving TUI admission toward the shared lifecycle gate removes existing duplication. `App::admit_repository` currently reimplements repository lookup/allocation instead of calling `ensure_repository` (`baude/src/app.rs:1914-1947`).
- Daemon branch admission already uses the same lifecycle activation pipeline as TUI, providing a solid parity seam (`bauded/src/manager.rs:857-889`, `baude/src/app.rs:2268-2300`).
- Adding ownership to the serialized core report naturally reaches `--json`, because the CLI serializes `ScanReport` directly (`baude/src/main.rs:965-993`).
- README’s Worktrees section is the appropriate documentation location; it currently describes the managed root and lifecycle but not repository-directory identity or migration (`README.md:325-339`).

### Concerns

- **HIGH — WTID-03 is not satisfied for daemon/PWA users.** The daemon’s public creation path returns a session or an error through `POST /sessions` (`bauded/src/manager.rs:759-804`). Merely logging a collision at info level neither names the owner nor offers a resolution to the API client.
- **HIGH — The TUI behavior is underspecified.** Current admission errors are converted into a generic “repository admission failed” message (`baude/src/app.rs:4750-4767`). Returning `CollisionReport` from `admit_repository` does not ensure the owner and non-destructive action are actually displayed.
- **HIGH — The scan data model does not represent “each managed checkout.”** A `Candidate` currently represents exactly one repository directory with relative path `[workspace, repository-<key>]` (`baude-core/src/worktree_scan.rs:734-755`). Child checkouts are only inspected as directory contents (`baude-core/src/worktree_scan.rs:1052-1067`). An `owner` field on `Candidate` can report one repository-directory owner, not an owner per checkout.
- **HIGH — No tests are planned for this wave.** The source already has separate admission implementations and separate state files (`baude/src/app.rs:1914-1994`, `bauded/src/manager.rs:1015-1048`; `baude-core/src/worktree_scan.rs:324-330`). Without cross-binary fixtures, the promised TUI/daemon parity and state-reset behavior remain assertions rather than verification.
- **MEDIUM — The proposed return types conflate unrelated identities.** `admit_repository` currently returns `Option<u64>` for a runtime/session result (`baude/src/app.rs:1914`, `baude/src/app.rs:4760-4765`), while `ensure_repository` returns a `RepositoryKey` (`baude-core/src/lifecycle.rs:1307-1340`). Adding collision tuples to both does not supply the physical managed-directory identity needed downstream.
- **MEDIUM — Daemon admission has more than one path.** Non-worktree sessions go through `record_checkout_intent`, which independently allocates repository state (`bauded/src/manager.rs:1007-1048`), while managed branches use lifecycle activation (`bauded/src/manager.rs:857-875`). “Call ensure_repository” needs an explicit refactor of both flows and their capacity preflight (`bauded/src/manager.rs:1351-1368`).
- **MEDIUM — Scan ownership must preserve prune safety and privacy semantics.** JSON reports are saved and later handed back as prune approval (`baude/src/main.rs:974-1002`). Any owner field must be re-derived during prune, included in the report-version change, and preferably use a user-meaningful repository path without weakening exact identity comparison.

### Suggestions

- Define a shared `AdmissionNotice`/`CollisionReport` with owner display path, conflicting managed path, selected replacement path, and resolution reason.
- Surface that notice in the TUI message/details view and in the daemon API response or retrievable session event—not only logs.
- Decide whether WTID-04 means ownership per repository-directory candidate or per child checkout. If it truly means each checkout, introduce a child record such as `ManagedCheckoutOwnership` rather than adding one field to `Candidate`.
- Add explicit tests for:
  - identical physical resolution in TUI and daemon;
  - independent TUI/daemon state files;
  - both state files deleted and reconstructed from markers;
  - legacy directory reused without rename or clone;
  - collision response naming the incumbent and replacement;
  - text and JSON scan output;
  - scan/prune compatibility after the schema change.
- Document marker filename/version, normal digest layout, legacy retention, collision fallback, state-reset recovery, and what the operator sees or can safely do.

### Risk Assessment

**HIGH.** The plan has suitable UI and daemon touchpoints, but its proposed behavior does not actually deliver the required collision resolution to remote users and cannot represent ownership at the granularity promised by WTID-04.

# Cross-plan assessment

| Requirement | Coverage | Assessment |
|---|---|---|
| WTID-01 | Partial | Stable hashing is proposed, but fixed-prefix collisions, concurrent ownership claims, and replacement-directory selection are undefined. |
| WTID-02 | Insufficient | Markers can label legacy directories, but no resolver returns that legacy physical key to future path composition. |
| WTID-03 | Insufficient | TUI presentation is unspecified; daemon logging is not a user-facing resolution. |
| WTID-04 | Partial | Owner output is planned, but the existing report is repository-directory-level rather than checkout-level, and prune/schema implications are omitted. |

The dependency waves are conceptually ordered, but Wave 1 must first establish a complete physical-directory resolution contract—not merely hashing and marker I/O. Wave 2 should integrate that resolver and fully migrate the scan/prune schema. Wave 3 should then expose the already-proven collision and ownership results through TUI, daemon API, CLI, and documentation.

I would block execution until the plans explicitly answer these four design questions:

1. What exact physical key is returned for a marked legacy directory?
2. What deterministic key is used when the first 12 digest characters collide?
3. How is ownership claimed atomically between concurrent TUI and daemon processes?
4. Does scan report ownership per repository directory or per checkout, and how is that incorporated into prune re-verification?
