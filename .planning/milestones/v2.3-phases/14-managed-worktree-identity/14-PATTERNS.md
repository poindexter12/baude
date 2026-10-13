# Phase 14: Managed Worktree Identity - Pattern Map

**Mapped:** 2026-09-20
**Files analyzed:** 11 new/modified files
**Analogs found:** 11/11 (100% coverage)

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `baude-core/src/marker.rs` | utility/library | file-I/O | `baude-core/src/persist.rs` | exact (atomic save pattern) |
| `baude-core/src/repository.rs` | library | transform | `baude-core/src/repository.rs` (existing) | exact (same file, new function) |
| `baude-core/src/git.rs` | library/utility | transform | `baude-core/src/git.rs` (existing) | exact (same file, signature update) |
| `baude-core/src/lifecycle.rs` | service | CRUD | `baude-core/src/lifecycle.rs` (existing) | exact (same file, collision logic) |
| `baude-core/src/launch.rs` | service | request-response | `baude-core/src/launch.rs` (existing Phase 13) | exact (same file, marker adoption) |
| `baude-core/src/worktree_scan.rs` | service | CRUD | `baude-core/src/worktree_scan.rs` (existing) | exact (same file, key parsing & owner field) |
| `baude-core/src/lib.rs` | module config | — | `baude-core/src/lib.rs` (existing) | exact (same file, mod declaration) |
| `baude/src/app.rs` | controller | request-response | `baude/src/app.rs` (existing) | exact (same file, admission handling) |
| `baude/src/main.rs` | handler | request-response | `baude/src/main.rs` (existing) | exact (same file, scan output) |
| `bauded/src/manager.rs` | service | request-response | `bauded/src/manager.rs` (existing) | exact (same file, daemon admission) |
| `README.md` | documentation | — | `README.md` (existing) | exact (same file, Worktrees section) |

## Pattern Assignments

### `baude-core/src/marker.rs` (utility/library, file-I/O)

**Analog:** `baude-core/src/persist.rs` (atomic file save pattern)

**Imports pattern** (lines 1-10):
```rust
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};
```

**Marker metadata struct** (modeled on StateFile in persist.rs, lines 18-23):
```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct MarkerMetadata {
    pub canonical_common_dir: String,
    pub scheme_version: u32,  // 1 for digest-based
    pub recorded_at_ms: u64,  // Unix ms
}
```

**Atomic write pattern** (based on persist.rs:611-691, atomic_save_current function):
- Temp file creation with PID-based uniqueness: `format!(".baude-marker.json.tmp-{}", std::process::id())`
- Write → flush → sync_all on temp file (lines 655-660 in persist.rs)
- Rename temp to destination atomically
- On Unix, directory sync after rename (lines 673-679 in persist.rs)
- Error cleanup: drop file handle and remove temp on failure (lines 683-686 in persist.rs)

**Fail-closed read pattern** (based on persist.rs error handling):
- Missing marker file → Ok(None) (not an error)
- Malformed JSON → Err (but caller should treat as unknown owner)
- Apply to: `pub fn read_marker(dir: &Path) -> Result<Option<MarkerMetadata>>`

---

### `baude-core/src/repository.rs` (library, transform)

**Analog:** `baude-core/src/repository.rs` (existing allocate_repository_key function, lines 653-664)

**Digest computation function** (new function, style from existing allocate functions):
```rust
pub fn compute_repository_digest(common_dir: &Path) -> Result<String, std::io::Error> {
    use sha2::{Sha256, Digest};
    
    let canonical = common_dir.canonicalize()?;
    let path_bytes = canonical.to_string_lossy().as_bytes();
    let mut hasher = Sha256::new();
    hasher.update(path_bytes);
    let digest = hasher.finalize();
    Ok(format!("{:x}", digest)[0..12].to_string())
}
```

**Pattern:** Follows existing fn structure in repository.rs:
- Deterministic computation (no randomness)
- Fallible with Result<String, Error>
- Returns normalized, hashable output
- Parallel to `allocate_repository_key()` in structure but deterministic, not stateful

---

### `baude-core/src/git.rs` (library/utility, transform)

**Analog:** `baude-core/src/git.rs` (existing functions, lines 1779-1819)

**Path composition signature update** (lines 1779-1783, modified from u64 to &str):
```rust
// BEFORE: pub fn managed_default_worktree_path(repository_key: u64, checkout_key: u64) -> PathBuf {
// AFTER:
pub fn managed_default_worktree_path(repository_key: &str, checkout_key: u64) -> PathBuf {
    worktrees_base()
        .join(&crate::workspace::active().name)
        .join(format!("repository-{repository_key}"))  // key is now String (hex digest)
        .join(format!("primary-{checkout_key}"))
}

pub fn managed_branch_worktree_path(
    repository_key: &str,  // Changed from u64
    checkout_key: u64,
    branch: &str,
) -> PathBuf {
    let sanitized = sanitize(branch);
    let mut label = String::new();
    for character in sanitized.chars() {
        if label.len() + character.len_utf8() > 48 {
            break;
        }
        label.push(character);
    }
    if label.is_empty() {
        label.push_str("branch");
    }
    worktrees_base()
        .join(&crate::workspace::active().name)
        .join(format!("repository-{repository_key}"))  // key is now String
        .join(format!("{label}-{checkout_key}"))
}
```

**Pattern:** Preserve existing path shape (repository-<key>, role-<checkout_key>), only swap key type. The sanitize function (lines 1786-1796) remains unchanged.

---

### `baude-core/src/lifecycle.rs` (service, CRUD)

**Analog:** `baude-core/src/lifecycle.rs` (existing ensure_repository function, lines 1307-1341)

**ensure_repository return type enhancement** (new enum for collision reporting):
```rust
// Addition: a new enum to report collisions non-destructively
#[derive(Debug, Clone)]
pub struct CollisionReport {
    pub path: PathBuf,
    pub owner_canonical_dir: String,
    pub newcomer_canonical_dir: String,
}

// Modified signature (still returns RepositoryKey on success, but now with optional collision):
// pub fn ensure_repository(
//     state: &mut RepositoryState,
//     snapshot: &RepositorySnapshot,
// ) -> Result<RepositoryKey, LifecycleError>;
//
// AFTER: Return type becomes Result<(RepositoryKey, Option<CollisionReport>), LifecycleError>
// So callers can handle collisions non-destructively.
```

**Collision detection insertion point** (after key allocation, before returning):
- After allocating new key and computing managed path (similar to lines 1320-1322)
- Check: does `managed_path` exist?
- If yes: read marker file from that path
- Compare marker's canonical_common_dir with newcomer's canonical_common_dir
- If mismatch: return Ok((key, Some(CollisionReport { ... })))
- If match: return Ok((key, None))

**Pattern:** Follows existing allocate-check-record pattern in ensure_repository (lines 1319-1330). Collision detection is informational, not fatal.

---

### `baude-core/src/launch.rs` (service, request-response)

**Analog:** `baude-core/src/launch.rs` (existing start_workspace function, Phase 13, lines 66-141)

**Marker adoption hook insertion** (new code added after lock claim, before binding record):
```rust
// Inside start_workspace function, after step 4 (lock claim), before step 5:
// 
// Step 4: Claim the workspace state lock.
// match persist::claim_workspace_state_lock(lock_base, workspace) {
//     Ok(()) => {
//         // NEW: Step 4a — Adopt markers for existing managed directories
//         if config.folder_context_enabled() {
//             let _ = adopt_markers(&workspace.name);  // Silently continue on error
//         }
//         
//         // Step 5: Record the binding...
//         if let Some(repo_root_path) = &repo_root {
//             ...
//         }
//     }
// }
```

**adopt_markers function** (new function to add to launch.rs):
```rust
/// Walk managed worktree directories and write markers for legacy (pre-digest) repos.
/// Silently succeeds on any error; markers are advisory and startup must not block on them.
fn adopt_markers(workspace_name: &str) -> Result<(), anyhow::Error> {
    let base = crate::git::worktrees_base().join(workspace_name);
    if !base.exists() {
        return Ok(());
    }
    
    for entry in std::fs::read_dir(&base)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() { continue; }
        
        let dir_name = entry.file_name();
        let dir_str = dir_name.to_string_lossy();
        
        // Only process repository-* directories
        if !dir_str.starts_with("repository-") { continue; }
        
        // Skip if marker already exists
        let marker_path = path.join(".baude-marker.json");
        if marker_path.exists() { continue; }
        
        // Discover owner from any primary-* or branch-* checkout
        if let Ok(common_dir) = discover_owner_from_checkouts(&path) {
            let metadata = crate::marker::MarkerMetadata::new(&common_dir);
            let _ = crate::marker::write_marker(&path, &metadata);
        }
    }
    
    Ok(())
}

fn discover_owner_from_checkouts(repo_dir: &Path) -> std::result::Result<std::path::PathBuf, anyhow::Error> {
    for entry in std::fs::read_dir(repo_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() { continue; }
        
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if !name_str.starts_with("primary-") && !name_str.contains('-') { continue; }
        
        if let Ok(identity) = crate::git::discover_repository_identity(&path) {
            return Ok(identity.common_dir);
        }
    }
    
    Err(anyhow::anyhow!("no readable checkout in {}", repo_dir.display()))
}
```

**Pattern:** Modeled on existing folder_workspace::plan_launch pattern in launch.rs (lines 76-82) — fail-closed directory walk, silently continue on errors.

---

### `baude-core/src/worktree_scan.rs` (service, CRUD)

**Analog:** `baude-core/src/worktree_scan.rs` (existing repository_key function, lines 1012-1015, and Candidate struct, lines 744-771)

**repository_key function update** (lines 1012-1015, modified to accept hex or decimal):
```rust
// BEFORE:
// fn repository_key(name: &str) -> Option<u64> {
//     let key: u64 = name.strip_prefix("repository-")?.parse().ok()?;
//     (format!("repository-{key}") == name).then_some(key)
// }

// AFTER: Accept both legacy decimal and new hex
fn repository_key(name: &str) -> Option<String> {
    let suffix = name.strip_prefix("repository-")?;
    
    // Try decimal first (legacy format)
    if let Ok(num) = suffix.parse::<u64>() {
        if format!("repository-{num}") == name {
            return Some(num.to_string());
        }
    }
    
    // Try hex (new format, 12 chars)
    if suffix.len() == 12 && suffix.chars().all(|c| c.is_ascii_hexdigit()) {
        if format!("repository-{suffix}") == name {
            return Some(suffix.to_string());
        }
    }
    
    None
}
```

**Candidate struct update** (lines 744-755, add owner field):
```rust
pub struct Candidate {
    pub relative: Vec<String>,
    pub workspace: String,
    pub repository_key: String,  // Changed from u64 to String (handles both legacy and hex)
    pub verdict: Verdict,
    pub owner: Option<OwnershipInfo>,  // NEW: ownership metadata from marker
}

// New struct for owner information:
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OwnershipInfo {
    pub canonical_common_dir: String,
    pub display_name: Option<String>,
}
```

**ScanReport struct update** (lines 788-799, add owner to JSON output):
```rust
pub struct ScanReport {
    pub format_version: u32,
    pub worktrees_base: crate::repository::PersistedPath,
    pub config_dir: crate::repository::PersistedPath,
    pub candidates: Vec<Candidate>,
    pub state_inventory: StateInventorySummary,
    // Format version remains 1; new field is optional in deserialization
}
```

**Pattern:** Follows existing Candidate and ScanReport design (serde with deny_unknown_fields, parallel evidence collection in verdict).

---

### `baude-core/src/lib.rs` (module config)

**Analog:** `baude-core/src/lib.rs` (existing module declarations, lines 8-25)

**Module export addition** (add between existing declarations):
```rust
pub mod marker;  // NEW: added in alphabetical order
```

**Pattern:** Follows existing pub mod declarations (lines 8-25). Placed alphabetically after `launch` and before `lifecycle`.

---

### `baude/src/app.rs` (controller, request-response)

**Analog:** `baude/src/app.rs` (existing admit_repository function, lines 1914-1947)

**admit_repository return type update** (lines 1914-1947, modified to surface collisions):
```rust
// BEFORE: pub fn admit_repository(&mut self, path: &Path) -> Result<Option<u64>>

// AFTER: Return collision report non-destructively
pub fn admit_repository(&mut self, path: &Path) -> Result<(Option<u64>, Option<CollisionReport>)> {
    let snapshot = git::discover_repository(path)?;
    let common = PersistedPath::from_path(&snapshot.common_dir);
    
    // Check state for existing entry
    let (repository_key, collision) = match self
        .repository_state
        .repositories
        .iter()
        .find(|repository| repository.observed_common_dir == common)
        .map(|repository| repository.key)
    {
        Some(key) => (key, None),
        None => {
            let key = self.repository_state.allocate_repository_key()?;
            let first_seen_order = self.repository_state.allocate_first_seen_order()?;
            
            // Call updated ensure_repository from lifecycle which handles collisions
            let (repo_key, collision_report) = crate::lifecycle::ensure_repository(
                &mut self.repository_state,
                &snapshot,
            )?;
            
            (repo_key, collision_report)
        }
    };
    
    // Record repository (existing logic)
    if let Some(repository) = self
        .repository_state
        .repositories
        .iter_mut()
        .find(|repository| repository.key == repository_key)
    {
        repository.observed_common_dir = common;
        repository.observed_main_worktree = PersistedPath::from_path(&snapshot.main_worktree);
        repository.health = RepositoryHealth::Available;
    }
    
    // Return key and optional collision
    Ok((Some(repository_key.get()), collision))
}
```

**Pattern:** Follows existing error propagation and state update pattern (lines 1914-1947). Collision is informational, not fatal.

---

### `baude/src/main.rs` (handler, request-response)

**Analog:** `baude/src/main.rs` (existing print_scan_summary and run_worktrees_scan functions, lines 736-995)

**Scan output formatting update** (add owner column to print_scan_summary):
```rust
// In print_scan_summary (lines 736+), update the candidate loop to include owner:
// Current loop (simplified):
//   for candidate in &report.candidates {
//       let workspace = &candidate.workspace;
//       let key = candidate.repository_key;
//       let verdict = verdict_label(&candidate.verdict);
//       writeln!(out, "  {} {} {}", workspace, key, verdict)?;
//   }
//
// UPDATED:
//   for candidate in &report.candidates {
//       let workspace = &candidate.workspace;
//       let key = &candidate.repository_key;
//       let owner = candidate.owner.as_ref()
//           .map(|o| format!("(owner: {})", &o.canonical_common_dir[..50.min(o.canonical_common_dir.len())]))
//           .unwrap_or_else(|| "".to_string());
//       let verdict = verdict_label(&candidate.verdict);
//       writeln!(out, "  {} {} {} {}", workspace, key, owner, verdict)?;
//   }
```

**JSON output pattern** (lines 974-990, already handles Serde, no changes needed):
```rust
if options.json {
    match serde_json::to_string_pretty(&report) {
        Ok(text) => {
            let _ = writeln!(out, "{text}");
            WORKTREES_EXIT_OK
        }
        Err(error) => {
            let _ = writeln!(
                err,
                "baude worktrees: the report could not be written: {error}"
            );
            WORKTREES_EXIT_FAILED
        }
    }
}
```

**Pattern:** Follows existing evidence_phrase and verdict_phrase helpers (lines 644-725). Add `owner_phrase()` helper for readability.

---

### `bauded/src/manager.rs` (service, request-response)

**Analog:** `bauded/src/manager.rs` (existing Manager structure and state handling, lines 77-90, 309+)

**Manager admission update** (modify state lock and repository admission sequence):
```rust
// In Manager::new (lines 309+), after loading state:
// Follow same pattern as app.rs:
// 
// let (repository_key, collision) = match crate::lifecycle::ensure_repository(
//     &mut state.state.repositories,
//     &snapshot,
// ) {
//     Ok((key, collision_opt)) => (key, collision_opt),
//     Err(e) => return Err(e),
// };
//
// If collision is Some(_), log it and continue
// (daemon does not have TUI status line, so surface in daemon log and /info)
```

**Manager state persistence** (lines 450-452, 465-467, no changes needed):
```rust
// Existing pattern (persist.rs handles state save, which includes all repositories):
persist::save_for_workspace_status(STATE_BASE, crate::workspace::active(), &state)
```

**Pattern:** Follows existing state load/save cycle in Manager (lines 449-467). Collision handling mirrors app.rs but without UI display.

---

### `README.md` (documentation)

**Analog:** `README.md` (existing Worktrees section, lines 325-342)

**Worktrees section update** (lines 325-342, add information about identity and markers):
```markdown
## Worktrees

Each admitted repository remains visible as a parent even when none of its
children has a running backend. Its main checkout, any separate managed
default checkout, and retained linked worktrees remain visible as durable
children in persisted oldest-first order.

From a local parent or child, `w` creates a valid local branch or activates an
eligible existing local branch in a managed path beneath
`~/.local/share/baude/worktrees/`, then starts the active workspace backend.

Managed worktree directories are identified by the canonical git common
directory (output of `git rev-parse --git-common-dir`), hashed to a stable
12-character hex digest. This ensures that two processes admitting the same
repository always derive the same path without coordination. Existing
counter-based directories from earlier baude versions are recognized in place;
the digest scheme applies only to new repositories.

baude does not fetch, guess a branch, or switch the main checkout. Lowercase
`x` closes only the runtime and keeps the checkout row for later reopening.
Uppercase `X` is a separate action: it is available only for a baude-managed
linked worktree, performs fresh topology and clean-state checks around an
exact-target confirmation, and uses ordinary non-destructive Git removal.
Dirty, conflicted, locked, submodule-unsafe, or indeterminate state blocks the
operation. A successful `X` removes only that worktree and child; its local
branch, repository parent, and siblings remain.

Path collisions (two repositories converging on the same managed directory) are
reported non-destructively: the newcomer receives a distinct digest-keyed
directory, and the collision is noted in `baude worktrees scan --json` output.
```

**Pattern:** Follows existing markdown style (full sentences, technical precision, action descriptions). Added digest explanation and collision note as optional context.

---

## Shared Patterns

### Atomic File Operations
**Source:** `baude-core/src/persist.rs` (lines 611-691, atomic_save_current)
**Apply to:** `marker::write_marker()` in new marker.rs
- Temp file with PID-based name to avoid collisions
- Write → flush → sync_all on temp
- Atomic rename (no clobber if exists)
- Directory sync on Unix for durability
- Error cleanup: drop and remove temp on failure
```rust
// Template from persist.rs:611-691
let temporary = dir.join(format!(".baude-marker.json.tmp-{}", std::process::id()));
let mut file = std::fs::File::create(&temporary)?;
file.write_all(&json)?;
file.sync_all()?;
drop(file);
std::fs::rename(&temporary, &marker_path)?;
#[cfg(unix)]
{
    let parent = marker_path.parent().ok_or_else(|| /* error */)?;
    std::fs::File::open(parent)?.sync_all()?;
}
```

### Serde JSON Serialization
**Source:** `baude-core/src/persist.rs` (lines 18-23, StateFile, and lines 624, serde_json::to_vec_pretty)
**Apply to:** `marker::MarkerMetadata` in new marker.rs, and `baude-core/src/worktree_scan.rs` Candidate owner field
- Use #[derive(Serialize, Deserialize)]
- Add #[serde(deny_unknown_fields)] for forward compatibility
- Serialize with serde_json::to_vec_pretty for readability
- Deserialize with serde_json::from_slice for flexibility

### Path Canonicalization and Validation
**Source:** `baude-core/src/git.rs` (lines 330-353, discover_repository_identity)
**Apply to:** `marker::read_marker()` and collision detection
- Canonicalize paths before comparing identity
- Never use string comparisons of non-canonical paths
- Fail-closed on canonicalization errors (treat as unknown/different)

### Fail-Closed Error Handling
**Source:** `baude-core/src/persist.rs` (error handling throughout) and `baude-core/src/launch.rs` (lines 104-140)
**Apply to:** All new admission paths and marker operations
- Missing marker → Ok(None) not Err
- Collision detection → informational, not fatal
- Marker adoption at startup → silently continue on errors
- Never escalate advisory operations into hard failures

### Testable Fixture Isolation
**Source:** `baude-core/src/testing.rs` (TestRedirect pattern)
**Apply to:** All new marker tests and collision detection tests
- Use TestRedirect to isolate filesystem operations
- All paths resolve through testing::worktrees_base_override()
- Escape guard aborts tests reaching real home

---

## No Analog Found

All files have analogs in the existing codebase. This phase extends existing patterns without introducing fundamentally new patterns.

---

## Metadata

**Analog search scope:** baude-core/src/*.rs, baude/src/*.rs, bauded/src/*.rs
**Files scanned:** 21 Rust source files
**Pattern extraction date:** 2026-09-20

**Key findings:**
- All 11 files are modifications or additions to existing modules with proven patterns
- Atomic file I/O pattern from persist.rs is production-tested and directly reusable
- Repository identity handling from git.rs and lifecycle.rs is stable and well-integrated
- Scan output from worktree_scan.rs is already serde-serializable; owner field is additive
- Collision handling as informational output (not fatal) matches baude's fail-closed philosophy
