---
phase: 13-workspace-derivation-and-new-session-open-defaults
fixed_at: 2026-09-20T00:00:00Z
review_path: 13-REVIEW.md
iteration: 1
findings_in_scope: 2
fixed: 2
skipped: 0
status: all_fixed
---

# Phase 13: Code Review Fix Report

**Fixed at:** 2026-09-20
**Source review:** 13-REVIEW.md
**Iteration:** 1

**Summary:**
- Findings in scope: 2 (critical_warning scope: CR-01, WR-01)
- Fixed: 2
- Skipped: 0

## Fixed Issues

### CR-01: Ancestor Walk May Cross Home Directory Boundary on Symlinked Paths

**File modified:** `baude-core/src/folder_workspace.rs`
**Commit:** 8052137

**Applied fix:**

The `find_binding` function now canonicalizes the home directory before comparison with the canonicalized launch_dir. This ensures consistent path comparison even when the home directory is reached through symlinks.

Key changes:
- Canonicalize home path internally: `home.canonicalize()` with fallback to non-canonical comparison if canonicalization fails
- Updated doc comment with a dedicated "# Canonicalization" section explaining the requirement
- Comparison now uses `canonical_home` instead of `home`, preventing boundary escape on macOS and other systems with symlinked home directories (e.g., `/var` → `/private/var`)

The fix prevents the ancestor walk from escaping above the home directory boundary when the home is reached through a symlink, addressing the security issue where bindings recorded at or above the parent directory could be unexpectedly discovered.

### WR-01: Ancestor Walk Symlink Sensitivity Without Canonicalization at Call Sites

**File modified:** `baude-core/src/folder_workspace.rs`
**Commit:** 8052137

**Applied fix:**

The same commit that fixes CR-01 also addresses WR-01 by documenting the canonicalization contract explicitly in the function's doc comment.

Key changes:
- Added a "# Canonicalization" section to the `find_binding` doc comment
- Documented that `launch_dir` must be canonicalized by the caller (requirement already satisfied in `launch.rs:73`)
- Documented that `home` is canonicalized internally (new in this fix)
- Explained the symlink scenario and why canonicalization is necessary

This eliminates the implicit coupling and maintainability risk by making the canonicalization contract explicit.

## Testing

A new test `find_binding_respects_symlinked_home_boundary` was added to validate the fix:
- Creates a temp directory with a real-home subdirectory and a symlink pointing to it
- Records a binding above the real home boundary
- Verifies that the ancestor walk stops at the symlinked home boundary and does not escape to find the binding above it

All 679 tests pass, including the new test.

## CI Gates

All gates passed with exit code 0:
- `cargo fmt --all -- --check`: exit code 0
- `cargo clippy --all-targets -- -D warnings`: exit code 0
- `cargo build --workspace`: exit code 0
- `cargo test --workspace`: exit code 0

---

_Fixed: 2026-09-20_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
