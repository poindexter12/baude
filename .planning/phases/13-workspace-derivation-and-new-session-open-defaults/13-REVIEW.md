---
phase: 13-workspace-derivation-and-new-session-open-defaults
reviewed: 2026-09-20T00:00:00Z
depth: standard
files_reviewed: 11
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
findings:
  critical: 1
  warning: 1
  info: 0
  total: 2
status: issues_found
---

# Phase 13: Code Review Report

**Reviewed:** 2026-09-20
**Depth:** standard
**Files Reviewed:** 11
**Status:** issues_found

## Summary

This phase adds workspace derivation from launch folder ancestor walks, shared startup helpers used by both TUI and daemon, workspace source labels for display in title bars and remote headers, and new-session directory prefill based on repository detection. The core design is sound and the shared `launch::start_workspace` function ensures TUI/daemon parity for lock claiming, workspace initialization, and folder binding recording.

**Key security-relevant surfaces reviewed:**
- Reading/writing `~/.config/baude/folder-workspaces.json` with ancestor walk logic
- Workspace name derivation from filesystem paths and sanitization before use in state files
- Lock acquisition before binding recording; single-writer guarantee via OS advisory locks
- Canonicalization of launch directories and home boundaries
- Untrusted remote daemon `/info` JSON parsed for workspace/workspace_source fields

One critical issue and one warning found.

## Critical Issues

### CR-01: Ancestor Walk May Cross Home Directory Boundary on Symlinked Paths

**File:** `baude-core/src/folder_workspace.rs:76-96`

**Issue:** The `find_binding` ancestor walk compares canonicalized paths (`current`) against a non-canonicalized home path (`home`), allowing the walk to escape above the home directory boundary on systems where the home directory is reached through symlinks (common on macOS where `/var` → `/private/var`).

Path comparison flow:
1. `launch_dir` is canonicalized in `launch.rs` to `/private/var/home/user/...`
2. `home` comes from `persist::home_dir()` which returns `dirs::home_dir()` without canonicalization: `/var/home/user`
3. After popping in the ancestor walk, `current = /private/var/home/user`
4. Comparison: `if current == home` → `/private/var/home/user` ≠ `/var/home/user` → FALSE
5. Walk continues past home, potentially records or retrieves bindings from `/private/var` or higher

This violates the documented behavior: "The walk stops at your home directory (`$HOME`) and does not continue above it."

**Impact:** On macOS and other systems with symlinked home directories, workspace bindings recorded at or above the parent directory (e.g., `/var`) could be discovered and used, causing unexpected workspace derivation. Users with unusual filesystem topologies could have their workspace choice influenced by shared-parent bindings they didn't intend.

**Fix:**

Canonicalize the home directory before the ancestor walk loop, or canonicalize paths for the boundary comparison:

```rust
pub fn find_binding(root: &Path, launch_dir: &Path, home: &Path) -> Option<String> {
    let (file, _) = load_json::<FolderWorkspaceFile>(&root.join(FILE_NAME));

    // Canonicalize home for consistent comparison with canonicalized launch_dir.
    // If canonicalization fails, fall back to non-canonical comparison (no walk occurs).
    let canonical_home = match home.canonicalize() {
        Ok(ch) => ch,
        Err(_) => {
            // Home doesn't exist; use it as-is for comparison (walk will likely not match anyway)
            home.to_path_buf()
        }
    };

    let mut current = launch_dir.to_path_buf();
    loop {
        let key = folder_key(&current);
        if let Some(entry) = file.folders.get(&key) {
            return Some(entry.workspace.clone());
        }

        // Stop if we've reached home (now with consistent canonicalization).
        if current == canonical_home {
            return None;
        }

        // Stop if we've reached the root.
        if !current.pop() {
            return None;
        }
    }
}
```

---

## Warnings

### WR-01: Ancestor Walk Symlink Sensitivity Without Canonicalization at Call Sites

**File:** `baude-core/src/folder_workspace.rs:76-96`, cross-referenced with `baude-core/src/launch.rs:73, 131-132`

**Issue:** The `find_binding` function is called in `plan_launch` with:
- `launch_dir` (canonicalized by caller, see `launch.rs:73`)
- `home` (non-canonicalized from `persist::home_dir()`)

The symlink sensitivity in CR-01 means the ancestor walk relies on the caller to canonicalize `launch_dir` but receives a non-canonicalized `home`. This creates implicit coupling: if a future call to `find_binding` passes a non-canonicalized launch_dir, the boundary check would fail even earlier (never finding any bindings).

**Impact:** Subtle correctness issue and maintainability risk. The function's behavior depends on implicit canonicalization of one parameter but not the other.

**Fix:**

Document the canonicalization requirement explicitly in the function signature, or (better) have `find_binding` canonicalize paths internally for comparison:

```rust
/// Find a recorded folder binding by walking up the directory tree from `launch_dir`.
///
/// **Important:** `launch_dir` must be canonicalized by the caller for correct home boundary
/// checking. This function assumes `launch_dir` has been canonicalized via `std::fs::canonicalize`.
/// The `home` path is canonicalized internally for boundary comparison.
pub fn find_binding(root: &Path, launch_dir: &Path, home: &Path) -> Option<String> {
    // ... implementation with home canonicalization as per CR-01 fix
}
```

Alternatively, remove the canonicalization requirement by having the function canonicalize `launch_dir` itself, but this may have performance implications if called frequently.

---

## Summary of Non-Issues

**Workspace name sanitization:** Confirmed safe. The `sanitize()` function converts all non-alphanumeric characters (except `_` and `-`) to `-`, preventing path traversal in state file names.

**Lock handling:** Correct. The lock is claimed in `launch::start_workspace` before binding recording. The OS advisory lock (file descriptor-based) ensures the kernel releases it on process exit, preventing stale lock files.

**Remote daemon JSON:** Untrusted strings from `/info` are accepted as-is for display. The `workspace_source` field values should be constrained ("explicit", "folder binding", "derived", "blank"), but rendering untrusted strings in the UI is not a security issue.

**New-session directory prefill:** Correct. Uses canonicalized paths (`repo_root` from git, `launch_dir` from canonicalization, or config value). No injection risk.

**Daemon startup parity:** Correct. Using shared `start_workspace` function ensures TUI and daemon follow identical workspace resolution and binding recording logic.

---

_Reviewed: 2026-09-20_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
