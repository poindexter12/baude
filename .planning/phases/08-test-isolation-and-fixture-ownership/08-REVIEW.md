---
phase: 08-test-isolation-and-fixture-ownership
reviewed: 2026-09-15T18:37:53Z
depth: standard
files_reviewed: 23
files_reviewed_list:
  - .github/workflows/ci.yml
  - baude-core/Cargo.toml
  - baude-core/src/git.rs
  - baude-core/src/hook.rs
  - baude-core/src/lib.rs
  - baude-core/src/lifecycle.rs
  - baude-core/src/meta.rs
  - baude-core/src/persist.rs
  - baude-core/src/pty.rs
  - baude-core/src/testing.rs
  - baude-core/src/workspace.rs
  - baude-core/src/worktree_scan.rs
  - baude/Cargo.toml
  - baude/src/app.rs
  - baude/src/main.rs
  - baude/src/ui.rs
  - baude/src/usage.rs
  - bauded/Cargo.toml
  - bauded/src/api.rs
  - bauded/src/main.rs
  - bauded/src/manager.rs
  - bauded/src/push.rs
  - scripts/assert-real-roots-untouched.sh
findings:
  critical: 2
  warning: 5
  info: 4
  total: 11
status: issues_found
---

# Phase 8: Code Review Report

**Reviewed:** 2026-09-15T18:37:53Z
**Depth:** standard
**Files Reviewed:** 23
**Status:** issues_found

## Summary

Phase 08 migrates the suite off the developer's real roots by pairing every
real-root resolver with a guarded wrapper (`git::real_worktrees_base()` /
`git::worktrees_base()`, `persist::real_config_base()` / `persist::config_base()`,
`meta::real_claude_config_dir()` / `meta::claude_config_dir()`), routing the
guards through a thread-local `testing::REDIRECTS` store, gating the whole guard
on a dev-dependency-only `test-support` cargo feature so the two leaking binaries
are covered without contaminating `cargo build --release`, and bracketing CI with
a fingerprint observer. It also adds a 3,550-line read-only `worktree_scan`
module and a `baude worktrees scan` CLI.

The load-bearing parts of the design hold up under inspection and I want to say
so before the findings, because several of them are the kind of thing that is
usually wrong:

- **The feature gate is correct.** `test-support` is declared only under
  `[dev-dependencies]` in `baude/Cargo.toml` and `bauded/Cargo.toml`, and the
  workspace is `resolver = "2"`, so `cargo build --workspace --release --locked`
  in the `artifact-readiness` job does not select it. The guard does not ship.
- **`assert_contained` asks the right question.** Containment, not
  override-presence, means the guard is armed from the first instruction of the
  binary — `testing.rs:305-310` proves that with a test that constructs no
  redirect at all and is declared first in the module. `NoFixtureRoot` is
  thread-local, so the escape probe cannot perturb a concurrent case.
- **The three real resolvers were correctly *not* unified.** Their heads differ
  (`CLAUDE_CONFIG_DIR` vs `XDG_*`) and their tails differ (`"."` vs `/tmp`);
  collapsing them would have changed production behavior. The comments say so
  and the code matches the comments.
- **The removable predicate matches decision (B) structurally.** `classify()`
  resolves blockers first (`ProvesLive` → `Live`, `PreventsConclusion` →
  `Indeterminate`) before any clearing signal is consulted, `blocking_role()` is
  exhaustive with no wildcard arm so a future `Evidence` variant cannot silently
  default to "harmless", and `NoGitdir` is genuinely absent from the clearing
  `find_map`.
- **The prune flow matches decision (C) on re-derivation.** `prune_at`
  re-resolves roots independently, `validate_record` rejects absolute /
  separator-bearing / `.` / `..` / NUL components so a saved report cannot name
  the tree to act on (T-08-25), duplicates are refused, the full scan is
  re-derived, and any delta between approved and re-derived proof is a
  `ProofChanged` refusal — a *newly*-qualifying candidate is refused too.
  `remove_verified` re-stats non-following immediately before deletion and uses a
  bounded `remove_empty_tree` rather than `remove_dir_all`.
- **The PTY child containment is thorough** in the support build: `env_clear()`
  first, then `ZDOTDIR`/`ENV`/`BASH_ENV` closed alongside `HOME`, and a gate
  script that drops `-il` for `--noprofile --norc -i`.
- **`usage.rs` is compiled out rather than stubbed at the call site**, which is
  the only thing that would actually have worked: the ccusage worker is detached
  and never sees a thread-local.

Against that, two findings are Critical. CR-01 is exactly the class the phase
exists to eliminate and the class the review was asked to hunt: three
`dirs::home_dir()` resolvers survived the migration in code that *is* compiled
into the test harnesses, two of them behind filesystem-touching call sites and
one behind a `git clone` destination. CR-02 is a correctness defect in the
destructive surface: an entire arm of decision (B) — `GitDisownsIt` — is
provably unreachable at removal time, so `--prune` prints "would remove" for
candidates that `--yes` refuses 100% of the time, and decision (C)'s clause
about routing gitdir-bearing candidates through the existing verified-removal
path is not implemented at all.

## Narrative Findings (AI reviewer)

## Critical Issues

### CR-01: Three unguarded `dirs::home_dir()` resolvers survive the fixture migration in test-compiled code

**File:** `bauded/src/manager.rs:231-241`, `baude/src/app.rs:157-167`, `baude/src/ui.rs:1217-1228`

**Issue:**
Every root resolver the phase touched gained a guarded wrapper that calls
`crate::testing::assert_contained`. These three did not. All are private
helpers in `baude`/`bauded`, which means `cfg(test)` is set for them by
`rustc --test` and they are compiled verbatim into both test harnesses — the
exact binaries that produced the original 1,433-directory leak.

```rust
// bauded/src/manager.rs:231
fn expand_tilde(s: &str) -> PathBuf {
    if let Some(rest) = s.strip_prefix("~/") {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("/")).join(rest)
    } else if s == "~" {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
    } else { PathBuf::from(s) }
}
```

`app.rs:157-167` is a byte-identical copy. `ui.rs:1217-1228` (`tilde_path`)
reads `dirs::home_dir()` to shorten a path for display.

The reachable filesystem touches, in ascending order of damage:

1. **`ui.rs:628`, `ui.rs:1253`, `ui.rs:1262`** — `tilde_path` runs on every
   sidebar row and every status-bar draw. Observation of the real HOME string
   only; no filesystem access.
2. **`bauded/src/manager.rs:785`**, inside `create_session`:
   ```rust
   let repo = expand_tilde(repo);
   let repo = repo.canonicalize().unwrap_or(repo);
   if !repo.is_dir() { return Err(anyhow!("not a directory: {}", repo.display()).into()); }
   ```
   A `~`-prefixed `repo` in a `POST /sessions` body stats the real home.
3. **`baude/src/app.rs:181`** via `complete_dir_path`, reached from the Tab
   handler at `app.rs:4365-4375` for `InputKind::NewSessionPath` and
   `InputKind::CloneDest`. `complete_dir_path` calls `std::fs::read_dir(&search)`
   — a directory listing of the developer's real home from inside a test binary.
4. **`baude/src/app.rs:4514`**, `InputKind::CloneDest`, is the worst case. The
   destination buffer is prefilled at `app.rs:4626-4631` from
   `config.clone_base_dir` with a literal `"~/Code"` default; `submit_input`
   then does `expand_tilde(&value)`, `dest.join(".git").exists()`,
   `dest.exists()`, `std::fs::read_dir(&dest)`, and on a clear destination
   spawns a real `git clone` into it. A test that drives that modal to
   submission **writes a clone into the developer's real `~/Code/...`** — and
   the CI bracket would not necessarily flag it, because
   `scripts/assert-real-roots-untouched.sh` fingerprints the config, claude and
   worktrees roots, not `~/Code`.

No test in the current suite supplies a `~`-prefixed value (verified: the only
literal-tilde occurrences in `baude/src` and `bauded/src` are the three helpers
themselves, the `~/Code` default, and the display formatter), so this is a
latent escape rather than an observed one. That is precisely why it belongs
here: the phase's stated invariant is "containment is a property of the
compiled binary rather than of execution history", and these three call sites
are the only remaining holes in it. Nothing prevents the next test — a
tab-completion regression, a clone-flow case, a bauded API case that posts
`~/repo` — from reaching the real home, and the reads in (1)–(3) are invisible
to the CI observer, which compares size/mtime, not atime.

**Fix:** Give the tilde helpers the same treatment as every other resolver in
this phase — a real/guarded pair, with the guard resolving the fixture's home
and asserting containment in support builds. `baude`/`bauded` cannot call
`baude_core::testing::assert_contained` (it is `pub(crate)`), so either promote
a narrow public entry point or route the helpers through a new
`baude_core::persist`-style guarded home resolver:

```rust
// baude-core/src/testing.rs — promote a containment entry point
pub fn assert_contained_path(resolved: &Path, what: &str) { assert_contained(resolved, what) }

// baude-core (new, e.g. in persist.rs alongside real_config_base)
pub fn real_home_dir() -> PathBuf { dirs::home_dir().unwrap_or_else(|| PathBuf::from("/")) }
pub fn home_dir() -> PathBuf {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(base) = crate::testing::config_dir_override() {
        // the fixture's home, derived from the same root as every other redirect
        return base.parent().map(|r| r.join("home")).unwrap_or(base);
    }
    let real = real_home_dir();
    #[cfg(any(test, feature = "test-support"))]
    crate::testing::assert_contained(&real, "home directory");
    real
}
```

then in all three call sites:

```rust
fn expand_tilde(s: &str) -> PathBuf {
    if let Some(rest) = s.strip_prefix("~/") {
        baude_core::persist::home_dir().join(rest)
    } else if s == "~" {
        baude_core::persist::home_dir()
    } else { PathBuf::from(s) }
}
```

`ui.rs::tilde_path` is display-only and can use the guarded resolver too; if the
panic is judged too aggressive for a render path, at minimum deduplicate the
two identical `expand_tilde` copies into the guarded core helper so there is one
place to fix. Add a `#[should_panic(expected = "resolved to the real user path")]`
test per helper, matching the pattern already established in
`bauded/src/push.rs` and `testing.rs:305-310`. Separately, extend
`scripts/assert-real-roots-untouched.sh` to fingerprint the resolved
`clone_base_dir` root (default `~/Code`), which is currently outside every
observed root.

### CR-02: The `GitDisownsIt` clearing arm can never produce a removal, so `--prune` previews a removal that `--yes` always refuses

**File:** `baude-core/src/worktree_scan.rs:1097-1111` (`gitdir_evidence`), `worktree_scan.rs:1496-1503` (`prune_one`), `worktree_scan.rs:1530-1535` (`remove_verified`)

**Issue:**
Decision (B) makes `GitDisownsIt` one of the two clearing signals, and decision
(C) says gitdir-bearing candidates "go through the existing verified-removal
path". Neither holds in the implementation, and the two halves contradict each
other through the same predicate.

`gitdir_evidence` emits `GitDisownsIt` **only when a gitdir is present**:

```rust
fn gitdir_evidence(path: &Path) -> Option<Evidence> {
    let Some(holder) = gitdir_holder(path) else {
        return Some(Evidence::NoGitdir);      // no gitdir → NoGitdir, never GitDisownsIt
    };
    ...
    Some(Evidence::GitDisownsIt { owning_repository })
}
```

`remove_verified` refuses **exactly when a gitdir is present**, using the same
`gitdir_holder`:

```rust
if let Some(holder) = gitdir_holder(path) {
    return refused(RefusalReason::GitdirPresent { holder });
}
```

So a candidate cleared by `GitDisownsIt` is, by construction, a candidate
`remove_verified` refuses. The escape hatch — the `.git` entry disappearing
between scan and prune — is closed by the phase's own re-derivation: the
re-derived evidence would be `NoGitdir` instead of `GitDisownsIt`, the proofs
would differ, and `prune_one` returns `ProofChanged` before removal. There is
no path from `GitDisownsIt` to `Removed`. Since `Empty` and `GitDisownsIt` are
mutually exclusive (an empty directory has no `.git`, hence `NoGitdir`), the
removable predicate collapses in practice to **ShapeMatch +
NotReferencedByState + Empty**. The second arm of decision (B) is dead.

The user-visible defect is worse than the dead arm. `prune_one` returns
`WouldRemove` *before* `remove_verified` is ever consulted:

```rust
if !confirmed {
    return PruneDisposition::WouldRemove;   // line ~1500
}
remove_verified(&candidate.resolve(base))    // the gitdir check lives in here
```

and `print_prune_account` (`baude/src/main.rs:854-857`) renders that as
`"would remove  (re-verified and still matching; awaiting --yes)"`. An operator
who follows the documented two-step flow is told a gitdir-bearing candidate
will be removed, re-runs with `--yes`, and gets
`"refused (GitdirPresent …)"` every single time. The preview contract is false
for an entire evidence class, which is a correctness defect in the one surface
in this codebase that deletes things.

Decision (C)'s third clause is also unimplemented: `remove_verified` refuses and
stops. It does not hand the candidate to git's verified-removal path, and
`PruneDisposition` has no variant meaning "routed to git", so nothing downstream
can either.

**Fix:** Pick one of the two coherent resolutions and make the code say it.

Option A — make the dry run tell the truth (smallest change, keeps the refusal
semantics):

```rust
// worktree_scan.rs, prune_one
if !confirmed {
    // Run the same final gate the confirmed path runs, but report instead of act.
    return match removal_gate(&candidate.resolve(base)) {
        Ok(()) => PruneDisposition::WouldRemove,
        Err(reason) => PruneDisposition::Refused { reason },
    };
}
match removal_gate(&candidate.resolve(base)) {
    Ok(()) => remove_now(&candidate.resolve(base)),
    Err(reason) => PruneDisposition::Refused { reason },
}
```

where `removal_gate` is `remove_verified`'s existing symlink / not-a-directory /
gitdir checks factored out of the deletion itself. This makes `WouldRemove`
mean what its rendered string claims, and it surfaces the dead `GitDisownsIt`
arm immediately in the preview rather than after the operator commits.

Option B — implement decision (C) fully: add a `PruneDisposition::RoutedToGit`
(or equivalent) carrying the owning repository from
`Evidence::GitDisownsIt { owning_repository }`, invoke the existing verified
removal for those candidates, and render it distinctly in
`print_prune_account`.

Either way, add a test that constructs a gitdir-bearing, git-disowned candidate
and asserts the `--prune` disposition and the `--yes` disposition agree. The
current suite has no such case, which is why the contradiction survived.

## Warnings

### WR-01: `worktree_scan::scan()` is dead code that pairs a real root with a redirectable one — and `main()` repeats the mix

**File:** `baude-core/src/worktree_scan.rs:823-833`, `baude/src/main.rs:275-278`

**Issue:**
```rust
pub fn scan() -> Result<ScanReport, ScanError> {
    scan_at(&ScanRoots {
        worktrees_base: crate::git::real_worktrees_base(),
        config_dir: crate::persist::config_dir(),
    })
}
```
The doc comment calls this "the production wrapper, and the only place a real
root is resolved" — but nothing calls it. The CLI builds its own `ScanRoots` in
`main.rs:275-278`. Worse, the pairing is asymmetric in both places:
`real_worktrees_base()` deliberately ignores the redirect and carries no
containment assertion, while `config_dir()` is the *guarded* resolver that
consults `testing::config_dir_override()` and panics on escape. In a
support build the two halves of one `ScanRoots` disagree about which universe
they live in — the worktrees root points at the developer's real tree and the
config root points at the fixture. In the shipped release binary they happen to
coincide, which is what hides the inconsistency.

The root cause is that `persist::real_config_base()` is private, so there is no
real counterpart to pair with `git::real_worktrees_base()`.

**Fix:** Make `real_config_base` public with the same "ungated on purpose — the
leak scanner is production code whose job is the real root" rationale
`git::real_worktrees_base` already carries, use it in both places, and delete
the unused `scan()` (or wire the CLI through it so there is genuinely one place
a real root is resolved):

```rust
// persist.rs
/// The real config root, with no test redirect and no containment check.
/// Public for the same reason `git::real_worktrees_base` is: the leak scanner
/// must target the real tree.
pub fn real_config_base() -> PathBuf { /* unchanged body */ }

// main.rs:275
let roots = baude_core::worktree_scan::ScanRoots {
    worktrees_base: baude_core::git::real_worktrees_base(),
    config_dir: baude_core::persist::real_config_base(),
};
```

### WR-02: `baude worktrees scan --prune --yes` exits 0 even when every removal failed

**File:** `baude/src/main.rs:993-1007`

**Issue:**
```rust
match baude_core::worktree_scan::prune_at(roots, &approved, options.yes) {
    Ok(report) => { print_prune_account(&report, out); WORKTREES_EXIT_OK }
    Err(error) => { ...; WORKTREES_EXIT_FAILED }
}
```
`prune_at` returns `Ok` whenever the *report* was acceptable; per-candidate
refusals travel inside `PruneReport.outcomes`. So a run in which every approved
candidate came back `Refused { RemovalFailed { … } }` — a genuine I/O failure
during deletion — exits 0, which the help text at `main.rs:488` defines as "did
what was asked". An operator scripting this (the natural thing to do with a
two-step confirm flow) cannot distinguish "removed cleanly" from "nothing could
be removed" without parsing stdout prose.

`NotApproved` / `Unapproved` / `WouldRemove` are legitimately exit-0 outcomes;
`RemovalFailed` is not.

**Fix:** Return `WORKTREES_EXIT_FAILED` when any outcome is
`Refused { reason: RefusalReason::RemovalFailed { .. } }`, and document the
distinction in `worktrees_help_text()`:

```rust
Ok(report) => {
    print_prune_account(&report, out);
    let failed = report.outcomes.iter().any(|o| matches!(
        &o.disposition,
        PruneDisposition::Refused { reason: RefusalReason::RemovalFailed { .. } }
    ));
    if failed { WORKTREES_EXIT_FAILED } else { WORKTREES_EXIT_OK }
}
```

(Whether safety refusals — `GitdirPresent`, `BecameSymlink`, `ProofChanged` —
should also be non-zero is a policy call; `RemovalFailed` is not.)

### WR-03: `TestChildRoots::create()` swallows every directory-creation error, and its stated rationale is factually wrong

**File:** `baude-core/src/pty.rs:95-108`

**Issue:**
```rust
/// Create only these directories. A child whose `HOME` does not exist falls
/// back to the passwd database inside `portable-pty`, which is the real one.
fn create(&self) {
    for dir in [&self.home, &self.config, &self.data, &self.state, &self.cache, &self.claude] {
        let _ = std::fs::create_dir_all(dir);
    }
}
```

Two problems. First, the comment is wrong. `portable_pty`'s
`CommandBuilder::get_home_dir()` (`portable-pty-0.8.1/src/cmdbuilder.rs:511-520`)
returns the builder's `HOME` whenever that key is *present*, and only consults
the passwd database when `HOME` is unset. `configure_test_child` always sets
`HOME`, so a missing directory does not trigger any fallback — the child simply
gets a `HOME` that does not exist. The comment describes a containment failure
mode that cannot occur, which means a future reader may "fix" the wrong thing.

Second, and independently: every error is discarded. If `create_dir_all` fails
(permissions, a file where a directory is expected, ENOSPC), the child launches
with a `HOME`/`CLAUDE_CONFIG_DIR` pointing at a path that does not exist. The
symptom is a confusing downstream test failure — a child that cannot write its
config — rather than a clear "the fixture could not be built". This is the one
place in the phase where a containment precondition is established by a call
whose failure is unobservable; every other guard in the phase panics loudly.

**Fix:** Panic on failure and correct the comment.

```rust
/// Create only these directories. A `HOME` that does not exist is not a
/// containment failure — `portable_pty` honours a present `HOME` regardless of
/// whether it resolves — but it IS a broken fixture, so it aborts here rather
/// than surfacing as a confusing child-side failure later.
fn create(&self) {
    for dir in [&self.home, &self.config, &self.data, &self.state, &self.cache, &self.claude] {
        std::fs::create_dir_all(dir).unwrap_or_else(|e| {
            panic!("fixture child root {} could not be created: {e}", dir.display())
        });
    }
}
```

### WR-04: `build_gate_command`'s production branch applies caller env AFTER the protected gate keys, inverting its own stated policy

**File:** `baude-core/src/pty.rs:178-189`

**Issue:**
`configure_test_child` documents the rule explicitly (`pty.rs:115-120`):

> Order is the policy. The caller's explicit env goes in FIRST so opaque
> launch-plan values reach the child, and the protected root/shell/gate keys go
> in LAST so no caller — and no inherited value — can name a root or a startup
> file.

The support branch obeys it. The production branch does the opposite:

```rust
#[cfg(not(any(test, feature = "test-support")))]
{
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("BAUDE_GATE_TOKEN", GATE_TOKEN);
    cmd.env("BAUDE_GATE_SHELL", &shell);
    cmd.env("BAUDE_GATE_MODE", gate_mode(command));
    cmd.env("BAUDE_GATE_COMMAND", command.unwrap_or_default());
    for (key, value) in env {        // <-- caller wins
        cmd.env(key, value);
    }
}
```

`GATE_SCRIPT` execs `"$BAUDE_GATE_SHELL" -il -c "$BAUDE_GATE_COMMAND"`, so a
launch-plan env entry named `BAUDE_GATE_COMMAND` or `BAUDE_GATE_SHELL` replaces
the command the gate was built to run, and one named `BAUDE_GATE_TOKEN` breaks
the handshake. `env` currently originates from backend launch plans derived from
`config.json` (`app.rs:2877`, `manager.rs:1198/1228/2192`), not from an HTTP
body, so this is a defense-in-depth gap rather than a remote vector — but it is
a shipped code path whose ordering contradicts the documented invariant three
lines away from it, and the daemon's config is exactly the kind of thing that
grows a remote-write endpoint later.

**Fix:** Mirror the support branch — caller env first, protected keys last.

```rust
let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
for (key, value) in env {
    cmd.env(key, value);
}
cmd.env("TERM", "xterm-256color");
cmd.env("COLORTERM", "truecolor");
cmd.env("BAUDE_GATE_TOKEN", GATE_TOKEN);
cmd.env("BAUDE_GATE_SHELL", &shell);
cmd.env("BAUDE_GATE_MODE", gate_mode(command));
cmd.env("BAUDE_GATE_COMMAND", command.unwrap_or_default());
```

The comment at `pty.rs:115-120` should also move up to `build_gate_command`,
since after this change it describes both branches rather than only the test one.

### WR-05: the CI observer's own-state overlap check is lexical, so a symlinked snapshot directory defeats it

**File:** `scripts/assert-real-roots-untouched.sh:305-309` (`is_inside`), used by `assert_snapshot_is_outside_every_root` at `:312-325`

**Issue:**
```python
def is_inside(candidate, root):
    if root == "" or candidate == "":
        return False
    root = root.rstrip("/") or "/"
    return candidate == root or candidate.startswith(root + "/")
```
This is a pure string comparison on unresolved paths. The check it guards —
"the observer's own state must not be part of what it observes" — is exactly the
kind of invariant that a symlink defeats: if `BAUDE_ROOT_SNAPSHOT_DIR` (or
`RUNNER_TEMP`, or `tempfile.gettempdir()`) resolves through a symlink into one
of the three roots, the lexical comparison sees two unrelated strings, the
`FATAL` never fires, and `before.json` is written inside a tree whose contents
the `after` run then compares against a snapshot that includes itself. On
macOS runners `/tmp` is itself a symlink to `/private/tmp`, so
non-normalized-path mismatches are not hypothetical on this matrix.

Nothing else in the script is affected — `walk_root` correctly uses `os.lstat`
and never follows links — so this is confined to the self-protection check.

**Fix:** Normalize both operands with `os.path.realpath` before comparing, and
keep the lexical comparison as the second step:

```python
def is_inside(candidate, root):
    if root == "" or candidate == "":
        return False
    candidate = os.path.realpath(candidate)
    root = os.path.realpath(root).rstrip("/") or "/"
    return candidate == root or candidate.startswith(root + "/")
```

Add a self-test row that symlinks the snapshot directory into a synthetic root
and asserts the `FATAL` fires; the existing snapshot-location table only covers
the lexical cases.

## Info

### IN-01: `run_mode()` passes a possibly-`None` script path into `subprocess.run`

**File:** `scripts/assert-real-roots-untouched.sh:616-624`

**Issue:** `script = os.environ.get("BAUDE_ASSERT_SCRIPT")` can be `None`, and
it is handed straight to `subprocess.run([shell, script, mode], …)`, which
raises an opaque `TypeError` rather than a diagnosable message. The bash
preamble (`:36-38`) always exports the variable, so this only bites someone
running the embedded Python body directly — but the sibling `BAUDE_ASSERT_BASH`
lookup on the very next line *does* carry an `or "/bin/bash"` default, so the
asymmetry reads as an oversight.

**Fix:** Fail with a message instead:

```python
script = os.environ.get("BAUDE_ASSERT_SCRIPT")
if not script:
    raise SystemExit("--self-test must be reached through the bash wrapper "
                     "(BAUDE_ASSERT_SCRIPT is unset)")
```

### IN-02: `print_prune_account` discards every write error while narrating a destructive operation

**File:** `baude/src/main.rs:826-880`

**Issue:** Every line uses `let _ = writeln!(out, …)`. A broken pipe (`baude
worktrees scan --prune --report f --yes | head`) silently truncates the account
of what was removed, and the process still exits 0. For the read-only scan path
this is the right trade; for the confirmed-prune account it means the only
record of which directories were deleted can vanish without a trace.

**Fix:** On the confirmed path, either propagate the first write error into
`WORKTREES_EXIT_FAILED` (pairs naturally with WR-02's change) or emit the
account to stderr as well when stdout write fails.

### IN-03: pre-existing process-global env mutation in parallel-running tests, now masked by `--test-threads=1`

**File:** `baude-core/src/persist.rs:1756-1765`, `baude-core/src/permission.rs:736-773`

**Issue:** These tests call `std::env::set_var` / `remove_var`
(`BAUDED_AUTO_ARCHIVE_MIN`, `BAUDE_PERMISSION_MODE`) in a binary whose other
cases run concurrently. Neither variable is a root path, so this is not a
containment escape — but it is the same class of hazard the phase eliminated
elsewhere (the `testing.rs:76-86` comment explains precisely why thread-locals
were chosen over env vars), and CI's new `cargo test -- --test-threads=1`
(`ci.yml:35`) now hides any resulting flake from the one place it would have
been caught. Local `cargo test` is still parallel. Not introduced by this phase;
flagged because the phase establishes the convention these two violate.

**Fix:** Route both through the same thread-local redirect mechanism
`testing::Redirects` already provides (add `auto_archive_min` /
`permission_mode` fields), or inject the value as a parameter to the function
under test.

### IN-04: `bauded/src/main.rs:181` discards the initialized workspace

**File:** `bauded/src/main.rs:181`

**Issue:** `let _ = baude_core::workspace::initialize(&config, None);` throws
away the `&'static Workspace` it just resolved, so every later reader pays a
`workspace::active()` lookup and the start-up ordering guarantee is implicit.
Harmless today — `active()` is the documented reader and `initialize` is
idempotent via `OnceLock` — but binding it would let the comment above it
("Identity is resolved explicitly, once … BEFORE the first reader") be enforced
by the type system rather than by convention.

**Fix:** `let workspace = baude_core::workspace::initialize(&config, None);` and
pass it where the daemon already needs it, or keep `let _` but note in the
comment that `active()` is the intended accessor.

---

_Reviewed: 2026-09-15T18:37:53Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
