---
phase: 14-managed-worktree-identity
fixed_at: 2026-09-21T19:55:00Z
review_path: 14-REVIEW.md
iteration: 1
findings_in_scope: 5
fixed: 5
skipped: 0
status: all_fixed
---

# Phase 14: Code Review Fix Report

**Fixed at:** 2026-09-21
**Source review:** 14-REVIEW.md
**Iteration:** 1

**Summary:**
- Findings in scope: 5 (critical_warning scope: CR-01, WR-01, WR-02, WR-03, WR-04)
- Fixed: 5
- Skipped: 0
- Info findings (IN-01 scheme-too-new wording, IN-02 physical_key validation at deserialization, IN-03 documenting the best-effort parent sync) left as-is; IN-02 is covered by RepositoryState validation of key shapes on load.

## Fixed Issues

### CR-01: Error swallowing on marker write
**File modified:** `baude-core/src/lifecycle.rs` (ensure_repository)
**Commit:** d701a3e
**Applied fix:** the marker write result is matched; `OwnedByOther` and `UnknownOwner` (which also carries I/O failures) return `LifecycleError::Topology` and the repository entry is not recorded, so a directory is never left claimed-in-state but unmarked on disk.

### WR-01: Redundant create_dir_all with swallowed error
**File modified:** `baude-core/src/lifecycle.rs`
**Commit:** d701a3e
**Applied fix:** removed the second `create_dir_all`; the earlier one with explicit error handling is the only creation site.

### WR-02: Lost I/O error in unknown-owner path
**File modified:** `baude-core/src/lifecycle.rs` (check_collision)
**Commit:** d701a3e
**Applied fix:** the `Err(error)` arm formats the error into `CollisionReason::UnknownOwner("io error: ...")` so the report stays diagnosable.

### WR-03: Check-then-create race in suffix allocation
**File modified:** `baude-core/src/lifecycle.rs` (allocate_suffixed_repository_path)
**Commit:** d701a3e
**Applied fix:** each candidate is claimed with exclusive `std::fs::create_dir`; `AlreadyExists` falls through to the marker check (reuse if ours, skip otherwise); other I/O errors propagate.

### WR-04: Silent allocator exhaustion
**File modified:** `baude-core/src/lifecycle.rs`
**Commit:** d701a3e
**Applied fix:** bound is a named constant and the exhaustion error names the base directory, the tried range, and the remedy.

## Verification
- `cargo fmt --all -- --check` 0, `cargo clippy --all-targets -- -D warnings` 0, `cargo build --workspace` 0, `cargo test --workspace` 0 (717 passed, 0 failed)
- lifecycle collision tests: 10 passed
