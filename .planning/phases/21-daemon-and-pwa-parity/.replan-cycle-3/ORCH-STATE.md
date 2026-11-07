# plan-phase 21 --reviews --chunked — orchestrator state
PHASE=21 PADDED=21 NAME="Daemon and PWA Parity" SLUG=daemon-and-pwa-parity STATUS=Planned(replan)
PHASE_DIR=/Users/joese/Code/github.com/poindexter12/baude/.planning/phases/21-daemon-and-pwa-parity
MODE=reviews CHUNKED=true TDD=true MVP=false TRACER=true REVERSIBILITY_GATES=true GRANULARITY=standard
commit_docs=true plan_checker_enabled=true nyquist=true text_mode=false models=inherit(omit model=)
REQ_IDS="API-01, API-02, API-03"
PATHS: state=.planning/STATE.md roadmap=.planning/ROADMAP.md reqs=.planning/REQUIREMENTS.md
  context=21-CONTEXT.md research=21-RESEARCH.md reviews=21-REVIEWS.md(76KB) patterns=21-PATTERNS.md(new)
  validation=21-VALIDATION.md spec=NONE uispec=NONE aispec=NONE api_surface=.planning/intel/API-SURFACE.md(247B,stale)
DONE: 1 init, 1.5 closed-gate PASS, 2.5 reviews-prereq PASS, 4 context, 4.5 no AI-SPEC, 4.6 ctx-drift PASS,
  5 research SKIPPED(--reviews), 5.5 validation exists, 5.55 threat-dups=0, 5.6 ui-gate frontend=false,
  5.65 codebase-drift skipped, 6 plans exist(6)+--reviews, 7 paths, 7.8 PATTERN MAPPING COMPLETE, 7.9 intel done
SPECLESS: EDGE_ABSENT=1 PROHIB_ABSENT=1 SPECLESS_FALLBACK=true disabled_note=NONE coverage=$SP/COVERAGE.json
  4 edges all unresolved: API-01 concurrency, API-02 UNCLASSIFIED(->flagged assumption, never auto-resolve), API-03 idempotency, API-03 concurrency
DONE+: 7.95 specless probe
DONE+: 7.99 stall helpers read (interval/threshold defaults 5/10), 8 prompt assembled
PLANNER_CONTEXT=$SP/PLANNER-CONTEXT.md (filled verbatim planning_context incl. 5 contribs + COVERAGE + protected blocks)
DEVIATION: chunk prompts POINT AT that file instead of inlining 27KB x7 (orchestrator context economy); content delivered verbatim
NEXT: 8.5.1 outline spawn -> 8.5.2 per-wave plan chunks -> 10 checker -> 12 revision loop -> 13-15
CONTRIBS: $SP/planner-contribs.md (15192B, order ai-integration,assumption-delta,schema-gate,security,tdd)
HOOKS: $SP/hooks-pre.json   INIT: $SP/init.raw
PATTERN-MAPPER CONFLICTS for planner: (1) D-09 vs passing test bauded/src/api.rs:1002-1010 forbids "checkout_key";
  (2) RESEARCH Pattern 2 names futures_util (dev-only) but convention is async_stream; (3) baude/src/remote.rs discards bodies.
OPEN DECISIONS: REQUIREMENTS.md:82 vs D-12 scope; use_worktrees flip in 0aee924 needs revert (outside this run).
chunked_parallel=false dispatch_capacity=20 stall_detection=true
DONE+: 7.99 stall helpers read; 8 prompt assembled -> $SP/PLANNER-CONTEXT.md (35KB, verbatim: 5 contribs, COVERAGE, all protected blocks)
DEVIATION-1: chunk prompts POINT AT PLANNER-CONTEXT.md rather than inlining 35KB x7 (orchestrator context economy); content delivered verbatim to each chunk.
DEVIATION-2: stall_detection=true but this runtime's Agent tool has NO run_in_background param and returns no async_launched outputFile, so {outputFile} cannot be bound -> taking the documented `false` branch (await runtime-native completion notification, consume the real result). Proven working this session by the pattern-mapper spawn. NOT fire-and-forget.
NEXT: 8.5.1 outline spawn -> 8.5.2 per-wave plan chunks (serial, chunked_parallel=false) -> 10 checker -> 12 revision loop -> 13-15

## 8.5.2 DISPATCH QUEUE (serial, chunked_parallel=false; --reviews so existing plans are OVERWRITTEN not skipped)
W1: 21-01
W2: 21-02 (checkpoint:decision D-09, autonomous:false)
W3: 21-07, 21-09
W4: 21-10
W5: 21-03
W6: 21-04
W7: 21-05, 21-08
W8: 21-06
W9: 21-11 (checkpoint:human-verify, autonomous:false)
Commit after EACH verified plan, in outline row order:
  gsd_run query commit "docs(21): plan <id> (chunked)" --files "<PD>/<id>-PLAN.md"
Chunk prompt = pointer to PLANNER-CONTEXT.md + 21-PLAN-OUTLINE.md row (objectives NOT retyped; they are long and on disk)
STATUS: dispatching 21-01
PROGRESS: outline committed 09fd57f (11 plans/9 waves, all 10 HIGHs owned, 1 reviewer suggestion rejected w/ reason)
  21-01 DONE+COMMITTED 04c7345 (3 tasks, tracer, HIGH#1 merge_live_runtime_metadata, HIGH#2 sh -c "sleep 30" + ps probe, 7 truths, 3 prohibitions, 18-row ledger)
  21-02 DISPATCHED (awaiting return) -- checkpoint:decision on D-09, autonomous:false
  REMAINING AFTER 21-02: W3 21-07,21-09 | W4 21-10 | W5 21-03 | W6 21-04 | W7 21-05,21-08 | W8 21-06 | W9 21-11
  THEN: step 10 plan-checker (checker_model="" -> omit model), step 12 revision loop, 13-15 state/next-up
RESUME RECIPE: re-read this file + 21-PLAN-OUTLINE.md; chunk prompt = pointer to PLANNER-CONTEXT.md + outline row + dep plans; commit each with gsd_run query commit "docs(21): plan <id> (chunked)" --files <PD>/<id>-PLAN.md
  21-02 DONE+COMMITTED e8ca9d3 (self-committed by planner; 3 tasks, checkpoint:decision blocking-human, /rows fields pinned, autonomous:false)
  21-07 DISPATCHING (wave 3, serial)

## RESUME POINT (context exhausted 2026-11-06 ~17:15 PST)

DONE + committed: outline 09fd57f | 21-01 04c7345 | 21-02 e8ca9d3 | 21-PATTERNS 6431160
IN FLIGHT at halt: 21-07 (spawned, writes 21-07-PLAN.md itself; check `git ls-files` for it)
REMAINING (serial, chunked_parallel=false), in wave order:
  21-09 (w3) -> 21-10 (w4) -> 21-03 (w5) -> 21-04 (w6) -> 21-05 (w7) -> 21-08 (w7) -> 21-06 (w8) -> 21-11 (w9)

HOW TO RESUME (fresh window):
  Do NOT re-run plan-phase from step 0 with --reviews: the chunked resume rule
  OVERWRITES existing plans when --reviews is set, which would redo 21-01/02/07.
  Instead dispatch the remaining chunks directly, one gsd-planner subagent per plan:
    - Give each chunk: this dir's PLANNER-CONTEXT.md (verbatim planning_context,
      already filled — do not regenerate it), 21-PLAN-OUTLINE.md (its own row),
      21-PATTERNS.md, and any dependency plan files named in its Depends On cell.
    - After each returns: `gsd_run query verify plan-structure --phase 21 --plan NN`
      then `gsd_run query commit "docs(21): plan NN ..." --files <plan>`
  THEN step 9: spawn gsd-plan-checker over all 11 plans; iterate to pass (max 3).
  Stall detection: config says true, but this runtime cannot bind {outputFile}
  (Agent tool has no run_in_background / async_launched). Use the documented
  false branch: await the native completion notification.

OPEN DECISIONS FOR JOE (do not let an executor guess):
  1. D-09 vs bauded/src/api.rs:1002-1010 -- 21-02 Task 1 is a blocking
     checkpoint:decision. Option A amend-test (one-way, breaks published
     compatibility invariant) vs Option B rows-only + client join (reversible).
  2. use_worktrees was flipped accidentally in 0aee924 -- still needs reverting.
