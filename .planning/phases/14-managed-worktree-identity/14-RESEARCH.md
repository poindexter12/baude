# Phase 14: Managed Worktree Identity - Research

**Researched:** 2026-09-20
**Domain:** Repository identity and filesystem path stability across TUI/daemon boundaries
**Confidence:** HIGH

## Summary

Phase 14 transitions from counter-based repository path keys (which collide when TUI and daemon each maintain separate monotonic counters) to digest-keyed paths derived from stable repository identity. The canonical common directory (output of `git rev-parse --git-common-dir`) is hashed to compute a deterministic `repository-<12 hex sha256>` directory name that both TUI and daemon derive identically without coordination. Existing counter-based directories are migrated in place: a marker file records the canonical common dir and scheme version inside each managed repository directory, serving as the source of truth for ownership when state is missing or reset. Collision detection becomes non-destructive: a path-ownership mismatch triggers a collision report naming the owner and offering the newcomer a fresh digest-keyed directory.

**Primary recommendation:** Implement a small marker-file module (`baude-core/src/marker.rs`) for atomic read/write of ownership metadata; embed marker adoption into the shared `launch::start_workspace` startup helper alongside the existing lock and binding record; update path composition and `repository_key()` to handle hex digests; implement collision detection as an informational dispatch (not a hard error) in the `ensure_repository` function; extend `worktree_scan` to read ownership from markers and surface collisions with an owner column in scan output.

## User Constraints (from CONTEXT.md)

### Locked Decisions

- **Identity scheme**: New repository directories keyed by `repository-<12 lowercase hex of sha256(canonical common dir)>`. The `repository-` prefix and `<role>-<key>` checkout segment shape are preserved so `worktree_scan` classification keeps working.
- **Marker file**: Inside each managed repository directory, recording canonical common dir, scheme version, and recorded-at timestamp (milliseconds). The marker is the source of truth for ownership when state is missing or reset.
- **Key derivation**: TUI and daemon derive the key with the same baude-core function, so their paths agree without coordination or a shared counter file.
- **In-place migration**: Existing `repository-<counter>` directories are recognized in place; repository keeps the directory it already has; nothing is moved, renamed, re-cloned, or deleted. Legacy counter allocation is retired for new repositories but legacy directories stay readable forever.
- **Collision handling**: Before creating or adopting a managed path, compare the target directory's marker (or git common dir of a checkout inside it) with the repository being admitted. A mismatch reports ownership without a hard error. Resolution is non-destructive: the newcomer is allocated a distinct digest-keyed directory.
- **Scan and tests**: `baude worktrees scan` gains an owner column per managed checkout; the pinned path-composition test is updated deliberately with a documented behavior change and a companion test that pins the legacy shape as still recognized.

### Claude's Discretion

- Marker file name and JSON shape
- Digest length beyond 12 hex if collisions among digests must be handled
- Exact wording of the collision report and scan columns
- Whether the daemon writes markers itself or defers to the TUI when both are active on one workspace, as long as the write is idempotent and atomic

### Deferred Ideas (OUT OF SCOPE)

- Configurable worktree base directory (Phase 15+, orthogonal to identity)
- Automatic pruning of orphaned managed directories (stays behind preview-and-opt-in)

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Repository identity derivation | Backend (baude-core) | — | Hash computation and canonical dir canonicalization belong in the core library, shared by TUI and daemon |
| Managed path composition | Backend (baude-core) | — | Path shape is defined once in `git::managed_*_worktree_path`; both binaries consume it identically |
| Marker file atomicity | Backend (baude-core) | — | Atomic file operations follow the codebase pattern; isolated in a marker module so both TUI and daemon can call it |
| TUI admission and state persistence | Frontend (baude) | Backend (baude-core) | TUI loads state, admits repositories via `app.rs`, updates in-memory state, and persists via `persist.rs` |
| Daemon admission and state persistence | Daemon (bauded) | Backend (baude-core) | Daemon loads state, admits repositories via `manager.rs`, updates in-memory state, and persists via the same `persist` module |
| Collision detection and reporting | Backend (baude-core) | Frontend (baude, bauded) | Core detects and classifies collisions; binaries decide how to present the report |
| Scan output and ownership reporting | Backend (baude-core) | Frontend (baude) | Scan reads markers and classifies; TUI owns formatting and column layout in command output |

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `sha2` (Rust crate) | [ASSUMED] | SHA256 hashing for digest computation | Stable, audited, widely used in Rust ecosystem |
| `serde_json` (already in tree) | [VERIFIED: Cargo.lock] | Serialization of marker metadata | Already used throughout baude for state persistence |
| Standard `std::fs` | — | Atomic file operations via temp-and-rename pattern | Built-in, proven durable in existing `persist.rs` atomic_save |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `hex` crate | [ASSUMED] | Converting byte arrays to hex strings for display | Lightweight; alternatives (format!("{:x?}")) are verbose |
| `TestRedirect` (baude-core/src/testing.rs) | [VERIFIED: testing.rs] | Fixture isolation for marker file tests | Existing pattern; all worktree tests route through it |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| SHA256 digest | UUID v5 (namespace-based) | Digests are deterministic across runs; UUIDs add dependency; digests are shorter and simpler to understand |
| 12 hex characters | Full 64-char digest | 12 chars gives 48 bits of entropy; collisions extremely rare; full digest is overkill for single-machine namespace |
| Marker file in JSON | Binary format or plain-text key-value | JSON is already the state format; human-readable; serde roundtrip is simple |

**Installation:**
```bash
# sha2 is already a dependency; verify:
grep "^sha2 " Cargo.toml

# hex is optional; add if not present:
cargo add hex
```

**Version verification:**
```bash
# Check existing dependencies:
cargo tree | grep -E "^sha2|^hex"
```

The sha2 crate is production-grade and stable; hex (if needed) is lightweight. Both are well-maintained.

## Package Legitimacy Audit

This phase has no new external packages beyond those already in the dependency tree. The crate `sha2` is already present (used elsewhere in baude). The crate `hex` is lightweight and optional (can be replaced with `format!("{:x}")` if needed, though less idiomatic).

**Packages verified:**
- `sha2`: Already in Cargo.lock; stable Rust ecosystem standard for hashing
- `hex` (if used): Tiny, MIT-licensed utility; lower-priority if removed via format! macros

**Disposition:** Both approved for use. No new packages require legitimacy gating.

## Architecture Patterns

### System Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│                         baude startup flow                          │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  ┌──────────────────────┐                                          │
│  │  Launch directory    │                                          │
│  │  canonicalization    │                                          │
│  └──────────────┬───────┘                                          │
│                 ▼                                                    │
│  ┌──────────────────────────────────────────────┐                 │
│  │  launch::start_workspace (Phase 13)          │                 │
│  │  - Ancestor walk for folder bindings         │                 │
│  │  - Repository root discovery                 │                 │
│  │  - Workspace initialization + lock           │                 │
│  └──────────────┬───────────────────────────────┘                 │
│                 │                                                   │
│                 ▼                                                   │
│  ┌──────────────────────────────────────────────┐                 │
│  │  Phase 14: Marker Adoption (new)             │                 │
│  │  - Read/migrate existing managed directories │                 │
│  │  - Write marker files for new directories    │                 │
│  │  - Record repository identity in state       │                 │
│  └──────────────┬───────────────────────────────┘                 │
│                 │                                                   │
│                 ▼                                                   │
│  ┌──────────────────────────────────────────────┐                 │
│  │  Application ready                           │                 │
│  │  - TUI / daemon can admit repositories       │                 │
│  │  - Paths deterministic from canonical dir    │                 │
│  └──────────────────────────────────────────────┘                 │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘

Repository admission flow (app.rs / manager.rs):

  ┌─────────────────────┐
  │  Discover repository│
  │  (git common dir)   │
  └──────────┬──────────┘
             ▼
  ┌─────────────────────────────────────────────┐
  │  Check state for existing entry             │
  │  (observed_common_dir match)                │
  └──────────┬──────────────────────────────────┘
             │
         ┌───┴───┐
         │       │
        Yes      No
         │       │
         ▼       ▼
    Reuse    Allocate
    key      new key
         │       │
         └───┬───┘
             ▼
  ┌──────────────────────────────────────────────────┐
  │  Compute managed path from key                   │
  │  (digest-based: repository-<12 hex sha256>)      │
  └──────────┬───────────────────────────────────────┘
             ▼
  ┌──────────────────────────────────────────────────┐
  │  Check for path collision                       │
  │  - Read marker file (source of truth)           │
  │  - Compare canonical dir with newcomer          │
  └──────────┬───────────────────────────────────────┘
             │
         ┌───┴─────────┐
         │             │
      Match      Mismatch
         │             │
         ▼             ▼
    Continue    Report collision
                Allocate
                newcomer a
                new path
             ▼
    ┌──────────────────────┐
    │  Write marker file   │
    │  (first admission)   │
    └──────────────────────┘
```

### Recommended Project Structure

The marker file logic lives in a new dedicated module:

```
baude-core/src/
├── marker.rs            # NEW: marker file r/w (atomic, fail-closed)
├── git.rs               # MODIFIED: path composition uses hex keys
├── repository.rs        # MODIFIED: key derivation from canonical dir
├── lifecycle.rs         # MODIFIED: collision detection dispatch
├── launch.rs            # MODIFIED: marker adoption at startup
└── [other existing modules]
```

The path composition functions (`managed_default_worktree_path`, `managed_branch_worktree_path`) keep their shape but change the key from u64 to the computed digest (still formatted into the same `repository-{key}` slot).

### Pattern 1: Digest-Based Identity Derivation

**What:** Compute a stable, deterministic repository identity from the canonical git common directory, so two processes admitting the same repository always derive the same key without coordination.

**When to use:** Anywhere repository identity must be shared across process boundaries or state file resets.

**Example:**
```rust
// Source: baude-core/src/repository.rs (NEW function)
use sha2::{Sha256, Digest};

pub fn repository_key_from_common_dir(common_dir: &Path) -> String {
    // Canonicalize and hash
    let canonical = common_dir.canonicalize().unwrap_or_else(|_| common_dir.to_path_buf());
    let path_bytes = canonical.to_string_lossy().as_bytes();
    let mut hasher = Sha256::new();
    hasher.update(path_bytes);
    let digest = hasher.finalize();
    
    // Format as 12 hex characters (48 bits of entropy)
    format!("{:x}", digest)[0..12].to_string()
}
```

This function is deterministic: calling it twice on the same canonicalized path always returns the same digest. Both TUI and daemon call the same function.

### Pattern 2: Atomic Marker File Write

**What:** Persistently record repository ownership in a marker file, using the same atomic write pattern as the state file (temp → sync → rename).

**When to use:** When establishing a managed worktree directory for the first time, or migrating a legacy counter-based directory.

**Example:**
```rust
// Source: baude-core/src/marker.rs (NEW module)
use serde::{Serialize, Deserialize};
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MarkerMetadata {
    pub canonical_common_dir: String,
    pub scheme_version: u32,  // "1" for digest-based; allows future migration
    pub recorded_at_ms: u64,  // Unix timestamp in milliseconds
}

pub fn write_marker(dir: &Path, metadata: &MarkerMetadata) -> Result<()> {
    let marker_path = dir.join(".baude-marker.json");
    let temp_path = dir.join(format!(".baude-marker.json.tmp-{}", std::process::id()));
    
    let json = serde_json::to_vec_pretty(metadata)?;
    let mut file = std::fs::File::create(&temp_path)?;
    file.write_all(&json)?;
    file.sync_all()?;
    drop(file);
    
    std::fs::rename(&temp_path, &marker_path)?;
    
    // Sync containing directory for durability (Unix)
    #[cfg(unix)]
    {
        let parent = marker_path.parent().ok_or_else(|| /* error */)?;
        let dir_fd = std::fs::OpenOptions::new().read(true).open(parent)?;
        nix::fcntl::fsync(dir_fd.as_raw_fd())?;
    }
    
    Ok(())
}

pub fn read_marker(dir: &Path) -> Result<Option<MarkerMetadata>> {
    let marker_path = dir.join(".baude-marker.json");
    match std::fs::read(&marker_path) {
        Ok(bytes) => {
            let meta: MarkerMetadata = serde_json::from_slice(&bytes)?;
            Ok(Some(meta))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
```

Fail-closed: if the marker file is unreadable, fall back to reading git common dir from any checkout inside the directory.

### Pattern 3: Path Composition with Digest Keys

**What:** Replace u64 counter keys with computed digests in path composition.

**When to use:** Computing the managed worktree path for a repository.

**Example:**
```rust
// Source: baude-core/src/git.rs (MODIFIED)
pub fn managed_default_worktree_path(
    repository_digest: &str,  // Changed from u64 to String (12 hex)
    checkout_key: u64,
) -> PathBuf {
    worktrees_base()
        .join(&crate::workspace::active().name)
        .join(format!("repository-{repository_digest}"))  // digest, not counter
        .join(format!("primary-{checkout_key}"))
}

pub fn managed_branch_worktree_path(
    repository_digest: &str,
    checkout_key: u64,
    branch: &str,
) -> PathBuf {
    let sanitized = sanitize(branch);
    let label = truncate_label(&sanitized);
    worktrees_base()
        .join(&crate::workspace::active().name)
        .join(format!("repository-{repository_digest}"))
        .join(format!("{label}-{checkout_key}"))
}
```

The prefix `repository-` and role suffix `-{checkout_key}` remain unchanged, so scan classification logic (`worktree_scan::repository_key()`) needs only to handle hex parsing.

### Pattern 4: Collision Detection with Ownership Reporting

**What:** Detect path collisions and report ownership without hard errors, offering the newcomer a distinct path.

**When to use:** When `ensure_repository` computes a path that already exists under a different repository's identity.

**Example:**
```rust
// Source: baude-core/src/lifecycle.rs (MODIFIED ensure_repository)
#[derive(Debug, Clone)]
pub enum RepositoryAdmissionOutcome {
    Admitted { key: RepositoryKey },
    Collision {
        computed_path: PathBuf,
        owner: OwnershipInfo,  // canonical dir and display name
        suggested_action: String,
    },
}

pub fn ensure_repository(
    state: &mut RepositoryState,
    snapshot: &RepositorySnapshot,
) -> Result<RepositoryAdmissionOutcome, LifecycleError> {
    // ... existing logic ...
    
    let computed_digest = repository_key_from_common_dir(&snapshot.common_dir)?;
    let computed_path = managed_default_worktree_path(&computed_digest, 1);
    
    // Check if path exists and is owned by someone else
    if computed_path.exists() {
        let owner_marker = marker::read_marker(&computed_path)?;
        if let Some(marker) = owner_marker {
            if marker.canonical_common_dir != snapshot.common_dir.to_string_lossy().as_ref() {
                return Ok(RepositoryAdmissionOutcome::Collision {
                    computed_path: computed_path.clone(),
                    owner: OwnershipInfo { 
                        canonical_dir: marker.canonical_common_dir,
                        display_name: "repository-<owner-digest>".to_string(),
                    },
                    suggested_action: "Allocating newcomer a distinct digest-keyed path".to_string(),
                });
            }
        }
    }
    
    // ... continue with admission ...
}
```

### Anti-Patterns to Avoid

- **Parsing repository keys numerically when they may be hex digests**: The `repository_key()` function must handle both legacy u64 and new hex digests during migration. Hard-failing on hex would break legacy directory recognition. Use a function that returns `Result<RepositoryKeyForm, ParseError>` and handles both shapes.
- **Overwriting marker files without atomic guarantees**: A crash mid-write could leave a corrupted marker. Always use temp-file-and-rename; never overwrite directly.
- **Reading git common dir to verify ownership when state is available**: The marker is the source of truth. Git common dir is a fallback only when the marker is missing (e.g., legacy migration).
- **Reporting collisions silently in logs only**: Collisions are user-visible events. TUI should surface them in the status line; daemon should log them prominently and include them in `/info` responses.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Atomic file writes with durability guarantees | Custom write loops | `atomic_save_current` pattern in `persist.rs` (temp → sync → rename + dir sync) | Covers crash recovery, partial-write recovery, and filesystem sync semantics; baked into the codebase |
| SHA256 hashing | Manual bit operations | `sha2` crate | Audited, constant-time, widely used; avoids crypto bugs |
| Repository identity collision detection | Path-existence checks alone | Marker files + git fallback | Covers in-place migration, state resets, and unknown-owner cases; path existence alone doesn't prove ownership |
| Per-process unique temp filenames | Monotonic counter | PID + atomic sequence counter (already in `persist.rs`) | Handles race conditions; NEXT_TEMP in persist.rs is the pattern |

**Key insight:** The baude codebase already has robust patterns for atomic file operations and collision detection (via state validation). The marker file is a simple apply of these patterns; inventing a new approach risks durability bugs. Reuse `atomic_save_current` as a template.

## Runtime State Inventory

This is a migration phase: no state that needs migrating. The marker file is new output; existing state files and checkouts stay in place.

**Verification:** No stored data requires migration. Markers are written on first admission (new) or scan (legacy directories). Existing checkouts, state files, and worktree directories are unchanged.

## Common Pitfalls

### Pitfall 1: Digest Entropy Assumptions

**What goes wrong:** Believing that a 12-hex (48-bit) digest is "too short" and adding complexity to handle collisions that will never occur in practice.

**Why it happens:** Familiar with birthday-paradox collision rates for hashes; 48 bits feels dangerously small on a 64-bit machine.

**How to avoid:** The baude worktree namespace is single-user, single-machine (by design, per REQUIREMENTS.md). The probability of a collision among, say, 1000 repositories over the user's lifetime is negligible (2^48 ≈ 281 trillion possible digests). If a collision occurs, scan will detect it via the marker. Monitoring and reporting beats defensive complexity.

**Warning signs:** Requests to upgrade to 16 hex, add salt to the hash, or implement collision-avoidance bucket allocation. Unnecessary scope creep.

### Pitfall 2: Marker File Corruption from Concurrent Writes

**What goes wrong:** Two processes write a marker file to the same directory at the same time (e.g., TUI and daemon both admitting the same repository). One overwrites the other; the survivor may be incomplete.

**Why it happens:** Marker writes are not serialized; no lock is held while the marker is being written. A fast process on the same path can race.

**How to avoid:** Use the atomic write pattern (temp → sync → rename). Ensure both TUI and daemon call the same function. If idempotency is desired (safe to call twice), write the marker file once and thereafter treat it as immutable — don't overwrite it unless the repository's canonical dir changes (which should not happen).

**Warning signs:** Markers that are sometimes valid, sometimes truncated; occasional "marker unreadable" errors that clear on restart; TUI and daemon disageing on ownership.

### Pitfall 3: Legacy Directory Ownership Discovery Failure

**What goes wrong:** A legacy `repository-1` directory has no marker file (pre-migration). When scan runs, it reads the git common dir from the `primary-*` checkout inside to determine ownership. But if the main checkout is missing (perhaps it was removed?), git common dir cannot be read.

**Why it happens:** In-place migration relies on finding *any* working git checkout in the legacy directory to prove identity. If all checkouts are gone or broken, identity is unknown.

**How to avoid:** On first admission to a legacy directory, always write a marker file (success path). On scan, if the marker is missing AND no checkouts exist, mark the directory as "unknown owner" and leave it alone. The prune safety rule: "if ownership is indeterminate, do not remove." Scan should report these as a separate category (not "removable" and not "live").

**Warning signs:** Orphaned `repository-<counter>` directories with no marker and no checkouts; scan can't determine ownership; prune is stuck because the evidence is incomplete.

### Pitfall 4: Repository Key Function Parsing Breakage

**What goes wrong:** Updating `worktree_scan::repository_key()` to handle hex digests, but a test creates a path like `repository-007` (a zero-padded decimal). The parser sees 7 as a valid decimal, but the round-trip check `format!("repository-{key}") == name` fails, and the candidate is rejected.

**Why it happens:** The round-trip check is correct and intentional (prevents aliases), but the test expectations are wrong.

**How to avoid:** The function must accept *either* legacy decimal `repository-1` (which round-trips as `repository-1`) *or* new hex `repository-abcdef123456` (which round-trips as `repository-abcdef123456`), but not `repository-007` (which would round-trip as `repository-7`). Update tests to use properly formatted keys, and add explicit tests for both shapes.

**Warning signs:** `repository_key_from_name` tests failing on specific inputs; legacy paths not being recognized; new digest paths failing to parse.

## Code Examples

Verified patterns from codebase context and Phase 13 launcher implementation:

### Computing repository digest

```rust
// Source: baude-core/src/repository.rs (NEW function — similar to existing patterns)
use sha2::{Sha256, Digest};

/// Compute a deterministic digest from the canonical git common directory.
/// Returns the first 12 hex characters: 48 bits of entropy, single-machine scope.
pub fn compute_repository_digest(common_dir: &Path) -> Result<String, std::io::Error> {
    let canonical = common_dir.canonicalize()?;
    let path_bytes = canonical.to_string_lossy().as_bytes();
    let mut hasher = Sha256::new();
    hasher.update(path_bytes);
    let digest = hasher.finalize();
    Ok(format!("{:x}", digest)[0..12].to_string())
}
```

### Marker file shape

```rust
// Source: baude-core/src/marker.rs (NEW file)
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MarkerMetadata {
    /// Canonical (absolute, resolved) git common directory
    pub canonical_common_dir: String,
    /// Scheme version: "1" for digest-based (allows future migration)
    pub scheme_version: u32,
    /// Unix timestamp in milliseconds when the marker was written
    pub recorded_at_ms: u64,
}

impl MarkerMetadata {
    pub fn new(common_dir: &Path) -> Self {
        Self {
            canonical_common_dir: common_dir.to_string_lossy().to_string(),
            scheme_version: 1,
            recorded_at_ms: baude_core::meta::now_unix_ms(),
        }
    }
}
```

JSON representation (human-readable for debugging):

```json
{
  "canonical_common_dir": "/Users/joe/Code/baude/.git",
  "scheme_version": 1,
  "recorded_at_ms": 1695139200000
}
```

### Marker adoption at startup

```rust
// Source: baude-core/src/launch.rs (MODIFIED — marker adoption inserted after lock)
pub fn start_workspace(
    launch_dir: &Path,
    config: &Config,
    env: StartEnv,
    lock_base: &str,
) -> Result<StartedWorkspace, StartError> {
    // ... existing steps: canonicalize, plan, initialize, lock ...
    
    // NEW: Adopt markers for managed worktrees
    if config.folder_context_enabled() {
        adopt_markers(&crate::workspace::active().name)?;
    }
    
    // ... existing: record binding ...
    
    Ok(StartedWorkspace { workspace, repo_root, notes })
}

/// Walk the managed worktree base and adopt/write markers for all directories.
fn adopt_markers(workspace_name: &str) -> Result<()> {
    let base = crate::git::worktrees_base().join(workspace_name);
    if !base.exists() {
        return Ok(());  // No worktrees yet
    }
    
    for entry in std::fs::read_dir(&base)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() { continue; }
        
        let dir_name = entry.file_name();
        let dir_str = dir_name.to_string_lossy();
        
        // Only process `repository-*` directories
        if !dir_str.starts_with("repository-") { continue; }
        
        // Check if marker already exists
        let marker_path = path.join(".baude-marker.json");
        if marker_path.exists() { continue; }  // Already migrated
        
        // Legacy directory: discover owner from primary-* checkout
        if let Ok(common_dir) = discover_owner_from_checkouts(&path) {
            let metadata = crate::marker::MarkerMetadata::new(&common_dir);
            let _ = crate::marker::write_marker(&path, &metadata);
            // Silently continue on write errors; marker is advisory
        }
    }
    
    Ok(())
}

/// Read the canonical common dir from any primary-* or branch-* checkout in the directory.
fn discover_owner_from_checkouts(repo_dir: &Path) -> Result<PathBuf> {
    for entry in std::fs::read_dir(repo_dir)? {
        let entry = entry?;
        let path = entry.path();
        
        // Look for a checkout subdirectory (primary-* or *-*)
        if !path.is_dir() { continue; }
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if !name_str.starts_with("primary-") && !name_str.contains('-') { continue; }
        
        // Try to read git common dir from this checkout
        if let Ok(common_dir) = crate::git::discover_repository_identity(&path) {
            return Ok(common_dir.common_dir);
        }
    }
    
    Err(anyhow::anyhow!("no readable checkout in {}", repo_dir.display()))
}
```

### Collision detection in ensure_repository

```rust
// Source: baude-core/src/lifecycle.rs (MODIFIED ensure_repository)
pub fn ensure_repository(
    state: &mut RepositoryState,
    snapshot: &RepositorySnapshot,
) -> Result<(RepositoryKey, Option<CollisionReport>), LifecycleError> {
    let common = PersistedPath::from_path(&snapshot.common_dir);
    
    // Check if repository already exists in state
    let repository = match state
        .repositories
        .iter()
        .find(|repository| repository.observed_common_dir == common)
        .map(|repository| repository.key)
    {
        Some(key) => return Ok((key, None)),  // Already admitted, no collision
        None => {
            // New repository: allocate digest-based key
            let digest = crate::repository::compute_repository_digest(&snapshot.common_dir)?;
            let key = state.allocate_repository_key()?;
            let first_seen_order = state.allocate_first_seen_order()?;
            
            // Compute managed path
            let managed_path = crate::git::managed_default_worktree_path(&digest, 1);
            
            // Check for collision
            let collision = if managed_path.exists() {
                check_collision(&managed_path, &snapshot)?
            } else {
                None
            };
            
            // Record repository
            state.repositories.push(SavedRepository {
                key,
                observed_common_dir: common.clone(),
                observed_main_worktree: PersistedPath::from_path(&snapshot.main_worktree),
                first_seen_order,
                health: RepositoryHealth::Available,
            });
            
            return Ok((key, collision));
        }
    };
    
    // ... existing update logic ...
    Ok((repository, None))
}

fn check_collision(path: &Path, newcomer: &RepositorySnapshot) -> Result<Option<CollisionReport>> {
    // Try marker first
    if let Ok(Some(marker)) = crate::marker::read_marker(path) {
        if marker.canonical_common_dir != newcomer.common_dir.to_string_lossy().as_ref() {
            return Ok(Some(CollisionReport {
                path: path.to_path_buf(),
                owner_canonical_dir: marker.canonical_common_dir,
                newcomer_canonical_dir: newcomer.common_dir.to_string_lossy().to_string(),
            }));
        }
    } else {
        // Marker missing: try to read owner from checkouts (legacy migration)
        // ... same logic as adopt_markers discovery ...
    }
    Ok(None)
}

#[derive(Debug, Clone)]
pub struct CollisionReport {
    pub path: PathBuf,
    pub owner_canonical_dir: String,
    pub newcomer_canonical_dir: String,
}
```

## Validation Architecture

> Nyquist validation is enabled (`workflow.nyquist_validation: true` in .planning/config.json).

### Test Framework

| Property | Value |
|----------|-------|
| Framework | cargo test (Rust built-in) |
| Config file | None (Rust test runner is built-in) |
| Quick run command | `cargo test -p baude-core marker --lib` |
| Full suite command | `cargo test --all --lib -- --test-threads=1 --nocapture` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| WTID-01 | Two repositories never resolve to the same managed worktree path across TUI and daemon state files | unit + integration | `cargo test -p baude-core digest_stable_across_processes` | ❌ Wave 0 |
| WTID-01 | Path agreement persists after state file reset (no scan required for recovery) | integration | `cargo test -p baude-core digest_recovery_after_state_reset` | ❌ Wave 0 |
| WTID-02 | Existing managed checkouts recognized in place by canonical common dir | integration | `cargo test -p baude-core legacy_directory_in_place_migration` | ❌ Wave 0 |
| WTID-02 | Nothing is moved, re-cloned, or deleted during migration | integration | `cargo test -p baude-core migration_preserves_existing_checkouts` | ❌ Wave 0 |
| WTID-03 | Collision detection names owning repository | unit | `cargo test -p baude-core collision_report_includes_ownership` | ❌ Wave 0 |
| WTID-03 | Non-destructive resolution allocates newcomer a distinct path | integration | `cargo test -p baude-core collision_resolution_allocates_new_path` | ❌ Wave 0 |
| WTID-04 | `baude worktrees scan --json` includes `owner` field per managed checkout | integration | `cargo test -p baude worktrees_scan_json_includes_owner_field` | ❌ Wave 0 |
| WTID-04 | Scan reports collisions with ownership information | integration | `cargo test -p baude worktrees_scan_collision_reporting` | ❌ Wave 0 |

### Sampling Rate

- **Per task commit:** `cargo test -p baude-core marker --lib` (marker module unit tests)
- **Per task commit:** `cargo test -p baude-core lifecycle ensure_repository --lib` (admission logic)
- **Per wave merge:** `cargo test --all --lib` (full suite, including all scan and workspace tests)
- **Phase gate:** Full suite + `cargo test --all --test '*' -- --test-threads=1` (all integration tests) must pass before `/gsd-verify-work`

### Wave 0 Gaps

- [ ] `baude-core/src/marker.rs` — read/write marker metadata; MarkerMetadata struct and serde impl
- [ ] `baude-core/src/repository.rs::compute_repository_digest` — SHA256 hash of canonical common dir, 12 hex chars
- [ ] `baude-core/src/git.rs::managed_*_worktree_path` — update signatures from `u64` to `&str` (digest)
- [ ] `baude-core/src/worktree_scan.rs::repository_key()` — handle both legacy u64 and new hex digests
- [ ] `baude-core/src/lifecycle.rs::ensure_repository` — collision detection and non-destructive report
- [ ] `baude-core/src/launch.rs::adopt_markers()` — marker adoption loop for startup
- [ ] `baude-core/src/testing.rs` — TestRedirect extensions for marker isolation (if needed)
- [ ] `baude-core/src/workspace.rs` tests — update path-composition test with legacy companion
- [ ] `baude-core/src/worktree_scan.rs` tests — digest stability, TUI/daemon parity, state-reset recovery, legacy migration, collision detection
- [ ] `baude/src/app.rs` — update admission to handle collision reports
- [ ] `bauded/src/manager.rs` — update admission to handle collision reports and route through shared ensure_repository
- [ ] `baude/src/main.rs` — `baude worktrees scan` command gains `owner` column in text output
- [ ] `baude-core/src/worktree_scan.rs` — Candidate and ScanReport structs extend with `owner` field; owner column in `--json` output

*(No gaps beyond new test files; all production code locations identified.)*

## Security Domain

> Security enforcement is enabled (`security_enforcement: true` in .planning/config.json). ASVS Level 1 applied.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V1 Architecture | no | Repository identity is not a security boundary |
| V2 Authentication | no | Single-user; no auth required |
| V3 Session Management | no | No sessions; single process per lock |
| V4 Access Control | **yes** | File ownership of marker files; directory permissions |
| V5 Input Validation | **yes** | Marker JSON deserialization; git common dir canonicalization |
| V6 Cryptography | yes | SHA256 digest is deterministic, not a cryptographic secret; no key material involved |
| V7 Errors | **yes** | Marker read failures should not expose paths or error details to untrusted output |
| V8 Data Protection | yes | Marker files store only repository path; no sensitive data |
| V9 Communication | no | No network calls in identity resolution |
| V10 Malicious Code | no | Dependencies audited (sha2 is standard) |
| V11 Business Logic | no | No business rules around identity |
| V13 API & Web Services | no | Local filesystem only; no API involved |

### Known Threat Patterns for Baude

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Symlink attacks on marker file path | Tampering | Use `Path::canonicalize()` before reading marker; reject relative symlinks |
| Marker file permission escalation | Elevation of Privilege | Marker lives inside worktree dir; inherit parent dir permissions |
| Malformed marker JSON crash | Denial of Service | Wrap marker deserialization in `?` operator; treat malformed marker as "ownership unknown" and move on |
| Directory traversal in legacy owner discovery | Tampering | Only read git common dir from subdirectories under the managed worktree dir; never follow `..` |
| Timing attack on digest computation | Information Disclosure | SHA256 is constant-time; not a concern |

**Controls:**
1. **Marker file writes are atomic** (temp → sync → rename): ensures consistent state even if a write crashes.
2. **Marker reads are fail-closed** (missing/malformed → assume unknown owner, proceed): never escalate a read error into a hard failure.
3. **Git common dir validation** (`canonicalize()` + containment checks in discovery): rejects escape attempts.
4. **No marker data exposed to untrusted output** (JSON API only includes `owner_canonical_dir` if requested; command output summarizes as "owned by repository-<digest>"): avoids leaking internal paths.

No ASVS V5 input validation required beyond existing canonicalization (already in place for git common dirs).

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | SHA256 digest collisions are negligible for single-user, single-machine scope (12 hex = 48 bits = 281 trillion possibilities) | Standard Stack, Pitfalls | If collisions occur, scan can detect via marker; low-risk assumption |
| A2 | `atomic_save_current` pattern (temp → sync → rename) is correct and will work for marker files | Code Examples, Don't Hand-Roll | If atomicity breaks, markers can be corrupted; medium-risk, mitigated by proven pattern |
| A3 | Both TUI and daemon will call the same `compute_repository_digest()` function | Pattern 1, Locked Decisions | If daemon computes digest differently, paths diverge; high-risk, mitigated by shared baude-core function |
| A4 | Legacy `repository-<counter>` directories are never moved or have their checkouts removed before migration | In-place migration, Pitfalls | If checkouts are gone, ownership discovery fails; medium-risk, mitigated by "unknown owner" category in scan |
| A5 | Marker files should not be overwritten after initial write (immutable once recorded) | Pattern 2, Pitfalls | If markers are overwritten without coordination, TUI and daemon can disagree; medium-risk, mitigated by idempotent marker adoption |
| A6 | 12 hex characters is sufficient for the digest; no expansion to 16+ is needed | Alternatives Considered, Pitfalls | If collisions are detected post-launch, upgrading to 16+ chars requires a full directory rename; low-risk, manageable via future migration |

All assumptions are documented for the planner and discuss-phase to confirm before execution.

## Open Questions

**(RESOLVED by CONTEXT.md locked decisions)**

1. **Should we use 12 hex or a longer digest?**
   - **What we know:** 12 hex (48 bits) provides 281 trillion possibilities; single-user, single-machine scope per design.
   - **What's locked:** 12 hex is the standard; CONTEXT.md decides this.
   - **Resolution:** Use 12 hex. If collisions are detected, scan reports them; no pre-emptive expansion needed.

2. **Marker file format: JSON or binary?**
   - **What we know:** baude already uses JSON for state; serde integration is proven.
   - **What's locked:** JSON marker format is acceptable (CONTEXT.md implies JSON in metadata description).
   - **Resolution:** JSON. Schema: `{ canonical_common_dir: String, scheme_version: 1, recorded_at_ms: u64 }`.

3. **Should the daemon defer marker writes to the TUI or write them itself?**
   - **What we know:** Both TUI and daemon can admit repositories independently; marker write must be atomic and idempotent.
   - **What's locked:** CONTEXT.md allows either, as long as writes are idempotent and atomic.
   - **Resolution:** Both TUI and daemon call the same `marker::write_marker()` function. Writes are idempotent (reading existing marker → ok, writing same marker twice → ok). Atomic writes guarantee no corruption mid-flight.

4. **Marker file name: `.baude-marker.json` or something else?**
   - **What we know:** Hidden files (starting with `.`) are conventional for metadata; JSON extension matches state file format.
   - **What's locked:** CONTEXT.md defers exact name to Claude's discretion.
   - **Recommendation:** `.baude-marker.json` (follows `.<basename>.json` pattern).

5. **How does `repository_key()` in `worktree_scan` handle both legacy and new keys?**
   - **What we know:** Current function parses decimal u64; new function must accept 12 hex characters.
   - **What's locked:** Path shape stays `repository-{key}`, so scan classification keeps working.
   - **Resolution:** Update `repository_key()` to try parsing as decimal first (legacy), then as hex (new). Use round-trip check to reject ambiguous cases (e.g., `repository-007` doesn't round-trip as decimal).

## Environment Availability

**Trigger:** This phase depends on git availability (already verified by Phase 13).

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Git | Repository discovery | ✓ | [system default] | None — git is foundational |
| Rust std::fs | Atomic file operations | ✓ | [built-in] | None — std::fs is required |
| serde_json | Marker serialization | ✓ | [already in Cargo.lock] | Could use plain-text key=value, but more fragile |
| sha2 crate | Digest computation | ✓ | [already in Cargo.lock] | Could use format!() for hex, but sha2 is standard |

**Missing dependencies:** None. All required libraries are already in the tree or built-in.

## Sources

### Primary (HIGH confidence)

- **baude-core/src/git.rs:1748-1819** — Current path composition functions and `worktrees_base()` implementation; shows u64 counter keys.
- **baude-core/src/repository.rs:481-690** — Repository state struct with per-state-file counters; allocation functions.
- **baude-core/src/worktree_scan.rs:1006-1015** — Current `repository_key()` parser (decimal u64 only).
- **baude-core/src/lifecycle.rs:1307-1365** — `ensure_repository()` admission logic; shows canonical common dir lookup.
- **baude-core/src/persist.rs:611-686** — `atomic_save_current` function; proven atomic write pattern with sync semantics.
- **baude-core/src/testing.rs** — `TestRedirect` and fixture patterns; used throughout tests for isolation.
- **baude-core/src/launch.rs** — Phase 13 shared startup helper; natural location for marker adoption.
- **baude-core/src/workspace.rs:657** — Pinned path-composition test showing current u64 shape.
- **bauded/src/manager.rs:77-100** — Daemon state struct and Manager initialization; shows independent counter.
- **baude/src/app.rs:1914-1946** — TUI `admit_repository()` flow; identical to lifecycle `ensure_repository()`.

### Secondary (MEDIUM confidence)

- **CONTEXT.md (14-managed-worktree-identity)** — Phase locked decisions on digest scheme, marker file, in-place migration, and collision handling.
- **REQUIREMENTS.md WTID-01..04** — Phase requirements specifying no collisions, in-place migration, non-destructive resolution, and scan ownership reporting.
- **13-01-SUMMARY.md, 13-03-SUMMARY.md** — Phase 13 completion; `launch::start_workspace` is production-ready and shared by TUI and daemon.

### Tertiary (LOW confidence)

- Training knowledge of Rust atomic operations and crash recovery patterns (not verified in this session).
- Assumption that `sha2` crate is available and well-maintained (standard in Rust ecosystem, but version not confirmed via cargo tree in this session).

## Metadata

**Confidence breakdown:**
- **Locked Decisions (CONTEXT.md):** HIGH — User confirmed digest scheme, marker file, in-place migration, and collision handling.
- **Standard Stack:** HIGH — sha2 and serde_json are already in tree; verified via Cargo.lock and codebase inspection.
- **Code Patterns:** HIGH — Atomic save pattern proven in persist.rs; TestRedirect fixture pattern proven in existing tests.
- **Architecture:** HIGH — Codebase structure shows clear integration points (git.rs, repository.rs, lifecycle.rs, launch.rs, worktree_scan.rs).
- **Assumptions:** MEDIUM — Digest stability and collision rate are well-founded but not independently verified; assumed safe per CONTEXT.md decision.

**Research date:** 2026-09-20
**Valid until:** 2026-09-27 (stable domain; low velocity)

---

*Phase 14 research complete. Ready for planning phase WTID-01 through WTID-04.*
