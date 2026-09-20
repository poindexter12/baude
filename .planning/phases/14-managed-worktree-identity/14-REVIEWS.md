---
phase: 14
reviewers: [codex]
reviewed_at: 2026-09-20T18:45:47Z
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

## GPT-5.3-Codex Review

## 14-01 Plan Review (Marker module and digest resolution)

### 1) Summary
This plan has a solid intent (shared digest identity + marker ownership source of truth), but it mixes foundational work with substantial policy and fallback machinery that introduces avoidable complexity and several correctness contradictions. As written, parts of the implementation would likely block migration-in-place and conflict with the phase’s non-destructive goals.

### 2) Strengths
- Clear focus on deterministic identity and shared core logic between TUI/daemon.
- Good emphasis on atomic marker writes and fail-closed marker reads.
- Strong test-first posture, including recovery and collision scenarios.
- Explicit threat model thinking (symlink handling, bounded reads, malformed JSON behavior).

### 3) Concerns
- **HIGH:** `write_marker` design is contradictory (`create_dir` ownership claim on an already-existing repository dir). Migration requires writing marker *inside* existing `repository-*` dirs; this approach would fail normal paths.
- **HIGH:** Adds `repository_identity` + digest-length fallback (12→16→…→64) in tracer scope, which is likely over-engineering before basic WTID behavior is proven.
- **HIGH:** `HashMap<Vec<u8>, String>` state mapping (introduced later, implied here) is risky for JSON persistence semantics and compatibility.
- **MEDIUM:** 1 KiB marker read cap may reject legitimate markers as fields evolve (especially with base64 path bytes + extra fields).
- **MEDIUM:** Non-UTF8 byte-preservation across all layers is thoughtful but may be unnecessary complexity unless the surrounding state model already uses byte-path semantics end-to-end.
- **LOW:** Adds `sha2` dependency changes despite likely existing workspace dependency; potential duplication churn.

### 4) Suggestions
- Keep 14-01 minimal: deterministic 12-hex digest + marker read/write (atomic temp file in existing dir), no fallback-length expansion yet.
- Remove `create_dir` from marker writes; use `create_new` temp file + rename within existing managed repo dir.
- Use a marker schema aligned to locked decisions only: canonical common dir, scheme version, recorded-at (avoid premature `physical_key` unless required by callsites).
- Keep path identity types as UTF-8 canonical strings unless there is a demonstrated cross-platform need for byte-level path storage.
- Reserve resolver/fallback policy for 14-02 where integration concerns are handled holistically.

### 5) Risk Assessment
**Overall risk: HIGH**  
The architectural direction is right, but there are fundamental implementation contradictions (especially marker write semantics) that can break migration-in-place and derail WTID-02/03 if not corrected early.

---

## 14-02 Plan Review (Path composition, scanner migration, collision integration)

### 1) Summary
This is the critical integration plan and it correctly targets the right surfaces (`git.rs`, `lifecycle.rs`, `worktree_scan.rs`), but it currently carries several cross-cutting inconsistencies that could prevent WTID guarantees from actually holding in production, especially around collision fallback behavior and key-format parsing.

### 2) Strengths
- Correctly changes managed path composition to string repository keys.
- Explicitly preserves legacy directory readability while introducing digest keys.
- Includes scanner schema/version migration and marker-awareness (important for tools/prune safety).
- Captures non-fatal collision handling intent in admission path.

### 3) Concerns
- **HIGH:** Collision handling path is underspecified for continuation. Returning `Some(CollisionReport)` is not enough; plan must guarantee newcomer gets a distinct directory immediately (WTID-03).
- **HIGH:** Parser rule conflict: `repository_key` only accepts 12-char hex, but earlier fallback logic proposes longer digest keys. These plans conflict directly.
- **HIGH:** `common_dir_to_physical_key: HashMap<Vec<u8>, String>` in persisted state is a high-risk schema choice; JSON map-key behavior and backward compatibility are unclear.
- **MEDIUM:** “Lazy adoption only” is good for perf, but recovery guarantees after state reset depend on marker presence; plan should ensure marker adoption occurs reliably during admission/scan before key resolution decisions.
- **MEDIUM:** Task boundaries are blurred (14-02 still shaping APIs/UI behavior that 14-03 also owns), increasing rework risk.
- **LOW:** Verification commands are brittle and likely to be flaky due to filter usage patterns.

### 4) Suggestions
- Define exact collision continuation contract in `ensure_repository`: on mismatch, allocate/resolve alternate physical key *in the same function* and return both report + resolved key.
- Unify key format strategy across plans: either fixed 12 hex everywhere or variable-length digests everywhere (parser, docs, tests, scan).
- Replace `Vec<u8>` map keys in persisted state with stable string form (canonical path string) unless byte keys are absolutely required.
- Add explicit state schema migration/defaulting tests for pre-v2.3 state files.
- Make marker adoption trigger points explicit and guaranteed: admission path + scan path, with idempotent writes and clear event/reporting.

### 5) Risk Assessment
**Overall risk: MEDIUM-HIGH**  
The plan is close to phase goals but has unresolved internal contradictions that can cause partial correctness and mismatched behavior across components.

---

## 14-03 Plan Review (Admission UX, daemon parity, scan ownership output, docs)

### 1) Summary
This plan appropriately focuses on user-visible behavior and cross-binary parity, but it introduces additional modeling complexity (per-checkout ownership structures) and contains output-policy contradictions that should be resolved before implementation to avoid churn and accidental information leaks.

### 2) Strengths
- Correctly treats collisions as non-fatal in both TUI and daemon flows.
- Good emphasis on parity testing between TUI and daemon.
- Includes required WTID-04 scan ownership surfacing and README updates.
- Keeps migration non-destructive and explicitly documents legacy compatibility.

### 3) Concerns
- **HIGH:** Internal contradiction on path exposure: prohibitions say don’t expose canonical dirs in user-facing output, but task text proposes printing owner paths directly in scan text rows.
- **MEDIUM:** Per-checkout ownership structures may be more than required for WTID-04 and can bloat complexity; requirement is owner reporting, not necessarily full nested model redesign.
- **MEDIUM:** API shape churn risk (`/sessions` response wrapper/optional collision field) without explicit compatibility strategy for existing clients.
- **MEDIUM:** Reintroduces base64 canonical-dir serialization patterns that may not be needed if this is daemon/admin-only data.
- **LOW:** Task scope mixes UX, API contract, scan model, and docs in one wave, increasing coupling risk.

### 4) Suggestions
- Resolve output policy first: text UI should show stable display owner (`repository-<key>` / display name), keep canonical dirs in JSON/debug only.
- Keep WTID-04 minimal: add owner fields to existing scan candidate model first; defer deeper per-checkout nested model unless required by current consumers.
- Version API responses or add additive optional fields only, with explicit backward-compat tests.
- Align README and CLI output terminology with actual implementation fields (owner display vs canonical path).

### 5) Risk Assessment
**Overall risk: MEDIUM**  
This plan can achieve WTID-03/04, but only if it trims optional complexity and resolves the canonical-path exposure contradiction.

---

## Overall Cross-Plan Assessment

### Summary
The plan set is directionally strong and requirement-aware, but it is currently over-specified in some low-value areas and under-specified in one crucial area: guaranteed non-destructive collision continuation. The biggest risks are internal inconsistencies (key format, marker semantics, output policy) rather than missing intent.

### Top cross-plan fixes before execution
- Normalize one repository key strategy across all plans (fixed 12 hex vs variable length).
- Fix marker write semantics for existing dirs (no `create_dir` ownership claim).
- Specify and test collision continuation behavior in one authoritative place (`ensure_repository`).
- Simplify state serialization types for compatibility and maintainability.
- Clarify user-facing vs admin/debug ownership data exposure policy end-to-end.

### Overall Risk Level
**MEDIUM-HIGH**  
Achievable phase with strong foundations, but requires targeted de-risking of contradictory details before implementation starts to avoid late-cycle rewrites.

---

---

## Codex Review — Cycle 3

## 14-01

### Summary

The plan chooses the right identity primitive—Git’s canonical common directory—and sensibly separates durable ownership metadata into a shared core module. However, the proposed marker write is atomic as a file replacement, not atomic as an ownership claim. That race undermines the collision-prevention guarantee at the foundation of the phase. The plan also omits a required direct `base64` dependency and leaves malformed-marker behavior internally inconsistent.

### Strengths

- The digest input matches the repository identity already established by the code. `discover_repository` canonicalizes the input and the absolute `--git-common-dir` result before storing it in the snapshot ([baude-core/src/git.rs:321](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/git.rs:321), [baude-core/src/git.rs:348](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/git.rs:348)).

- Lossless path bytes fit the current persistence model. `PersistedPath` already stores `Vec<u8>` and uses Unix `OsStrExt`/`OsStringExt` for round trips ([baude-core/src/repository.rs:38](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/repository.rs:38)).

- Reusing the existing durability sequence is appropriate. Current state persistence uses an exclusively created temporary file, flush, `sync_all`, rename, and parent-directory sync ([baude-core/src/persist.rs:631](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/persist.rs:631), [baude-core/src/persist.rs:649](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/persist.rs:649)).

- Bounded deserialization, symlink rejection, non-UTF-8 coverage, foreign-owner tests, and scheme-version preservation are all useful hardening beyond the minimum phase requirement.

### Concerns

- **HIGH — Atomic replacement does not provide an exclusive ownership claim.** The planned algorithm reads the existing marker, writes a unique temporary file, and renames it over the destination. Two writers can both observe no marker and both successfully rename; the later rename replaces the earlier marker on Unix. The existing persistence code avoids temporary-name collisions with `create_new`, but serializes final state replacement with a held state lock ([baude-core/src/persist.rs:629](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/persist.rs:629), [baude-core/src/persist.rs:637](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/persist.rs:637), [baude-core/src/persist.rs:665](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/persist.rs:665)). The proposed `concurrent_marker_writes` expectation that only one claimant wins is therefore not implemented by the described mechanism.

- **HIGH — The declared artifact set cannot compile as written.** `MarkerMetadata` requires base64 serialization, but the plan adds only `sha2` to `baude-core`. `baude-core` currently has neither dependency ([baude-core/Cargo.toml:13](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/Cargo.toml:13)); `base64` and `sha2` are direct dependencies only of the daemon ([bauded/Cargo.toml:14](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/bauded/Cargo.toml:14), [bauded/Cargo.toml:26](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/bauded/Cargo.toml:26)). Transitive dependencies are not importable in Rust.

- **MEDIUM — Malformed-marker semantics contradict the later plans.** Plan 14-01 correctly returns parse, size, and symlink errors. Plan 14-02’s threat model says malformed markers become “ownership unknown and continue,” but its admission steps only distinguish matching and foreign `Some(marker)` cases. Because current path creation treats any pre-existing path as a collision ([baude-core/src/git.rs:1077](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/git.rs:1077)), the unknown-owner branch needs a precise fail-closed outcome.

- **MEDIUM — The digest API does not enforce the canonical-path invariant.** `compute_repository_digest(&[u8])` can hash arbitrary bytes even though the plan prohibits non-canonical inputs. The canonicalization guarantee currently belongs to `discover_repository`, not an arbitrary byte slice ([baude-core/src/git.rs:325](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/git.rs:325)). Incorrect call sites would silently create different identities.

- **MEDIUM — Matching-owner rewrites weaken marker immutability.** The plan allows a matching canonical directory to overwrite the marker without specifying checks for `scheme_version` or `physical_key`. A same-owner call could therefore change the physical key or recorded time. That conflicts with the stated “marker is source of truth” model.

### Suggestions

- Implement marker claiming as an actual compare-and-create operation. Suitable approaches include a marker lock file acquired with `create_new`, or creation of the final marker itself with `create_new` followed by durability syncing. On `AlreadyExists`, read and compare the winner. Do not rely on overwriting rename for first ownership.

- Add `base64 = "0.22"` explicitly to `baude-core/Cargo.toml`, or use a serde representation already available without another dependency.

- Split the APIs into:

  - `claim_marker`, which never overwrites an existing valid marker;
  - `read_marker`, which distinguishes missing, valid, corrupt, and unsafe;
  - optionally `rewrite_marker` only for an explicit future schema migration.

- Accept `&PersistedPath` or a clearly named `canonical_common_dir_bytes` newtype for digesting, or provide `compute_repository_digest(&Path)` that canonicalizes internally.

- Define unknown ownership as a first-class result and require callers to leave the directory untouched.

### Risk Assessment

**HIGH.** The proposed ownership primitive does not satisfy its concurrent-writer test or the phase’s “never collide across TUI and daemon” guarantee. This must be corrected before downstream resolver work is reliable.

## 14-02

### Summary

This plan identifies most of the necessary source surfaces, especially the path helpers and scanner schema. Its core data model is not yet coherent, though: it preserves numeric logical `RepositoryKey` values while introducing string physical keys without carrying the latter through lifecycle requests. More seriously, a naïve scanner migration from numeric to string keys can make live digest directories appear unreferenced to the prune engine. The plan also contradicts the scanner’s established read-only contract by proposing scan-time marker adoption.

### Strengths

- Changing path composition from numeric to string repository keys is correctly scoped. The current functions embed numeric keys directly in both repository directory forms ([baude-core/src/git.rs:1776](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/git.rs:1776), [baude-core/src/git.rs:1798](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/git.rs:1798)).

- Preserving the `repository-<key>/<role>-<checkout>` shape minimizes migration impact and retains the scanner’s bounded directory model ([baude-core/src/worktree_scan.rs:863](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/worktree_scan.rs:863)).

- Bumping `REPORT_FORMAT_VERSION` is mandatory and correctly recognized. Prune explicitly refuses reports whose version differs from the current format ([baude-core/src/worktree_scan.rs:1286](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/worktree_scan.rs:1286)).

- The resolver ordering—state mapping, marker discovery, then fresh digest—is directionally right for preserving legacy directories.

- Requiring unanimous checkout identity before adopting an unmarked legacy directory is a good fail-closed rule.

### Concerns

- **HIGH — Logical and physical keys are conflated.** `RepositoryKey` is and remains a private `u64` wrapper ([baude-core/src/repository.rs:11](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/repository.rs:11)). `ensure_repository` currently returns that logical key, and `prepare_activation` puts it into `ActivationRequest` before deriving the managed path from `repository.get()` ([baude-core/src/lifecycle.rs:1307](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/lifecycle.rs:1307), [baude-core/src/lifecycle.rs:1343](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/lifecycle.rs:1343)). A return type of `(RepositoryKey, CollisionReport)` cannot make its first element “the 12-hex digest,” as the planned test claims. `PreparedActivation` needs the resolved physical key or resolved path explicitly.

- **HIGH — The scanner-to-state mapping is underspecified and potentially destructive.** The scanner currently extracts numeric `SavedRepository.key` and `SavedCheckout.repository_key` values from state ([baude-core/src/worktree_scan.rs:578](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/worktree_scan.rs:578)), then matches them to the numeric key parsed from the directory name ([baude-core/src/worktree_scan.rs:654](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/worktree_scan.rs:654)). Simply changing both types to `String` risks comparing logical `"1"` with physical `"abcdef123456"`. A live digest directory could then receive `NotReferencedByState`, which participates in removable classification and later prune re-verification.

- **HIGH — Scan-time adoption violates an explicit safety contract.** `scan_at` is documented as unconditionally read-only—no file, lock, or probe directory may be created ([baude-core/src/worktree_scan.rs:831](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/worktree_scan.rs:831)). The CLI tells users the command “only reads” ([baude/src/main.rs:821](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/main.rs:821)). Calling marker adoption from scan would silently break both guarantees.

- **HIGH — State-reset recovery does not cover checkout-key recovery.** Resetting state resets both repository and checkout counters to one ([baude-core/src/repository.rs:490](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/repository.rs:490)). Recovering only the repository directory still allows the next `primary-1` or branch path to collide with an existing child. Current default worktree creation hard-fails when the requested path exists unless Git inventory already finds the relevant branch elsewhere ([baude-core/src/git.rs:1064](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/git.rs:1064), [baude-core/src/git.rs:1077](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/git.rs:1077)). The plan needs child-checkout reconciliation or allocation above the maximum recognized checkout key.

- **HIGH — Daemon legacy adoption is missing from the specified ordering.** The plan names future TUI admission and scanner call sites, but not daemon admission. The daemon has independent repository-recording paths, including `record_checkout_intent` ([bauded/src/manager.rs:1007](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/bauded/src/manager.rs:1007)) and branch activation through `prepare_activation` ([bauded/src/manager.rs:857](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/bauded/src/manager.rs:857)). An old unmarked daemon directory could therefore be bypassed in favor of a new digest directory, violating in-place migration.

- **HIGH — Unknown-owner behavior is incomplete.** The locked decision says an existing directory whose ownership cannot be proved must be left alone. The action instead proceeds to “create the chosen directory” after marker checking, but does not define what happens for missing, malformed, oversized, or symlink markers. Current Git code treats occupied paths as collisions rather than safe claims ([baude-core/src/git.rs:1344](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/git.rs:1344)).

- **MEDIUM — The new state map is weakly modeled.** A `BTreeMap` cannot contain duplicate keys, so validating “no duplicate encoded keys” adds nothing. The important validations are:

  - every saved repository has exactly one physical key;
  - no two different common directories claim one physical key;
  - the physical key conforms to a recognized shape;
  - checkouts resolve through their owning repository.

  None is specified.

- **MEDIUM — Marker creation becomes a filesystem mutation before durable lifecycle ownership.** The current branch flow deliberately records pending checkout ownership and saves it before Git activation ([bauded/src/manager.rs:872](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/bauded/src/manager.rs:872), [bauded/src/manager.rs:882](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/bauded/src/manager.rs:882)). The plan has `ensure_repository` create directories and markers before that save, but does not specify recovery if persistence then fails.

- **MEDIUM — Verification commands do not reliably run the named tests.** Several commands pass multiple positional filters to the Rust test harness; libtest supports a single filter. The build pipeline also greps for `Compiling|Finished` without `pipefail`, so a failed build that printed “Compiling” can be reported as success.

### Suggestions

- Keep logical and physical identity explicit:

  ```text
  RepositoryKey(u64)        durable relational key
  RepositoryPhysicalKey     legacy decimal | digest | digest suffix
  ```

  Store `physical_key` on `SavedRepository`, rather than in a parallel map, unless a compelling migration reason requires the map.

- Extend `PreparedActivation` or `ActivationRequest` with `physical_key`/`managed_repository_dir`; never derive a physical directory from `RepositoryKey::get()` after this migration.

- During state inventory, build a `RepositoryKey -> physical_key` map per state file. For old state without a physical key, use the numeric logical key. Add a must-pass test proving a state-referenced digest directory is `Live`, never `Removable`.

- Keep `scan_at` read-only. It may infer and report ownership from marker or Git, but marker writing should happen only during admission or through a separately named migration command.

- Add explicit recovery of child checkout identities: inspect marker-owned directory children and Git inventory, adopt matching existing checkouts, and advance checkout allocation past all recognized suffixes.

- Specify four occupied-directory outcomes: owned by us, owned by another repository, unknown owner, and unsafe/unreadable. Only the first may be reused; the second may trigger a suffix; the latter two must be left untouched.

- Use one filter per `cargo test` command, or run a module filter and assert the exact named tests from output. Enable `set -o pipefail` or avoid grep-based build success checks.

### Risk Assessment

**HIGH.** The current plan could misclassify live digest directories for pruning, fail state-reset recovery at the checkout layer, and bypass legacy directories in daemon flows. These are phase-goal and data-safety failures, not implementation polish.

## 14-03

### Summary

The final integration plan correctly recognizes that TUI, daemon, API, scanner output, and documentation all need updates. It does not yet provide a viable collision-information path through either frontend. The TUI task returns a report without displaying it, while the daemon’s existing API and manager types have nowhere to retain the report. The proposed POST response would also be ignored by both current clients. Several promised cross-binary tests cannot be placed in the listed crates without dependency changes.

### Strengths

- Both local and daemon branch creation already converge on core lifecycle activation, making shared physical-key behavior achievable. The TUI uses `prepare_activation` ([baude/src/app.rs:2281](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/app.rs:2281)); the daemon does the same ([bauded/src/manager.rs:872](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/bauded/src/manager.rs:872)).

- Extending scanner serialization is a natural way to satisfy JSON ownership reporting. `ScanReport` and `Candidate` are already fully serialized and versioned ([baude-core/src/worktree_scan.rs:742](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/worktree_scan.rs:742), [baude-core/src/worktree_scan.rs:786](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/worktree_scan.rs:786)).

- Per-checkout collision flags are more informative than a single directory-level owner and align with the requirement to report every managed checkout.

- Keeping canonical paths out of ordinary human-readable output while allowing structured administrative output is a reasonable security posture, provided it is reconciled with the locked collision-reporting decision.

### Concerns

- **HIGH — The TUI does not actually surface the collision.** The task says `admit_repository` returns the report and defers display to a “next phase,” but Phase 14 is responsible for WTID-03. The current admission call site handles only success focus, `None`, and errors ([baude/src/app.rs:4760](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/app.rs:4760)). Returning another tuple member does nothing unless the caller calls `set_message` or stores a durable startup note.

- **HIGH — The proposed daemon response does not match the existing API types.** `POST /sessions` currently returns `Json<SessionInfo>` directly ([bauded/src/api.rs:163](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/bauded/src/api.rs:163)), and `Manager::create` returns `MutationResult<SessionInfo>` ([bauded/src/manager.rs:761](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/bauded/src/manager.rs:761)). There is no separate POST response struct to extend. The plan needs explicit new signatures or a manager-side collision-event store.

- **HIGH — Neither client would show the optional POST field.** The PWA takes the returned object only to navigate to `s.id` ([bauded/web/app.js:542](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/bauded/web/app.js:542)). The remote TUI discards the entire response body and maps success to `()` ([baude/src/remote.rs:201](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/src/remote.rs:201)). Thus the plan does not “offer a non-destructive resolution” to daemon/PWA users even if the field is serialized.

- **HIGH — The plan does not implement the locked `/info` reporting decision.** `/info` currently returns workspace, backend, version, source, and persistence status ([bauded/src/api.rs:117](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/bauded/src/api.rs:117)). Plan 14-03 changes only POST `/sessions`, so collision information is ephemeral and unavailable to later clients.

- **HIGH — `CollisionReport` lacks data later consumers require.** Plan 14-02 defines only requested path, allocated path, and `owner_display`. Plan 14-03 expects owner canonical directory, owner display name, path, and resolution. The locked decision also requires naming the repository that wanted the path. There is no source for those missing fields after the report crosses the lifecycle boundary.

- **HIGH — The specified cross-binary tests have no feasible home.** `baude` and `bauded` each depend on `baude-core`, but neither depends on the other ([baude/Cargo.toml:12](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude/Cargo.toml:12), [bauded/Cargo.toml:16](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/bauded/Cargo.toml:16)). A test inside either crate cannot instantiate both frontends without adding a dependency or a workspace-level integration-test package. Testing the shared resolver twice is not a genuine cross-binary parity test.

- **MEDIUM — Human-readable ownership policy conflicts with the locked context.** The plan prohibits canonical paths in TUI and scan text, while the accepted decision says a collision should name the owner’s canonical common directory and the repository that requested it. Decide which output surfaces count as administrative before implementation; otherwise WTID-03 acceptance will be ambiguous.

- **MEDIUM — Ownership discovery may repeat expensive Git inventory work.** `discover_repository` invokes `git worktree list --porcelain -z` and canonicalizes the full inventory ([baude-core/src/git.rs:300](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/git.rs:300), [baude-core/src/git.rs:355](/Users/joese/.local/share/baude/worktrees/smoke/repository-1/primary-3/baude-core/src/git.rs:355)). Calling it independently for every checkout can become quadratic for large repository directories.

- **MEDIUM — Owner display-name derivation is unspecified.** The marker stores common-dir bytes and a physical key, not a repository display name. `OwnershipInfo.display_name` therefore needs a deterministic derivation or state lookup, especially when a bare repository or nonstandard common directory is encountered.

- **LOW — README verification is brittle.** Exact phrase greps test wording, not correctness. They can fail a well-written edit or pass misleading prose containing the required fragments.

### Suggestions

- Make collision presentation part of this plan:

  - TUI: immediately call `set_message` with owner, requested path, and allocated resolution.
  - Daemon: retain recent admission notices in manager state and expose them through `/info` or a dedicated notices field.
  - PWA: display a toast/banner when POST returns a collision.
  - Remote TUI: deserialize the POST response and surface the same notice.

- Introduce an explicit response type:

  ```rust
  struct CreateSessionResponse {
      session: SessionInfo,
      collision: Option<CollisionInfo>,
  }
  ```

  If backward compatibility requires the existing flat `SessionInfo`, add collision to `SessionInfo` and document that GET/list responses may also carry the last applicable notice.

- Define one core `CollisionReport` containing all required facts: requested path, allocated path, owner physical key, owner canonical common-dir bytes, owner display name, newcomer common-dir bytes/display name, and resolution.

- Put parity tests in `baude-core` around the shared resolver and lifecycle gate. Add separate thin adapter tests in each frontend proving that they call the gate and surface its report. Do not claim a cross-binary test unless a workspace integration harness is added.

- Cache ownership discovery per repository directory or group child checkouts by the common directory returned from a minimal `git rev-parse --git-common-dir` query.

- Replace README phrase greps with a documentation review checklist, while retaining one basic link/heading smoke test if desired.

### Risk Assessment

**HIGH.** As written, collisions can be handled internally but remain invisible to the user, especially through daemon/PWA flows. The API contract and report types are insufficient to implement the promised behavior.

## Cross-plan assessment

The three plans are **not execution-ready**. The most important corrections are:

1. Make marker claiming genuinely exclusive under concurrent TUI/daemon writers.
2. Model logical repository keys and physical directory keys as distinct types and carry both through lifecycle activation.
3. Preserve the scanner’s read-only contract and explicitly map persisted logical keys to physical keys before any prune decision.
4. Recover checkout identities, not only repository directories, after state reset.
5. Add daemon legacy adoption before resolution.
6. Specify unknown/corrupt marker behavior as leave-untouched.
7. Design an end-to-end collision notice path that the TUI, remote TUI, PWA, logs, and `/info` actually expose.
8. Replace the infeasible cross-binary test promise with core parity tests plus adapter tests.

Overall phase risk: **HIGH**. The direction is sound, but the unresolved ownership race, prune-reference mapping, state-reset child recovery, and missing user-visible collision path directly threaten WTID-01 through WTID-03.
