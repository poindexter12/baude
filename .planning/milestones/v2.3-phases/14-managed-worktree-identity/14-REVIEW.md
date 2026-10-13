---
phase: 14-managed-worktree-identity
reviewed: 2026-09-21T00:00:00Z
depth: standard
files_reviewed: 17
files_reviewed_list:
  - baude-core/Cargo.toml
  - baude-core/src/breadcrumbs.rs
  - baude-core/src/git.rs
  - baude-core/src/lib.rs
  - baude-core/src/lifecycle.rs
  - baude-core/src/marker.rs
  - baude-core/src/persist.rs
  - baude-core/src/repository.rs
  - baude-core/src/workspace.rs
  - baude-core/src/worktree_scan.rs
  - baude/src/app.rs
  - baude/src/hierarchy.rs
  - baude/src/main.rs
  - baude/src/remote.rs
  - baude/src/ui.rs
  - bauded/src/api.rs
  - bauded/src/manager.rs
findings:
  critical: 1
  warning: 4
  info: 3
  total: 8
status: issues_found
---

# Phase 14: Managed Worktree Identity — Code Review

**Reviewed:** 2026-09-21
**Depth:** standard
**Files Reviewed:** 17
**Status:** Issues found (1 critical, 4 warnings, 3 info)

## Summary

Phase 14 implementation introduces stable repository identity via SHA256 digests, marker files for ownership tracking, and collision detection with non-destructive resolution. The core design is sound and most code is well-structured. However, there is one critical issue where marker write errors are silently swallowed, violating the "marker as source of truth" invariant. Additionally, several opportunities for improved error handling and state consistency checks were identified.

## Structural Findings (fallow)

None provided.

## Narrative Findings (AI reviewer)

### CR-01: Error swallowing on marker write violates marker-as-truth design

**File:** `baude-core/src/lifecycle.rs:1462`
**Issue:** The marker write result is discarded with `let _ = crate::marker::write_marker(&allocated_path, &marker_meta);` after successfully creating the repository directory. If the marker write fails (permission denied, out of disk space, I/O error), the directory exists without a marker, but the repository entry is still added to state with `physical_key` set. This violates the design requirement that "the marker is the source of truth for ownership when state is missing or reset" (phase context, lines 18-21).

**Failure scenario:** 
1. Repository A is admitted and `create_dir_all` succeeds at line 1432
2. Marker write fails at line 1462 (e.g., due to permission denied on parent directory)
3. Error is swallowed; SavedRepository is created with physical_key set
4. State is reset (lost or reloaded from disk)
5. `ensure_repository` is called again with the same repository snapshot
6. `check_collision` reads the directory, finds no marker
7. `discover_checkout_owner` returns None (no checkouts yet)
8. Collision detected with reason `UnknownOwner`
9. Repository gets allocated a suffixed path (e.g., `repository-<digest>-2`)
10. Now the original directory remains unclaimed and later admits may incorrectly treat it as unknown owner

This violates the non-destructive invariant: "nothing in ensure_repository... ever writes into, renames, or deletes a directory whose marker... names another repository" and "a repository admitted once (dir + marker written) is found again by a fresh RepositoryState via the marker without collision."

**Fix:**
```rust
// At lifecycle.rs:1462, propagate the error instead of swallowing it:
let marker_write_result = crate::marker::write_marker(&allocated_path, &marker_meta);
match marker_write_result {
    Ok(_) => {
        // Marker was created or already matches
    }
    Err(e) => {
        // If marker write fails, do not add the repository entry to state
        // The directory exists but is uncommitted; fail the admission
        return Err(LifecycleError::Topology(format!(
            "failed to write repository marker: {}", e
        )));
    }
}
```

**Classification:** BLOCKER — Correctness violation of core invariant.

---

### WR-01: Redundant directory creation and lack of error context in marker write

**File:** `baude-core/src/lifecycle.rs:1461`
**Issue:** Line 1461 calls `std::fs::create_dir_all(&allocated_path)` again, immediately after line 1432 already created the directory with explicit error handling. The comment "Design G" suggests this is intentional for safety, but the intent is not clear and the error from the second call is also swallowed with `let _ = ...`. This creates confusion about why two creates are needed and makes the code less maintainable. Additionally, if the first create succeeds but the directory is deleted or becomes a symlink between lines 1432 and 1461, the second create's result is ignored, leaving an inconsistent state.

**Failure scenario:**
1. Line 1432 creates directory successfully
2. Between lines 1432 and 1461, an attacker or stray process deletes the directory or replaces it with a symlink
3. Line 1461 tries to create again; on a symlink, create_dir_all follows the symlink and creates in an attacker-controlled location
4. Error is swallowed, so the code continues
5. Marker is written to the attacker-controlled location or fails silently

**Fix:**
Either remove the redundant create_dir_all (preferred), or document the safety intent and add error handling:
```rust
// Remove line 1461 entirely, or if the double-create is intentional:
// Create the directory with explicit safety check
std::fs::create_dir_all(&allocated_path).map_err(|e| {
    LifecycleError::Topology(format!("failed to create repository directory (second verify): {}", e))
})?;
```

**Classification:** WARNING — Redundant code that masks potential security or consistency issues.

---

### WR-02: Missing error logging in check_collision when discovering checkout owner

**File:** `baude-core/src/lifecycle.rs:1562-1575`
**Issue:** When `discover_checkout_owner` returns an error (line 1562), the error is silently treated as "unknown owner" (line 1566) without logging the underlying I/O error. This makes debugging permission issues, filesystem problems, or symlink attacks difficult. A caller cannot distinguish between "no checkouts found" (benign) and "failed to read directory due to permission denied" (potential issue).

**Failure scenario:**
1. Repository directory exists but parent directory becomes inaccessible (permissions changed, mount fails)
2. `discover_checkout_owner` returns `Err(_)` trying to read directory entries
3. Error is silently converted to `UnknownOwner("io error".to_string())`
4. User sees "collision with unknown owner" instead of "permission denied reading directory"
5. Debugging is difficult because the actual error is lost

**Fix:**
```rust
Err(error) => {
    // Couldn't determine ownership, treat as unknown owner but log the error
    let requester_display = crate::repository::repository_display_name(requester_common_dir);
    eprintln!("Warning: failed to determine collision owner: {}", error);
    Ok(Some(CollisionReport {
        requested_path: target_dir.to_path_buf(),
        allocated_path: target_dir.to_path_buf(),
        owner_common_dir: None,
        owner_display: "unknown owner".to_string(),
        requester_common_dir: requester_common_dir.to_path_buf(),
        requester_display,
        reason: CollisionReason::UnknownOwner(format!("io error: {}", error)),
    }))
}
```

**Classification:** WARNING — Loss of diagnostic information.

---

### WR-03: Potential race condition in suffix allocation between check and create

**File:** `baude-core/src/lifecycle.rs:1703-1727` and `lifecycle.rs:1432`
**Issue:** The suffix allocator in `allocate_suffixed_repository_path` checks if candidate directories exist (line 1709: `if candidate.exists()`) and returns a path when it finds one that doesn't exist. However, between the check and the actual `create_dir_all` call at line 1432 in `ensure_repository`, another process could create the directory. This could cause:

1. The non-existent check passes at line 1709
2. allocate_suffixed_repository_path returns candidate path
3. Another process creates that directory concurrently
4. ensure_repository calls create_dir_all at line 1432, which succeeds (idempotent)
5. ensure_repository tries to write marker at line 1462, which may fail with different error than expected

While the risk is minimal with digest-based keys (two processes would need the exact same repository), the code does not ensure atomicity of the "allocate + create" operation.

**Failure scenario:**
1. TUI and daemon both admit repository with same canonical common dir
2. Both compute the same digest and base path `repository-<digest>`
3. TUI detects collision, allocates `repository-<digest>-2`
4. Between TUI's check (directory doesn't exist) and create_dir_all:
5. Daemon also detects collision, allocates `repository-<digest>-2`
6. Both try to create the same directory; create_dir_all succeeds for both
7. First marker write succeeds; second marker write sees AlreadyExists
8. Second process reads the marker and sees it's owned by TUI's repository
9. Returns OwnedByOther error, which is handled as collision

This is currently handled correctly by the marker write semantics, but relying on marker atomicity for correctness is fragile.

**Fix:**
```rust
// Use O_EXCL semantics to atomically allocate and claim directory:
// Instead of allocate_suffixed_repository_path returning a path and 
// ensure_repository creating it later, create the directory and write 
// the marker in one atomic operation.
```

**Classification:** WARNING — Race condition with low probability but correctness impact if triggered.

---

### WR-04: Suffix allocator bound (2..=1000) never validates feasibility before exhaustion

**File:** `baude-core/src/lifecycle.rs:1703-1732`
**Issue:** The suffix allocator tries suffixes from 2 to 1000 and returns an error if all are exhausted (line 1730-1732). However, there is no check or warning if the allocator reaches high suffix numbers. A user with 1000+ collisions for the same repository would silently fail with a cryptic "could not allocate suffixed repository path" error instead of a more informative message like "suffix allocator exhausted at 1000; manual cleanup required."

Additionally, the bound of 1000 is not documented or configurable, and there's no way for users to know they're approaching the limit until they hit it.

**Failure scenario:**
1. Same repository is admitted 1001 times concurrently (unlikely but possible with test harnesses)
2. Each collision triggers suffix allocation
3. First 1000 succeed
4. 1001st fails with `Err(LifecycleError::Topology("could not allocate suffixed repository path"))`
5. User sees cryptic error and cannot easily understand the suffix exhaustion

**Fix:**
```rust
// Add bounds checking and informative error:
for suffix in 2..=1000 {
    // ... existing logic ...
    if suffix >= 950 {
        eprintln!("Warning: suffix allocator approaching limit ({}/1000)", suffix);
    }
}

Err(LifecycleError::Topology(
    "suffix allocator exhausted at 1000 collisions; manual cleanup required".to_string(),
))
```

**Classification:** WARNING — User experience degradation at scale; difficult to diagnose.

---

### IN-01: Marker parsing accepts future scheme versions as Invalid instead of explicit handling

**File:** `baude-core/src/marker.rs:182-186`
**Issue:** When reading a marker with `scheme_version > CURRENT_SCHEME_VERSION`, the code returns `Ok(MarkerRead::Invalid(InvalidReason::SchemeTooNew(...)))`. This is fail-closed as designed, but the `SchemeTooNew` variant is never matched or handled specially anywhere. All Invalid variants are treated identically in collision detection and ownership discovery, so there's no way to communicate to the user that this directory is owned by a newer version of baude and should not be touched.

**Failure scenario:**
1. Future version of baude writes marker with scheme_version = 2
2. Older version of baude reads marker and sees scheme_version > 1
3. Marker is returned as Invalid(SchemeTooNew(2))
4. Ownership discovery returns None
5. Directory is treated as unknown owner
6. New repository collision, allocated suffix

This is safe (fail-closed) but does not preserve the original directory for the newer version.

**Fix:**
Consider treating `SchemeTooNew` as "owned by unknown version" rather than "unknown owner", so ownership is preserved:
```rust
if meta.scheme_version > CURRENT_SCHEME_VERSION {
    // Directory owned by a newer version of baude
    return Ok(MarkerRead::Invalid(InvalidReason::SchemeTooNew(meta.scheme_version)));
}
```
Then in collision detection, treat SchemeTooNew as an external owner:
```rust
Ok(crate::marker::MarkerRead::Invalid(crate::marker::InvalidReason::SchemeTooNew(version))) => {
    let requester_display = crate::repository::repository_display_name(requester_common_dir);
    Ok(Some(CollisionReport {
        owner_display: format!("owned by baude v{} (newer)", version / 100),
        reason: CollisionReason::UnknownOwner(format!("scheme version {} too new", version)),
        // ...
    }))
}
```

**Classification:** INFO — Edge case with low probability, fail-closed semantics are safe but suboptimal user experience.

---

### IN-02: SavedRepository physical_key should validate format at deserialization

**File:** `baude-core/src/repository.rs:314-315`
**Issue:** The `physical_key: String` field uses `#[serde(default)]` to allow loading old JSON without the field, which is correct for backward compatibility. However, there is no validation that the deserialized physical_key is a valid format (12 hex chars, or 12 hex + suffix, or legacy decimal). A corrupted state file with an invalid physical_key string would not be caught until path composition fails at runtime.

This is low-risk because worktree_scan's `repository_key` parser (worktree_scan.rs:1095-1121) validates the format, but it would be more robust to validate at deserialization.

**Failure scenario:**
1. State file is manually edited or corrupted
2. physical_key field contains invalid value like "repository-invalid-key"
3. State is loaded successfully (no validation)
4. prepare_activation calls physical_key accessor and gets "repository-invalid-key"
5. Path composition creates `~/.local/share/baude/worktrees/<workspace>/repository-repository-invalid-key/...`
6. Later, scan reads this directory and rejects it as non-matching shape

**Fix:**
Add validation in SavedRepository::validate() or add a TryFrom implementation:
```rust
impl SavedRepository {
    pub fn validate_physical_key(&self) -> Result<(), ValidationError> {
        // Validate physical_key format if non-empty
        if !self.physical_key.is_empty() {
            // Accept legacy decimal, 12 hex, or 12 hex + suffix
            if self.physical_key.parse::<u64>().is_ok() {
                // Legacy decimal, OK
            } else if self.physical_key.len() == 12 
                && self.physical_key.chars().all(|c| c.is_ascii_hexdigit()) {
                // New digest, OK
            } else if let Some((hex, suffix)) = self.physical_key.rsplit_once('-') {
                if hex.len() == 12 && hex.chars().all(|c| c.is_ascii_hexdigit())
                    && suffix.parse::<u64>().is_ok() {
                    // Suffixed digest, OK
                } else {
                    return Err(ValidationError::InvalidPhysicalKey(self.physical_key.clone()));
                }
            } else {
                return Err(ValidationError::InvalidPhysicalKey(self.physical_key.clone()));
            }
        }
        Ok(())
    }
}
```

**Classification:** INFO — Data validation improvement; low-risk but good for robustness.

---

### IN-03: No documentation of error swallowing semantics in marker module

**File:** `baude-core/src/marker.rs:107-108`
**Issue:** The marker write function swallows the parent directory sync error at line 108 with `let _ = parent_file.sync_all();`. This is reasonable (best-effort durability), but it's not documented. A future maintainer might interpret this as a bug rather than intentional trade-off.

**Fix:**
```rust
// Sync parent directory on Unix to ensure durability (best-effort, errors ignored)
#[cfg(unix)]
{
    let parent = dir;
    if let Ok(parent_file) = std::fs::File::open(parent) {
        // Best-effort durability; failure does not affect correctness
        let _ = parent_file.sync_all();
    }
}
```

**Classification:** INFO — Documentation improvement for maintainability.

---

## Critical Issues

### CR-01: Error swallowing on marker write violates marker-as-truth design

**File:** `baude-core/src/lifecycle.rs:1462`
**Issue:** See narrative findings above.
**Fix:** Propagate marker write errors instead of swallowing them, or refactor to ensure directory creation and marker writing are atomic.

---

## Warnings

### WR-01: Redundant directory creation and lack of error context
### WR-02: Missing error logging in check_collision
### WR-03: Potential race condition in suffix allocation
### WR-04: Suffix allocator bound never validated before exhaustion

See narrative findings above for details and fixes.

---

## Info

### IN-01: Marker parsing accepts future scheme versions without special handling
### IN-02: SavedRepository physical_key should validate format at deserialization
### IN-03: No documentation of error swallowing semantics in marker module

See narrative findings above for details and improvements.

---

_Reviewed: 2026-09-21_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
