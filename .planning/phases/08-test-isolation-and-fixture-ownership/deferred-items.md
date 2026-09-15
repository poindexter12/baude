# Deferred items — phase 08

Out-of-scope discoveries logged during execution. Not fixed by the plan that
found them.

## `baude-core/src/lifecycle.rs` test module holds no `TestRedirect` (found: 08-03)

**Discovered during:** 08-03 task 1, while measuring the blast radius of the
per-fixture identity change.

**Symptom:** five tests fail with the plan-01 containment guard:

```
lifecycle::tests::blocked_activation_retry_round_trips_all_provenance
lifecycle::tests::occupied_protected_checkout_blocks_activation_recovery_merge
lifecycle::tests::occupied_protected_checkout_refuses_activation_overwrite
lifecycle::tests::pending_activation_recovery_distinguishes_absent_and_exact_git_facts
lifecycle::tests::post_verification_compensation_failure_records_typed_recovery_child
```

each panicking at `baude-core/src/testing.rs`:

> managed worktree root resolved to the real user path
> `/Users/joese/.local/share/baude/worktrees` during a test; hold a
> `baude_core::testing::TestRedirect` on this thread, or set
> `BAUDE_TEST_FIXTURE_ROOT` for a re-exec'd child process

**Pre-existing, measured:** the identical five failures were reproduced at
08-03's RED commit (`f6a3b1c`, before the identity change landed) —
`cargo test -p baude-core --lib lifecycle` → exit 101, 41 passed / 6 failed
(the sixth, `git::tests::lifecycle::creation_safety::durable_keys_not_labels_
supply_bounded_path_identity`, WAS in 08-03's file set and is fixed in
`c860e26`). They are plan-01 fallout, not an 08-03 regression.

**Why deferred:** `baude-core/src/lifecycle.rs` appears in no phase-08 plan's
`files_modified` — not 08-01 through 08-08. Fixing it here would be an
unbudgeted scope expansion into a file another plan may still restructure.

**Containment status:** SAFE. `assert_contained` *panics* rather than letting
the resolution through, so these are resolution-only escapes: no file under a
real user root is read or written. The guard is doing its job; the fixtures
simply have not been migrated yet.

**Fix shape:** the test module holds zero `TestRedirect`s (`grep -c TestRedirect
baude-core/src/lifecycle.rs` → 0). Each of the five needs a root guard, and a
literal identity guard alongside it now that `workspace::active()` requires
fixture provenance. Same pattern as `c860e26`.

**Suggested owner:** plan 08-06, which already owns the cross-cutting
`scripts/assert-real-roots-untouched.sh` + CI gate and cannot go green while
these five fail. If 08-06's budget will not absorb it, this needs its own plan
before the phase can claim a clean `cargo test -p baude-core --lib`.
