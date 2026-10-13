---
phase: 13-workspace-derivation-and-new-session-open-defaults
reviewed: 2026-09-20T00:00:00Z
depth: standard
files_reviewed: 12
files_reviewed_list:
  - baude-core/src/folder_workspace.rs
  - baude-core/src/launch.rs
  - baude-core/src/lib.rs
  - baude-core/src/workspace.rs
  - baude-core/src/worktree_scan.rs
  - baude/src/app.rs
  - baude/src/main.rs
  - baude/src/remote.rs
  - baude/src/ui.rs
  - bauded/src/api.rs
  - bauded/src/main.rs
  - README.md
findings:
  critical: 0
  warning: 0
  info: 0
  total: 0
status: clean
---

# Phase 13: Code Review Report (Fix-Loop Iteration 2)

**Reviewed:** 2026-09-20
**Depth:** standard
**Files Reviewed:** 12
**Status:** clean

## Summary

Re-review of phase 13 following fixes to CR-01 and WR-01 (commit 8052137). The previous review identified a critical symlink-boundary issue in the ancestor walk and an implicit canonicalization contract. Both have been addressed correctly, and no additional issues were found in the full file scope.

## Verification of Fixes

### CR-01 Fix: Ancestor Walk Symlink Boundary (VERIFIED CORRECT)

**Implementation:** `baude-core/src/folder_workspace.rs:89-95`

The home directory is now canonicalized before comparison with the canonicalized launch_dir:

```rust
let canonical_home = match home.canonicalize() {
    Ok(ch) => ch,
    Err(_) => {
        home.to_path_buf()
    }
};
```

The boundary check then uses this canonical home for consistent comparison:

```rust
if current == canonical_home {
    return None;
}
```

**Validation:** This prevents the walk from escaping the home boundary on systems with symlinked home directories (e.g., macOS where `/var` → `/private/var`). The fix ensures that even when `home` is reached through a symlink, it is canonicalized before comparison, matching the canonicalized `launch_dir`.

### WR-01 Fix: Explicit Canonicalization Contract (VERIFIED CORRECT)

**Implementation:** `baude-core/src/folder_workspace.rs:77-83`

The function doc comment now explicitly documents the canonicalization requirement:

```rust
/// # Canonicalization
///
/// **Important:** `launch_dir` must be canonicalized by the caller for correct home boundary
/// checking via `std::fs::canonicalize`. The `home` path is canonicalized internally to ensure
/// consistent comparison, even when reached through symlinks. On systems where the home
/// directory is a symlink (e.g., macOS with `/var` → `/private/var`), this ensures the
/// walk stops at the correct boundary and does not escape to parent directories.
```

This removes the implicit contract and documents the requirement explicitly, improving maintainability.

### New Test: `find_binding_respects_symlinked_home_boundary` (VERIFIED CORRECT)

**File:** `baude-core/src/folder_workspace.rs:432-485`

The test validates the fix by:
1. Creating a symlinked home directory: `fixture/home-link -> fixture/real-home`
2. Creating a launch directory inside the real home: `fixture/real-home/projects/myrepo`
3. Recording a binding at a directory above home: `fixture/above`
4. Calling `find_binding` with the symlinked home path
5. Asserting that the walk stops at the boundary and does NOT find the binding above home

The test correctly validates that canonicalization prevents the boundary escape.

---

## Full File Scope Re-Review

All 12 files in the change scope were reviewed for correctness, security, and code quality issues. Key findings:

**Launch directory canonicalization:** Verified consistent across all entry points:
- TUI (`baude/src/main.rs:347`): Canonicalizes argv[1] or current_dir before passing to start_workspace
- Daemon (`bauded/src/main.rs:178`): Canonicalizes current_dir before passing to start_workspace
- Shared (`baude-core/src/launch.rs:73`): Defensive canonicalization as safety measure

**Workspace name sanitization:** Confirmed safe from path traversal. The `sanitize()` function (workspace.rs:151-161) converts all non-alphanumeric characters (except `_` and `-`) to `-`, preventing names like `../evil` from escaping the config directory.

**Lock-refusal safety:** Binding recording is guarded by successful lock acquisition (launch.rs:104-116). Error paths return without recording, preserving the single-writer invariant.

**New-session directory prefill:** Paths are properly canonicalized in the submission phase (`app.rs:4069` via `persist::expand_tilde` → `.canonicalize()`), preventing symlink-based escapes.

**Remote daemon `/info` JSON:** The `workspace_source` field is derived from the internal workspace object's `display_hint()` method, which returns hardcoded strings ("explicit", "folder binding", "derived", "blank"). This is not user input, and parsing it in the TUI is safe.

**Folder-workspaces.json durability:** Uses advisory locks and read-merge-write pattern with graceful degradation on corruption. Single-writer invariant maintained via OS-level file descriptor locks.

No additional issues identified.

---

_Reviewed: 2026-09-20_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
