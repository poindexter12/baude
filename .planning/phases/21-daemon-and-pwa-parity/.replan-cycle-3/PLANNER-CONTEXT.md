<planning_context>
**Phase:** 21
**Mode:** reviews

<required_reading>
- .planning/STATE.md (Project State)
- .planning/ROADMAP.md (Roadmap)
- .planning/REQUIREMENTS.md (Requirements)
- .planning/phases/21-daemon-and-pwa-parity/21-CONTEXT.md (USER DECISIONS from /gsd-discuss-phase)
- .planning/phases/21-daemon-and-pwa-parity/21-RESEARCH.md (Technical Research)
- .planning/phases/21-daemon-and-pwa-parity/21-PATTERNS.md (Pattern Map — analog files and code excerpts)
- .planning/phases/21-daemon-and-pwa-parity/21-REVIEWS.md (Cross-AI Review Feedback — --reviews IS SET; actionable findings must be incorporated or explicitly deferred/rejected in PLAN.md)
- .planning/phases/21-daemon-and-pwa-parity/21-VALIDATION.md (Nyquist validation criteria)
- .planning/intel/API-SURFACE.md (API Surface — HINT ONLY; see <intel_surface_hint> below)
</required_reading>

<proven_verify_commands>
**Verify commands the previous phase actually ran (#2401) — reuse before re-deriving.** These
are the `<automated>` commands from the nearest prior phase that had any. They resolved from
the executor's cwd in a real run, so a path here is grounded evidence, not a guess. When this
phase's build/test story is the same, **copy the command verbatim**; do not re-derive a
directory.

- Phase 20 · Task 1 (RED): failing renderer tests over hand-built RemovalReports: `cargo test -p baude ui::tests::remove_project_lines -- --test-threads=1 2>/tmp/baude-20-01-red.log; STATUS=$?; grep -c 'error\[E' /tmp/baude-20-01-red.log; test $STATUS -ne 0`
- Phase 20 · Task 2 (GREEN): implement remove_project_confirmation_lines: `cargo test -p baude ui::tests::remove_project_lines -- --test-threads=1 && cargo clippy -p baude --all-targets -- -D warnings`
- Phase 20 · Task 3 (REFACTOR, conditional): share helpers with remove_confirmation_lines: `cargo test -p baude ui::tests -- --test-threads=1 && cargo fmt --check && cargo clippy -p baude --all-targets -- -D warnings`
- Phase 20 · Task 1: D dispatch, RemoveProject action and RemoteRemove refusal: `cargo test -p baude app::tests::sidebar_action_d -- --test-threads=1 && cargo test -p baude app::tests::d_on_remote_row_sets_footer_notice -- --test-threads=1`
- Phase 20 · Task 2: Modal::ConfirmRemove, prepare_remove_project and confirm_remove_project with handle custody: `cargo test -p baude app::tests::d_opens_confirm_remove_modal -- --test-threads=1 2>/tmp/baude-20-02-red.log; STATUS=$?; grep -c 'error\[E' /tmp/baude-20-02-red.log; test $STATUS -ne 0`
- Phase 20 · Task 2: Modal::ConfirmRemove, prepare_remove_project and confirm_remove_project with handle custody: `cargo test -p baude app::tests::d_opens_confirm_remove_modal -- --test-threads=1 && cargo test -p baude app::tests::d_on_row_with_live_runtime -- --test-threads=1 && cargo test -p baude app::tests::confirm_remove_managed_worktree_drops_row_in_one_save -- --test-threads=1 && cargo test -p baude app::tests::esc_on_confirm_remove_is_non_mutating -- --test-threads=1`
- Phase 20 · Task 3: Repository parent confirm and crate gate: `cargo test -p baude app::tests::confirm_remove_repository_parent_drops_every_child_in_one_save -- --test-threads=1 && cargo fmt --check && cargo clippy -p baude --all-targets -- -D warnings`
- Phase 20 · Task 1: render tests for the ConfirmRemove draw_modal arm at 80% height with scroll: `cargo test -p baude ui::tests::confirm_remove_modal_renders -- --test-threads=1`
- Phase 20 · Task 1: Confirm handle custody and expose the Phase 18 unkillable-runtime double: `cargo test -p baude-core removal::tests -- --test-threads=1 && cargo clippy -p baude-core --all-targets -- -D warnings`
- Phase 20 · Task 2: forced kill failure, forced save failure, retry and clearing-failure notice: `cargo test -p baude app::tests::remove_project_ -- --test-threads=1 2>/tmp/baude-20-04-tests.log; STATUS=$?; tail -n 5 /tmp/baude-20-04-tests.log; exit $STATUS`
- Phase 20 · Task 3: repository gates: `scripts/gate.sh; STATUS=$?; test $STATUS -eq 0 && scripts/gate.sh --linux`
</proven_verify_commands>

<intel_surface_hint>
**API Surface (HINT — may be incomplete):** `.planning/intel/API-SURFACE.md` lists symbols extracted from the codebase by regex/JS analysis. Prefer symbols listed there when referencing existing code. This surface is regex/JS-derived and MAY BE INCOMPLETE — a symbol's absence means *unknown*, not *nonexistent*. Never treat the surface as exhaustive. If you reference a symbol that is not in the surface and this phase creates it, list it under "Artifacts this phase produces". NOTE: this file is small and likely stale; treat it as a weak hint only and prefer 21-PATTERNS.md, which was generated this run against live source.
</intel_surface_hint>

<review_incorporation_contract>
**Mode is reviews:** REVIEWS.md is feedback input, not a hidden execution contract. /gsd-execute-phase primarily consumes PLAN.md plus the normal phase context, so every current actionable review finding must become visible in the relevant PLAN.md before planning can pass.

For each current actionable finding in REVIEWS.md, the planner MUST either:
- incorporate it into a PLAN.md task, `<action>`, `<acceptance_criteria>`, `<verify>`, `must_haves`, threat model, or artifact list; or
- explicitly document a deferral/rejection rationale in the relevant PLAN.md, using the Review Dispositions Ledger in `gsd-core/references/planner-reviews.md`.

Historical findings already incorporated, explicitly deferred/rejected in PLAN.md, or marked fully resolved do not require new plan changes.

**This is replan cycle 3.** Cycles 1 and 2 produced byte-identical plans, which means the prior
two review rounds were NOT absorbed. 21-REVIEWS.md currently carries 10 HIGH and 41 actionable
findings. Treat an unchanged plan as a failure of this run. Every HIGH must be visibly
incorporated or carry an explicit written disposition in the Review Dispositions Ledger.
</review_incorporation_contract>

<orchestrator_flags>
**Conflicts found by the pattern mapper this run — resolve each explicitly in the plan, do not
plan over them:**

1. **D-09 contradicts a currently passing test.** `bauded/src/api.rs:1002-1010` lists
   `"checkout_key"` among the forbidden keys in
   `flat_session_api_remains_a_non_hierarchical_compatibility_projection` — the very test D-09
   promises will keep passing. Either the decision or the test must change; name which, in the
   plan, with the task that does it.
2. **RESEARCH.md Pattern 2 names the wrong dependency.** It specifies
   `futures_util::stream::unfold`, but `futures-util` is a DEV-ONLY dependency; the shipped
   convention in both existing SSE handlers is `async_stream::stream!` with
   `Sse::new(..).keep_alive(KeepAlive::default())`. `async-stream` and `tokio::sync` are already
   runtime deps, so `/events` needs no Cargo change. Plan to the codebase convention, not the
   research text. (This also resolves research assumptions A1 and A2.)
3. **`baude/src/remote.rs` discards every response body** (`.map(|_| ())`, and `short_err` drops
   the `Status` body). Removal needs the opposite: a typed 200 report plus the 409 body read out
   of `ureq::Error::Status(409, response)`. Plan the body-reading change explicitly.

**Also settled by the pattern map:** research assumption A5 — the TUI remote refusal is three
concrete sites: `baude/src/app.rs:299-300` (enum variant), `:343-350` (dispatch), `:1328-1330`
(message).

**Open scope decision the plan must state an assumption for (there is no human in this loop):**
REQUIREMENTS.md:82 ("Removing remote daemon sessions from the TUI attach view" is explicitly
out of scope) versus D-12. State the reading you plan to, mark it as an assumption, and keep the
REQUIREMENTS.md exclusion authoritative unless CONTEXT.md overrides it in writing.

**Error-mapping note from the pattern map:** every `bauded` handler is
`lock(&state).method(..)` + `map_err(|e| mutation_error(e, fallback))` into a plain-text
`(StatusCode, String)`. `RemovalError` is `Display`-only, not Serde, so it needs a parallel
`removal_error` mapper rather than reuse of `mutation_error`.
</orchestrator_flags>

**Phase requirement IDs (every ID MUST appear in a plan's `requirements` field):** API-01, API-02, API-03

<tracked_source_paths>
**Tracked-source paths (#3645):** Every path you write into PLAN.md —
`files_modified`, `must_haves.artifacts`, action paths, and paths inherited
from `21-PATTERNS.md` or prior-phase plans — must name git-tracked source,
never a gitignored install/runtime mirror. Verify existing-file paths with
`git ls-files -- <path>` (non-empty = tracked); resolve a gitignored hit to its
tracked origin. A not-yet-existing path is a new file — keep the intended path.
Re-verify inherited paths: fix a mirror path, never inherit.
</tracked_source_paths>

<!-- gsd:protected:start -->
<failing_direction_contract>
**Stated failing direction (#3172):** Every runnable `<automated>` verify command
you write MUST be followed by a `<fails_when>` sibling naming what output
constitutes failure — an exit code, a string in the output, a missing line. A
command with no expressible failure mode is not an acceptance test.

```xml
<verify>
  <automated>npm --prefix apps/api test -- auth.spec.ts</automated>
  <fails_when>non-zero exit, or "0 passed" in the summary line</fails_when>
</verify>
```

One statement per runnable command, placed immediately after it: within a task
each `<fails_when>` binds to the nearest preceding `<automated>`, and the first
statement after a command is the binding one. Name an OBSERVABLE signal, never
the word "failure" — `non-zero exit` is complete, `the command fails` is a
restatement. `TBD`/`TODO`/`N/A`/`none`/`unknown`/`?`/`-` are rejected outright as
whole values. The `MISSING — Wave 0 …` sentinel is exempt: it is not runnable, so
it has no failure mode to state. Ask yourself: if this command were silently
doing nothing, what in its output would tell me? If you cannot answer, fix the
command — do not invent a statement for it.
Rules + worked examples: @gsd-core/references/planner-failing-direction.md
</failing_direction_contract>
<!-- gsd:protected:end -->

**Project instructions:** Read ./CLAUDE.md (and ../CLAUDE.md, ~/.claude/CLAUDE.md) — follow project-specific guidelines. For this repo specifically: `scripts/gate.sh` before every push; `scripts/gate.sh --linux` for anything touching platform code; never judge a gate step by piping to tail/grep and reading `$?`; never nest parentheses in a commit body or PR title; build fixtures in the shapes real users produce.
**Project skills:** Check .claude/skills/ or .agents/skills/ (if either exists) — read SKILL.md files; plans should account for project skill rules.


# API Coverage Decision Checkpoint

> Full API Coverage by Default — Opt Out, Never Opt In. Fires when a phase
> integrates an external API / SDK / service. Most non-API phases will not fire
> it — that is the point.

## Why this exists

"We integrated the API" too often silently means "we integrated whatever the
first use case exercised." Every un-built capability is then an invisible hole,
discovered later by a user who reasonably expected it to work. The phase sealed
green because its tasks completed; nobody decided the gaps were acceptable,
because nobody enumerated them. This checkpoint makes the surface **visible and
decided** before the phase can seal.

## Detect whether this phase integrates an external API

The detector is a deterministic scan over the phase scope. It strips fenced
code blocks first, so a trigger term inside a code snippet does not fire. It
returns a typed result: `{ detected, signals[], terms }`. Run it on the phase
scope (the concatenation of this phase's ROADMAP section + the PLAN body):

```bash
SCOPE="$(cat "${PHASE_DIR}"/*-PLAN.md 2>/dev/null) $(gsd_run query roadmap.get-phase "${PHASE}" 2>/dev/null || true)"
API_COVERAGE_JSON=$(printf '%s' "$SCOPE" | node gsd-core/bin/lib/api-coverage.cjs --json 2>/dev/null) || true
[ -n "$API_COVERAGE_JSON" ] || API_COVERAGE_JSON='{"skipped":true,"reason":"probe_unavailable"}'
```

The `|| true` neutralizes the assignment's status without discarding the
detector's own payload: the detector exits **1** for a real "no integration"
verdict, so treating any non-zero exit as failure would throw away a correct
answer. Emptiness — not exit status — is what proves the probe never ran, and
the second line is the only place the fragment manufactures a payload of its
own — one that records the *absence* of a verdict rather than asserting one.

The detector's exit code and `--json` payload now distinguish a real negative
from an unexamined input (ADR-3889 Phase 3, #3907): empty/whitespace-only
`$SCOPE` or a stdin read failure emit `{"skipped":true,"reason":"no_input"|
"stdin_error"}` — no `detected` key at all. **Check for `skipped` before
reading `detected`**: a `skipped` payload is not a confirmed "no API
integration" verdict, it means the detector never examined real input. Do not
treat it as `detected:false`. Read `API_COVERAGE_JSON.detected` only when
`skipped` is absent — act on it only, do **not** pattern-match the prose
yourself.

**If `skipped` is `true`:** the detector could not establish a scope (empty
`$SCOPE`) or failed to run (stdin read error). Skip the checkpoint for this
run rather than asserting a verdict about input that was never examined; do
not raise it with the user.

**If `detected` is `false`:** this phase does not integrate an external API. Skip
the checkpoint entirely and continue planning. Do not raise it with the user.

**If `detected` is `true`:** an external-API integration is in scope. You MUST
produce a **coverage matrix** before the plan is finalized.

**If `detected` is `true` but the phase genuinely integrates no external API**
(the detector is deterministic, not infallible — confirm by re-reading the phase
scope, not by preference): do NOT fabricate a matrix row for a capability that
does not exist. Write a reasoned declaration to `${PHASE_DIR}/COVERAGE.md`
instead:

```markdown
No external API integration: <one-line reason — what the phase touches instead>.
```

The reason is required, exactly like an `OPT-OUT` reason. The seal-time gate
accepts this declaration in place of a matrix.

## Produce the coverage matrix

Enumerate the external API's full **capability surface** — the verb/endpoint/method
list (e.g. for a music service: `search`, `play`, `pause`, `skip`, `set_volume`,
`get_playlist`, `create_playlist`, `add_to_playlist`, …). For each capability
record a decision, starting from **full coverage** as the default:

| capability | decision | reason |
|---|---|---|
| `<capability-id>` | `INTEGRATE` \| `OPT-OUT` | `<one-line reason if OPT-OUT>` |

Rules:

- **`INTEGRATE` is the default.** Every capability starts as INTEGRATE; the
  matrix is the *subtraction record*.
- **Every `OPT-OUT` MUST carry a one-line reason** (`not needed`, `not needed
  yet`, `explicitly out of scope`, …). An opt-out without a reason is an
  un-decided hole — the exact failure mode this gate exists to close.
- **A second integration against the same need** (e.g. a second platform for the
  same capability) starts from the **same full-coverage baseline** as the first.
  Do not carry over the first integration's opt-outs silently — re-decide each
  capability for the new surface, so a first-class/fallback asymmetry cannot
  accumulate.

Write the matrix to `${PHASE_DIR}/COVERAGE.md` (canonical markdown-table form):

```markdown
# API Coverage — <service>

> Full coverage by default. Opt-outs are explicit, reasoned decisions.

| capability | decision | reason |
|---|---|---|
| search | INTEGRATE | |
| playlists | INTEGRATE | |
| skip | OPT-OUT | not needed yet — tracked for follow-up phase |
```

A fenced ` ```coverage ` JSON block is also accepted for machine-generated
matrices; the markdown table is preferred (human-editable, diff-friendly).

## The seal-time gate

This checkpoint is enforced. At `verify:pre` the `api-coverage.verify-pre` gate
runs `check api-coverage.verify-pre <phase-dir>`:

- If `COVERAGE.md` exists, it is validated — every row needs a valid decision and
  every `OPT-OUT` a reason. A malformed/partial matrix **blocks the seal**. A
  reasoned `No external API integration: …` declaration (and no rows) passes.
- If `COVERAGE.md` is absent, the detector runs again over the phase scope. If a
  strong external-API-integration signal is found, the seal is **blocked** until a
  matrix is produced. If no signal is found, the phase is treated as a non-API
  phase and the seal proceeds.

So: an API-integrating phase cannot seal without a decided matrix. Produce it at
plan time; do not leave it for seal time.

## Tuning the vocabulary (optional)

The trigger vocabulary is a curated, additive-only set in
`gsd-core/bin/lib/api-coverage.cjs` (`DEFAULT_API_COVERAGE_TERMS`). To widen it
for a project, override at the call site:

```bash
printf '%s' "$SCOPE" | node gsd-core/bin/lib/api-coverage.cjs --json \
  --verbs integrate,wrap,connect,embed --nouns api,sdk,rest,grpc,webhook,plugin
```

The whole checkpoint is toggleable via `workflow.api_coverage_gate` in
`.planning/config.json`.


# Assumption-Delta Architecture Checkpoint

> Advisory, non-blocking. Fires **only** when the phase scope shows a singular→plural / required→optional / derived→chosen transition. When it fires, it surfaces ONE identity-model question before the plan is finalized. Most phases will not fire it — that is the point.

## Why this exists

Most quietly-imported architectural debt does not come from a missing upfront design phase. It comes at the *seam*: a later phase introduces a second case (a second platform, auth method, tenant, region, source of truth) and nobody re-asks whether the original abstraction still names the right thing. The phase that adds the second case is exactly the 20-minute conversation that prevents an afternoon of later cleanup.

## Run the detector

The detector is a deterministic scan over the phase scope text. It strips fenced code blocks first, so a trigger word that appears only inside a code snippet does not fire. It returns a typed result: `{ detected, signals[], terms }`. Resolve it through the `assumption-delta scan` query (same phase-section resolver as `roadmap.get-phase`):

```bash
ASSUMPTION_DELTA_JSON=$(gsd_run query assumption-delta scan "${PHASE}" --json 2>/dev/null) || true
[ -n "$ASSUMPTION_DELTA_JSON" ] || ASSUMPTION_DELTA_JSON='{"skipped":true,"reason":"probe_unavailable"}'
```

> If the phase section cannot be resolved (no `ROADMAP.md` / unknown phase, or a section with no body), the query emits `{ "skipped": true, "reason": "phase_unresolved" }` — **not** `detected:false`. A probe that never had input does not get to assert that this phase changes no core assumption. The checkpoint does not fire either way; the difference is that a skip is now distinguishable from a real negative. Do not block on it.
>
> Optional tuning — pass `--terms <comma-list>` to replace the curated pluralization cues for this project (the `optional`/`chosen` cues keep their defaults): `gsd_run query assumption-delta scan "${PHASE}" --json --terms second,alternative,fallback`.

## Decision branch

Read `ASSUMPTION_DELTA_JSON`. Act on `detected` only — do **not** pattern-match the human prose.

**If `skipped` is `true`:** the detector never examined a phase section — it could not resolve one (`phase_unresolved`) or could not run at all (`probe_unavailable`). Skip the checkpoint for this run rather than asserting a verdict about input that was never examined; do not raise it with the user. **Check for `skipped` before reading `detected`** — a skipped payload carries no `detected` key, and treating its absence as `false` re-creates the fabrication this branch exists to prevent.

**If `detected` is `false`:** this phase does not change a core assumption. Skip the checkpoint entirely and continue planning. Do not raise it with the user.

**If `detected` is `true`:** a core assumption may have lost its monopoly. The `signals[]` array tells you which family fired:

| `kind` | What changed | The question to answer |
|---|---|---|
| `pluralization` | A second X was introduced where there was one (second platform / auth method / tenant / region / source of truth) | Does the current primary key / identity model still name the right noun? |
| `optional` | A required / `only` field became optional | Is the field still the right anchor, or has the anchor moved? |
| `chosen` | A derived value became chosen, or a constant became a parameter | Has a configuration decision become a modeling decision? |

Before finalizing the plan, answer this for the user and record the decision explicitly:

> **Promote vs. add-alongside.** The usual correct move when a generalization occurs is to **promote** the new general representation to the primary and **demote** the old specific one to a detail of one variant — *not* to add the new one alongside the still-required old one. Adding alongside silently contradicts the generalized intent (a later variant that does not fit the old primary can be stored but never confirmed as a default).

Record the outcome in the PLAN.md front matter / a `<assumption_delta_decision>` block:

- The **noun** that is now primary (the generalized identity).
- The **decision**: `promote` | `add-alongside` | `no-change`, with a one-line rationale.
- If `add-alongside`: call it out as accepted debt and note what would force a later promote.

## Optional companion: an invariant test

When `detected` is `true`, suggest (do not require) a contract/invariant test that encodes the now-generalized intent — e.g. *"every confirmed default round-trips through the primary use-path, for every supported variant."* That test goes red the instant a future phase reintroduces the singular assumption, so the regression cannot land silently. If the user accepts, add the test as a task in the plan.

## Tuning the vocabulary (optional)

The trigger vocabulary is a curated, additive-only set in `gsd-core/bin/lib/assumption-delta.cjs` (`DEFAULT_ASSUMPTION_DELTA_TERMS`). Bare "or" is intentionally excluded — it is too common in prose and would make the gate fire constantly. To widen or narrow the cues for a project, override at the call site with `--terms <comma-list>` (replaces the pluralization cues; `optional`/`chosen` keep defaults). The whole checkpoint is toggleable via `workflow.assumption_delta` in `.planning/config.json`.

This checkpoint is advisory: it informs and records; it never blocks the phase.


# Schema Push Detection Gate

> Detects schema-relevant files in the phase scope and injects a mandatory `[BLOCKING]` schema push task into the plan. Prevents false-positive verification where build/types pass because TypeScript types come from config, not the live database.

Check if any files in the phase scope match schema patterns:

```bash
PHASE_SECTION=$(gsd_run query roadmap.get-phase "${PHASE}" --pick section 2>/dev/null)
```

Scan `PHASE_SECTION`, `CONTEXT.md` (if loaded), and `RESEARCH.md` (if exists) for file paths matching these ORM patterns:

| ORM | File Patterns |
|-----|--------------|
| Payload CMS | `src/collections/**/*.ts`, `src/globals/**/*.ts` |
| Prisma | `prisma/schema.prisma`, `prisma/schema/*.prisma` |
| Drizzle | `drizzle/schema.ts`, `src/db/schema.ts`, `drizzle/*.ts` |
| Supabase | `supabase/migrations/*.sql` |
| TypeORM | `src/entities/**/*.ts`, `src/migrations/**/*.ts` |

Also check if any existing PLAN.md files for this phase already reference these file patterns in `files_modified`.

**If schema-relevant files detected:**

Set `SCHEMA_PUSH_REQUIRED=true` and `SCHEMA_ORM={detected_orm}`.

Determine the push command for the detected ORM:

| ORM | Push Command | Non-TTY Workaround |
|-----|-------------|-------------------|
| Payload CMS | `npx payload migrate` | `CI=true PAYLOAD_MIGRATING=true npx payload migrate` |
| Prisma | `npx prisma db push` | `npx prisma db push --accept-data-loss` (if destructive) |
| Drizzle | `npx drizzle-kit push` | `npx drizzle-kit push` |
| Supabase | `supabase db push` | Set `SUPABASE_ACCESS_TOKEN` env var |
| TypeORM | `npx typeorm migration:run` | `npx typeorm migration:run -d src/data-source.ts` |

Inject the following into the planner prompt (step 8) as an additional constraint:

```markdown
<schema_push_requirement>
**[BLOCKING] Schema Push Required**

This phase modifies schema-relevant files ({detected_files}). The planner MUST include
a `[BLOCKING]` task that runs the database schema push command AFTER all schema file
modifications are complete but BEFORE verification.

- ORM detected: {SCHEMA_ORM}
- Push command: {push_command}
- Non-TTY workaround: {env_hint}
- If push requires interactive prompts that cannot be suppressed, flag the task for
  manual intervention with `autonomous: false`

This task is mandatory — the phase CANNOT pass verification without it. Build and
type checks will pass without the push (types come from config, not the live database),
creating a false-positive verification state.
</schema_push_requirement>
```

Display: `Schema files detected ({SCHEMA_ORM}) — [BLOCKING] push task will be injected into plans`

**If no schema-relevant files detected:** Skip silently.


Each PLAN.md must include a <threat_model> block when security enforcement is active. Use the configured ASVS level and blocking threshold from workflow.security_asvs_level and workflow.security_block_on.

<tdd_mode_active>
**TDD Mode is ENABLED.** Apply TDD heuristics to all eligible tasks:
- Business logic with defined I/O → type: tdd
- API endpoints with request/response contracts → type: tdd
- Data transformations, validation, algorithms → type: tdd
- UI, config, glue code, CRUD → standard plan (type: execute)
Each TDD plan gets one feature with RED/GREEN/REFACTOR gate sequence.
</tdd_mode_active>

**TRACER_MODE:** true (false = horizontal layers instead of a leading `type="tracer"` slice; see `planner-mvp-mode.md`.)
**REVERSIBILITY_GATES:** true (false = rate but do not gate; see `planner-reversibility.md`.)
**MVP_MODE:** false (when false, ignore MVP guidance entirely.)
**WALKING_SKELETON:** false
**Granularity:** standard

<specless_probe_fallback>
**Spec-less probe fallback.** Phase 21 has NO SPEC file, so step 7.95 set BOTH `EDGE_ABSENT=1`
and `PROHIB_ABSENT=1`. Author both sections' predicates into `must_haves` via the
`<downstream_consumer>` else-branch below, per §A/§B/§C of
`$HOME/.poindexter/claude/gsd-core/references/specless-probe-fallback.md` (read it now —
descriptor-less prohibitions, never auto-dismiss, no silent drops).

Edge coverage report (deterministic probe run this session over API-01/02/03):

```json
{
  "items": [
    { "requirement_id": "API-01", "category": "concurrency",  "status": "unresolved", "verification": null, "resolution": null, "reason": null, "probe": "If interrupted or run in parallel, what is guaranteed?" },
    { "requirement_id": "API-02", "category": "unclassified",  "status": "unresolved", "verification": null, "resolution": null, "reason": null, "probe": "unclassified — review manually" },
    { "requirement_id": "API-03", "category": "idempotency",   "status": "unresolved", "verification": null, "resolution": null, "reason": null, "probe": "What happens if this runs twice on the same input?" },
    { "requirement_id": "API-03", "category": "concurrency",   "status": "unresolved", "verification": null, "resolution": null, "reason": null, "probe": "If interrupted or run in parallel, what is guaranteed?" }
  ],
  "coverage": { "applicable": 4, "resolved": 0, "unresolved": 4, "unclassified": 1, "byVerification": { "explicit": 0, "backstop": 0 } }
}
```

Apply the `--auto` resolution rules verbatim (§A, D-06):
- The three CLASSIFIED rows (API-01 concurrency, API-03 idempotency, API-03 concurrency) are
  auto-`resolved`: write a defensible acceptance criterion as a plain `must_haves.truths` string
  where you can (verification: explicit); where you genuinely cannot, author a **structured
  flat-scalar marker** `{ statement: <the check>, verification: backstop }` in
  `must_haves.truths` — NOT a prose note, NOT a nested object (#1110 / ADR-550 #1278).
- The API-02 row is **`unclassified`, so it STAYS `unresolved`** — never auto-resolve it with a
  backstop. Surface it as an explicit flagged assumption in the plan. Never silently drop it.
- **Never auto-dismiss** any row.
- Concretely: API-01 concurrency is "two concurrent REST removals of the same row, or a removal
  interrupted mid-flight"; API-03 idempotency is "the PWA remove posted twice for the same row";
  API-03 concurrency is "a PWA removal racing a TUI removal of the same row". These are real
  shapes for this phase, not hypotheticals — Phase 20 already established the three-field recheck
  comparison and restore-on-`Err` custody at `baude/src/app.rs:3908-4086`.

Prohibition recall (§B) runs IN THIS PROMPT — there is no compiled engine. Run the full two-stage
protocol from `$HOME/.poindexter/claude/gsd-core/references/prohibition-probe.md` (read it):
Stage 1 adversarial recall (~10 raw must-NOT candidates per requirement: "what could this
silently become that the author would NOT want, but the requirements do not forbid?"), Stage 2
precision (DROP routine engineering; KEEP values/safety/ethics, ~2–3 survive), then the
canon-referral drop (OWASP / prototype-pollution / path-traversal / injection / GDPR / generic
fairness → one-line breadcrumb and DROP, do not mint). Author each survivor into the
`must_haves.prohibitions:` sibling block **descriptor-less** — NEVER write `check_kind`,
`check_target`, `check_rule`, `check_violation_fixture` or `check_clean_fixture` — so each
disposes flagged-unverified.

**No-silent-drop equality:** for each section, (# probe-surfaced items) == (# authored into
`must_haves` + # surfaced as flagged assumptions).
</specless_probe_fallback>

</planning_context>

<!-- gsd:protected:start -->
<downstream_consumer>
Output consumed by /gsd-execute-phase. Plans need:
- Frontmatter (wave, depends_on, files_modified, autonomous)
- Tasks in XML format with read_first and acceptance_criteria fields (MANDATORY on every task)
- Verification criteria
- must_haves for goal-backward verification
- This phase has NO SPEC, so the `EDGE_ABSENT` else-branch applies: apply the edge lift to the
  fallback report above (per §C of `gsd-core/references/specless-probe-fallback.md`) — resolved
  (verification: explicit) → `must_haves.truths` plain string; resolved (verification: backstop)
  → structured flat-scalar `{ statement, verification: backstop }` marker, NOT a prose note
  (#1110), flat scalar continuation key, never a nested object (ADR-550 #1278). At verify time a
  `backstop` truth the verifier cannot confirm with explicit evidence abstains → `human_needed`
  (reason `insufficient_spec`), never a silent pass (#1154). `unresolved` edges are explicit
  assumptions — surface them in the plan, do not silently drop them.
- `PROHIB_ABSENT` likewise: author the recalled prohibitions into the `must_haves.prohibitions:`
  sibling block (NOT `truths` — ADR-550 D3) via the single `projectProhibitions` serializer
  (Hyrum — no second serializer), **descriptor-less** (no `check_*`) so each disposes
  flagged-unverified; never auto-dismiss. Section-level precedence + no-silent-drop equality apply.
- No `-UI-SPEC.md` exists for this phase (ui-plan-gate resolved `frontend: false`), so the UI lift
  does not apply.
- **"Artifacts this phase produces" section (MANDATORY)** — list every symbol this phase creates:
  decorators, classes, functions, CLI flags, struct/dataclass fields, new file paths. The
  plan-review-convergence source-grounding pass reads this section to exclude newly-created
  symbols from drift verification; omitting it causes new symbols to be flagged for acknowledgement.
</downstream_consumer>
<!-- gsd:protected:end -->

<deep_work_rules>
## Anti-Shallow Execution Rules (MANDATORY)

Every task MUST include these fields — they are NOT optional:

1. **`<read_first>`** — Files the executor MUST read before touching anything. Always include:
   - The file being modified (so executor sees current state, not assumptions)
   - Any "source of truth" file referenced in CONTEXT.md (reference implementations, existing patterns, config files, schemas)
   - Any file whose patterns, signatures, types, or conventions must be replicated or respected

2. **`<acceptance_criteria>`** — Verifiable conditions that prove the task was done correctly. Rules:
   - Every criterion must be checkable as a source assertion, behavior assertion, test command, or CLI output
   - NEVER use subjective language ("looks correct", "properly configured", "consistent with")
   - Include exact strings, patterns, values, command outputs, or observable behavior where that is the right proof

3. **`<action>`** — Must include CONCRETE values, not references. Rules:
   - NEVER say "align X with Y", "match X to Y", "update to be consistent" without specifying the exact target state
   - Include concrete identifiers and reference values: config keys, function signatures, SQL table names, class names, import paths, env vars, endpoint paths, etc.
   - If CONTEXT.md has a comparison table or expected values, copy only the target identifiers/values needed to remove ambiguity
   - Do not include full file contents, fenced code blocks, or complete implementations in `<action>`
   - The executor should understand the intended target state from `<action>` and use `<read_first>` files for current implementation details, patterns, and source-of-truth context

**Why this matters:** Executor agents work from the plan text. Vague instructions like "update the config to match production" produce shallow one-line changes. Concrete instructions produce complete work without turning the planner into the executor.
</deep_work_rules>

<!-- gsd:protected:start -->
<quality_gate>
- [ ] PLAN.md files created in phase directory
- [ ] Each plan has valid frontmatter
- [ ] Tasks are specific and actionable
- [ ] Every task has `<read_first>` with at least the file being modified
- [ ] Every task has `<acceptance_criteria>` with behavior, test-command, CLI, or source assertions
- [ ] Every `<action>` contains concrete identifiers without fenced code blocks or full implementations
- [ ] Dependencies correctly identified
- [ ] Waves assigned for parallel execution
- [ ] must_haves derived from phase goal
- [ ] Every PLAN.md includes an "Artifacts this phase produces" section listing symbols created by this phase
- [ ] Every fallback Edge Coverage resolved edge is represented in a plan's must_haves (no silent drops)
- [ ] Every recalled prohibition is represented in a plan's must_haves.prohibitions, descriptor-less (no silent drops)
</quality_gate>
<!-- gsd:protected:end -->
