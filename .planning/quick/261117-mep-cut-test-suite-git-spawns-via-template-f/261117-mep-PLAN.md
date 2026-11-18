---
phase: 261117-mep-cut-test-suite-git-spawns-via-template-f
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - .planning/quick/261117-mep-cut-test-suite-git-spawns-via-template-f/measure-git-spawns.sh
  - baude-core/src/testing.rs
  - baude-core/src/git.rs
  - baude-core/src/lifecycle.rs
  - baude-core/src/worktree_scan.rs
  - baude-core/src/removal.rs
  - baude/src/app.rs
  - bauded/src/api.rs
  - bauded/src/manager.rs
  - scripts/gate.sh
  - .github/workflows/ci.yml
autonomous: true
requirements: [QUICK-261117-mep]

estimate:
  tokens: 95000
  raw_tokens: 95000
  tasks: 3
  confidence: low

must_haves:
  truths:
    - "The SUMMARY records a baseline and a final whole-suite git spawn count and nextest wall time, both taken by the same counting wrapper through the same scripts/gate.sh invocation"
    - "If the Task 1 stop rule fired, no Rust, gate or CI file changed, and the SUMMARY carries the numbers that fired it"
    - "bash scripts/gate.sh exits 0 after the change. It runs the baseline test count plus only the new fixture tests, with the same skipped count and the same two #[ignore] attributes"
    - "No test fixture starts a git process to set user.name or user.email, and GitFixture::repo starts on topic through init -b topic instead of renaming the branch after the commit"
    - "Clone-shaped and linked-worktree fixtures still run a real git clone and a real git worktree add in every test that uses them"
    - "When Task 3 ran, with BAUDE_GIT_TEMPLATE_DIR set a templated repository matches an in-place one in tree OID, HEAD ref, refs and upstreams, local config apart from its own origin path, and file set, and git status reads it clean. With the variable unset every helper builds in place exactly as before"
    - "When Task 3 ran, scripts/gate.sh and the CI suite step each point BAUDE_GIT_TEMPLATE_DIR at a fresh per-run directory beside the suite TMPDIR, never inside it and never inside the checkout"
  artifacts:
    - path: ".planning/quick/261117-mep-cut-test-suite-git-spawns-via-template-f/measure-git-spawns.sh"
      provides: "Counting git wrapper, counted and timed gate runs, spawn and wall-time report, per-spawn benchmark"
      contains: "NEXTEST_TEST_NAME"
    - path: "baude-core/src/testing.rs"
      provides: "set_git_identity reuse, cloned_topology without per-worktree rediscovery, and in Task 3 committed_repo and pushed_repo with a build-once per-run template"
    - path: "scripts/gate.sh"
      provides: "BAUDE_GIT_TEMPLATE_DIR exported by isolated() into a per-run directory beside the suite TMPDIR (Task 3 only)"
      contains: "BAUDE_GIT_TEMPLATE_DIR"
  key_links:
    - from: "scripts/gate.sh"
      to: "baude-core/src/testing.rs"
      via: "BAUDE_GIT_TEMPLATE_DIR set in isolated(), read by the template_dir lookup"
      pattern: "BAUDE_GIT_TEMPLATE_DIR"
    - from: ".github/workflows/ci.yml"
      to: "baude-core/src/testing.rs"
      via: "BAUDE_GIT_TEMPLATE_DIR set on the step that runs scripts/test-shard.sh"
      pattern: "BAUDE_GIT_TEMPLATE_DIR"
    - from: "baude-core/src/git.rs"
      to: "baude-core/src/testing.rs"
      via: "GitFixture::repo calls committed_repo"
      pattern: "committed_repo"
    - from: "baude/src/app.rs"
      to: "baude-core/src/testing.rs"
      via: "admission_repo calls pushed_repo"
      pattern: "pushed_repo"
---

<objective>
Cut the git processes the test suite starts. The plan collapses repeated git calls in the fixture helpers. If the measured numbers justify it, it then builds each plain or pushed fixture repository once per run and copies it into every test that needs one, instead of running git init, add and commit per test. No test is skipped or weakened. Clone and linked-worktree shapes stay real.

Purpose: the gate's own comment says the suite makes some 15,000 git calls. Each one costs tens of milliseconds on macOS, and under nextest each test is its own process, so the cost is paid per test and never amortized.

Output: a measurement script in the quick-task directory, collapsed fixture helpers, an optional per-run template mechanism in baude-core/src/testing.rs wired through scripts/gate.sh and CI, and a SUMMARY with before and after spawn counts and wall times.

Order and stop points: Task 1 measures and can stop the whole plan. Task 2 makes cheap, always-safe helper changes. Task 3 builds the template only if a measured go rule holds, and it takes the final measurement either way.
</objective>

<execution_context>
@~/.poindexter/claude/gsd-core/workflows/execute-plan.md
@~/.poindexter/claude/gsd-core/templates/summary.md
</execution_context>

<context>
@.planning/STATE.md
@CLAUDE.md
@scripts/gate.sh
@baude-core/src/testing.rs

<interfaces>
Existing test-support API in baude-core/src/testing.rs, gated `#[cfg(any(test, feature = "test-support"))]` in baude-core/src/lib.rs. Line numbers are as of HEAD 24ddf35 and may drift.
- `pub fn git_in(dir: &Path, args: &[&str]) -> String` at line 344. It panics with stderr on a non-zero exit.
- `pub fn set_git_identity(repo: &Path, name: &str, email: &str)` at line 365. It appends a `[user]` section to `repo/.git/config` and panics unless that file exists, so it only works on a freshly initialized non-bare main checkout. A self-test near line 910 reads the identity back through `git config` and must stay.
- `pub fn cloned_topology(root: &Path) -> ClonedTopology` at line 408. It builds an upstream with init, add and commit, then runs a real clone, `switch -c chore/workspace` and two `worktree add` calls. After that it calls `discover_repository` three times: once for the clone and once for each worktree.
- `FixtureRoot::new(prefix)`: the prefix must start with `baude-` or `bauded-`. It creates `temp_dir()/<prefix>-<pid>-<seq>`.
- `baude_core::git::RepositorySnapshot` has fields `common_dir`, `main_worktree`, `selected_worktree: WorktreeRecord` and `worktrees: Vec<WorktreeRecord>`.

Fixture helper inventory at HEAD 24ddf35, used as the starting point for Task 1:
- baude-core/src/git.rs:3815 `GitFixture::repo` makes 6 spawns: init without -b, two identity config calls, add, commit, and a branch rename to topic. It has about 80 call sites in git.rs. Also in git.rs: `unborn_repo` at 3851, the remote and upstream helpers from 3860 to 3975, `linked_worktree` at 3977 and `git_ok` at 3998.
- baude-core/src/lifecycle.rs has five inline sequences that set identity through two git config spawns, at lines 3269, 3347, 3465, 3582 and 3653. Also `git_in` at 3937, `drift_repo` at 3954 and a local `cloned_topology` at 4318.
- baude-core/src/removal.rs: `RemovalFixture::new` near 1888 does init -b main, identity, add and commit. It has about 38 call sites. Also `managed_worktree` at 1919 and `init_repo` at 4586.
- baude-core/src/worktree_scan.rs: `git_repo` at 1960, `git_init_root` at 1978 (identity through two config spawns), and an inline sequence near 3879 (identity through two config spawns).
- baude-core/src/breadcrumbs.rs:515 `init_repository`. baude-core/src/launch.rs:159 and 195, and baude-core/src/folder_workspace.rs:493, run a bare unborn init.
- baude/src/app.rs: `git` at 8902 and `admission_repo` at 8995, which is the pushed shape: bare origin with init -b main, repo with init -b main, identity, file, add, commit, remote add, push -u. About 38 direct uses, plus fixtures built on it: `relaunch_fixture` at 10782, `suppression_fixture` at 10572, `saved_sessions_fixture` at 17324 and `stale_restore_fixture` at 17559. `admission_repo_cloned` at 9063 adds a real clone. `admission_collision_sets_status` at 17128 sets identity through two config spawns.
- bauded/src/api.rs: `committed_repo` at 1022, `initialized_repo_in_workspace` at 1072, `removal_fixture` at 2342, `push_to_bare_origin` at 2420 and `sibling_repo` at 2660.
- bauded/src/manager.rs: about 13 inline sequences of `init -b main` plus set_git_identity, file, add and commit, starting at 5206.
</interfaces>
</context>

<tasks>

<task type="auto">
  <name>Task 1: Baseline git spawn count, suite wall time and fixture helper inventory, with a stop rule</name>
  <files>.planning/quick/261117-mep-cut-test-suite-git-spawns-via-template-f/measure-git-spawns.sh</files>
  <precondition>The working tree is clean on branch gsd/fewer-git-calls, and `cargo nextest --version` reports 0.9.150.</precondition>
  <action>
Write the measurement script at .planning/quick/261117-mep-cut-test-suite-git-spawns-via-template-f/measure-git-spawns.sh. Keep it bash 3.2 compatible like scripts/test-shard.sh, with `set -u`, and give it three subcommands.

`run <label> <counted|timed|both>`:
- Set out to `${GIT_MEASURE_ROOT:-${TMPDIR:-/tmp}/git-measure}/<label>`. Exit 2 if out lies inside a git work tree, so logs never land in the checkout. Out is also never the suite TMPDIR, which the gate creates and checks for itself.
- Resolve the real git. On Darwin, when `command -v git` is /usr/bin/git, use `xcrun -f git`. Otherwise use `command -v git`. Require an absolute, executable path. The gate.sh comment explains why the xcrun shim is avoided.
- Generate `<out>/bin/git` as a /bin/sh script. Bake the real git path and the log directory in as literals, not environment variables, because some children run with a cleared environment. For each call, the script appends one line to `<out>/log/<PPID>.log` with three tab-separated fields: NEXTEST_BINARY_ID or `-`, NEXTEST_TEST_NAME or `-`, and every argv element followed by the 0x1f unit-separator byte. Then it execs the real git with the original arguments. nextest 0.9.116 and later set both variables in every test process.
- The counted run is `PATH="<out>/bin:$PATH" bash scripts/gate.sh` with stdout and stderr redirected to `<out>/gate-counted.log`. The gate builds its suite PATH from the caller's PATH, so the wrapper wins. Because `command -v git` is no longer /usr/bin/git, the gate's xcrun rewrite is skipped, and the wrapper calls the real binary directly.
- The timed runs are two plain `bash scripts/gate.sh` runs, each redirected to `<out>/gate-timed-N.log`.
- Capture every gate exit status directly. Never pipe a gate run, per CLAUDE.md. Exit 1 if any gate run failed.

`bench <label>`:
- Time 50 rounds of `init -q`, add and `commit -q` with the real git in a scratch directory under out. Report c, the milliseconds per spawn: total elapsed time divided by 150.
- Also time 50 `rev-parse HEAD` calls for reference.

`report <label>`:
- Print the suite spawns, meaning lines whose binary id is not `-`. Print the outside spawns separately. Those come from the gate itself and from children with a cleared environment.
- Count spawns by subcommand. Skip leading `-C <arg>` and `-c <arg>` pairs and other leading dash options. Keep the second word for worktree, config, branch, symbolic-ref and remote.
- Put spawns into a fixture-construction class: init, add, commit, clone, push, remote add, switch, checkout, update-ref, config with a value, branch create or rename, and symbolic-ref with a target. Before fixing the classes, `git grep` the non-test regions of baude-core/src/git.rs and the other production modules. Move every subcommand that production also issues, such as clone or worktree add, into an ambiguous class.
- List the top 25 tests by total suite spawns and the top 25 by fixture-class spawns.
- From each timed log, read the nextest Summary line: wall seconds, tests run, passed and skipped. Also sum the per-test PASS durations, and print the mean wall time.
- Exit 2 when the label has no counted or timed logs.

Run `run baseline both`, `bench baseline` and `report baseline`. If the first timed gate run fails at HEAD, stop and report: no baseline can be taken on a red gate.

Inventory: start from the interfaces list. Add every other test-code `Command::new("git")` site that builds a repository. For each one, record file:line, shape (plain committed, pushed, clone, linked worktree, unborn, or other), spawns per construction, number of call sites, and a disposition: Task 2 collapse, Task 3 template, or keep real. Cross-check the inventory against the two top-25 lists. If the suite total is far below the gate comment's 15,000, look for git started by PTY shells on their own PATH, and say so.

Stop rule: let W0 be the mean baseline wall time, F0 the baseline fixture-class suite spawns, and P = F0 × c / 1000 / 4, the most time removing every fixture-class spawn could save at four threads. If P is less than 5 percent of W0, stop after this task. Do not start Task 2. Write the SUMMARY with the numbers, the inventory and the reason, commit only the script and the SUMMARY, and report that the gains are negligible.

Commit the script with a message such as `perf: add a git spawn counter for the test suite`. Use no nested parentheses anywhere in the message.
  </action>
  <verify>
    <automated>bash .planning/quick/261117-mep-cut-test-suite-git-spawns-via-template-f/measure-git-spawns.sh report baseline</automated>
  </verify>
  <done>The report prints the baseline suite spawn total, the outside-suite total, per-subcommand counts, fixture, probe and ambiguous class totals, both top-25 lists, two timed wall times with their mean, tests run and skipped, and c. The inventory table and the stop-rule result, P against 5 percent of W0, are recorded for the SUMMARY. Either Task 2 starts, or the plan stops with that evidence committed.</done>
</task>

<task type="auto">
  <name>Task 2: Collapse repeated git calls in the fixture helpers</name>
  <files>baude-core/src/git.rs, baude-core/src/lifecycle.rs, baude-core/src/worktree_scan.rs, baude/src/app.rs, baude-core/src/testing.rs</files>
  <action>
Apply three rules to test code only. Do not change any production code.

Rule 1, identity: replace every pair of git config calls that sets user.name and user.email right after `git init` of a non-bare main checkout with `crate::testing::set_git_identity`, or `baude_core::testing::set_git_identity` outside baude-core, using the same name and email. This is the established pattern; set_git_identity's doc comment explains it. The sites are GitFixture::repo in baude-core/src/git.rs, the five inline lifecycle.rs sequences, git_init_root and the inline sequence near line 3879 in baude-core/src/worktree_scan.rs, and admission_collision_sets_status in baude/src/app.rs.
- The lifecycle sequences loop over argument vectors. Split each loop so that init stays a spawn with its success assertion and the identity becomes the call.
- A site whose `.git/config` is not a file, such as a linked worktree, stays as it is and is recorded.

Rule 2, branch: GitFixture::repo initializes with `-b topic` and drops the rename step that followed its commit. The end state is the same: one commit on topic and no other local branch. No test reads reflogs; `git grep` finds no reflog or logs/HEAD use. Leave GitFixture::unborn_repo and every other GitFixture method unchanged, so the unborn case keeps git's own default-branch behavior.

Rule 3, rediscovery: in testing::cloned_topology, and in lifecycle.rs's local cloned_topology if it also discovers each worktree, take the default and managed worktree paths from the clone snapshot's `worktrees` inventory instead of running discover_repository once per worktree.
- First read discover_repository in baude-core/src/git.rs and confirm that `selected_worktree` is taken from that same inventory. The inventory paths are then, by construction, the paths per-worktree discovery reports, and the helper's doc comment stays true.
- Match each record by the canonicalized worktree path and panic if none matches.
- If selected_worktree is not drawn from the inventory, skip this rule and record why.

Constraints:
- Remove no assertion other than a fixture step's own git exit check that moves into a helper that panics on failure. Add no `#[ignore]`.
- Do not write comments that quote the argument vectors you removed.
- Keep every clone, switch and worktree add a real git process.

Then:
- Run `bash scripts/gate.sh` with its exit captured directly. It must pass.
- Run `measure-git-spawns.sh run after-collapse counted` and `report after-collapse`. Record F1, the fixture-class suite spawns. Record E, the template-eligible suite spawns: init, add, commit, remote add and push in the fixture class. Task 3 uses E.
- Commit with a message such as `perf: write fixture identities directly and start GitFixture repos on their branch`, with no nested parentheses.
  </action>
  <verify>
    <automated>{ git grep -q -e '"config", "user\.name", "' -e '"config", "user\.email", "' -- '*.rs'; test $? -eq 1; } && { git grep -q -e 'OsStr::new("user\.name")' -e 'OsStr::new("user\.email")' -- '*.rs'; test $? -eq 1; } && { git grep -q -F 'OsStr::new("-M")' -- '*.rs'; test $? -eq 1; } && test "$(grep -rh -E '#\[ignore' --include='*.rs' baude-core baude bauded vendor | wc -l)" -le 2 && grep -q '"clone",' baude-core/src/testing.rs && test "$(grep -c '"worktree",' baude-core/src/testing.rs)" -ge 2 && bash scripts/gate.sh</automated>
  </verify>
  <acceptance_criteria>
Baselines measured at HEAD 24ddf35. Each `git grep -q` gate passes only on exit status 1, meaning no match; status 128 is an error and fails the gate.
- Identity-setting git config literals in .rs files, `git grep -n -e '"config", "user\.name", "' -e '"config", "user\.email", "' -- '*.rs'` lines [HEAD=16 EXPECT==0]
- OsStr identity literals, `git grep -n -e 'OsStr::new("user\.name")' -e 'OsStr::new("user\.email")' -- '*.rs'` lines [HEAD=2 EXPECT==0]
- GitFixture branch-rename literal, `git grep -n -F 'OsStr::new("-M")' -- '*.rs'` lines [HEAD=1 EXPECT==0]
- #[ignore] attributes, `grep -rh -E '#\[ignore' --include='*.rs' baude-core baude bauded vendor` lines [HEAD=2 EXPECT<=2 GUARD MUTATION: adding an #[ignore] to any test]
- Clone argv in baude-core/src/testing.rs, `grep -n '"clone",'` lines [HEAD=1 EXPECT>=1 GUARD MUTATION: replacing cloned_topology's real git clone with a copied repository]
- Worktree argv in baude-core/src/testing.rs, `grep -n '"worktree",'` lines [HEAD=2 EXPECT>=2 GUARD MUTATION: replacing either worktree add in cloned_topology with a copy or a hand-written gitfile]
  </acceptance_criteria>
  <done>The gate exits 0, with tests run equal to baseline and skipped equal to baseline. The after-collapse report gives F1 and E. Any site left unconverted is recorded with its reason.</done>
</task>

<task type="auto">
  <name>Task 3: Per-run fixture template for plain and pushed repositories if the numbers justify it, then the final measurement</name>
  <files>baude-core/src/testing.rs, scripts/gate.sh, .github/workflows/ci.yml, baude-core/src/git.rs, baude-core/src/removal.rs, baude-core/src/lifecycle.rs, baude/src/app.rs, bauded/src/api.rs, bauded/src/manager.rs</files>
  <action>
Go rule: let P3 = E × c / 1000 / 4. Build the template only if P3 is at least 5 percent of W0. Otherwise skip to the final measurement and record "template not built" with E, c, P3 and W0. Simple collapsing is the preferred outcome whenever the cache does not pay for itself. This task touches more than five files only because adoption replaces each fixture sequence with one call. Every edit outside testing.rs, gate.sh and ci.yml is that same mechanical replacement.

Mechanism, in baude-core/src/testing.rs, using std only. Add no crate.
- `pub struct CommittedRepo<'a>` with public fields `branch: Option<&'a str>`, `file: &'a str`, `contents: &'a [u8]`, `name`, `email` and `message`.
- `pub fn committed_repo(dest: &Path, spec: &CommittedRepo<'_>)`. In place, it runs `git init -q`, plus `-b` when branch is set, at dest. It then calls set_git_identity, writes the file, runs `git add` of that file and runs `git commit -q -m` with the message. That is exactly what the replaced sites do today.
- `pub fn pushed_repo(origin: &Path, repo: &Path, spec: &CommittedRepo<'_>)`. It runs `git init --bare -q -b <branch>` at origin, builds committed_repo at repo, runs `git remote add origin` with the origin path exactly as given, and runs `git push -q -u origin <branch>`. It panics when branch is None.
- Both functions delegate to internal variants that take `Option<&Path>` for the template directory, so tests never touch process environment. The public functions pass the result of a private template_dir lookup. That lookup reads BAUDE_GIT_TEMPLATE_DIR and returns None when the variable is unset, empty or relative.
- The template key is a shape tag plus every spec field, hashed with DefaultHasher. That hash is stable within one toolchain, and the directory is fresh per run. The template lives at `<dir>/<shape>-<16 hex>`. A plain template holds `repo`. A pushed template holds `origin.git` and `repo`. A `READY` file sits beside them and is written last.
- Building: when READY is missing, build into a private `<dir>/.building-<key>-<pid>-<seq>` with the in-place builders, write READY, then rename the directory to the final name. If the rename fails and READY now exists, remove your own building directory and use the existing template. Any other failure removes the building directory and builds in place at dest.
- Copying: walk the template recursively. Create directories, copy regular files with fs::copy, which keeps permission bits, and create the parent of dest. Any symlink or other file type aborts the copy, removes the partial dest and builds in place.
- Same filesystem: if the template directory's device differs from that of dest's nearest existing ancestor, build in place. `git init` probes the filesystem for core.ignorecase, precomposeunicode and filemode, and those settings must match the destination's filesystem.
- Pushed copies: rewrite `repo/.git/config`, replacing the single `url = <template origin>` line with the origin path as given. Assert that exactly one line changed. A wrong count panics, so a stale origin can never pass silently.
- Never write outside the template directory and the destination paths.

Known difference: a copied repository's index carries stale stat data, so the first git command that refreshes the index rewrites it. If a test fails, or behaves differently, with the template set but passes in place, revert that call site to in-place building and record it. Never edit the test.

Tests, in the testing.rs tests module, each under its own baude- FixtureRoot with an explicit template directory inside it:
- A templated committed repo matches an in-place one. Compare the sorted relative file lists of both trees including .git, `HEAD^{tree}`, `symbolic-ref HEAD` and `config --local --list`. `status --porcelain=v2` on the templated copy must be empty.
- The same comparison for pushed repos, plus origin `for-each-ref`, plus `for-each-ref` with refname and upstream in the repo. Compare the config lists after substituting each side's own origin path.
- A second call with the same spec reuses the template. The directory holds exactly one entry for that shape and no `.building-` entries.
- With None, the repo builds in place and the template directory stays absent.

Wiring:
- In scripts/gate.sh, isolated() sets BAUDE_GIT_TEMPLATE_DIR to `$isolated_root/git-templates`. Create that directory beside the existing home mkdir. It is a sibling of the suite TMPDIR `$isolated_root/tmp`, so assert-no-test-leftovers and assert-real-roots-untouched run unchanged. The trap removes it.
- Extend the comment block above isolated() to say why, and update its "some 15,000 git calls" figure to the measured final count, rounded.
- In .github/workflows/ci.yml, have the step that runs scripts/test-shard.sh under assert-no-test-leftovers create a sibling `mktemp -d` directory named `git-templates.XXXXXX` under the same `${TMPDIR:-/tmp}`, and export BAUDE_GIT_TEMPLATE_DIR to it for that command.

Adoption, in tracer order:
- First adopt GitFixture::repo, with branch topic, file tracked.txt, contents fixture, identity Baude Test at baude@example.invalid, and message fixture. Prove it with `BAUDE_GIT_TEMPLATE_DIR=<a scratch dir outside the checkout> cargo nextest run -p baude-core git::` and a counted run.
- Then fan out to every inventory site that builds exactly the plain or pushed shape from init through commit or push, with no interleaved step, and that the counted log shows built at least 5 times per run. Use each site's own exact spec. Expected sites: RemovalFixture::new, the upstream in both cloned_topology helpers, app.rs admission_repo as pushed, bauded committed_repo, initialized_repo_in_workspace, the removal_fixture pairing with push_to_bare_origin as pushed, and the manager.rs inline sequences.
- Steps after the commit stay real git: branch, clone, switch and worktree add.
- Do not touch sites built fewer than 5 times, git_init_root (it initializes a non-empty root), or unborn inits.

Final measurement, always, whether or not the template was built:
- Run `measure-git-spawns.sh run final both` and `report final`. Run `bash scripts/gate.sh` with its exit captured directly, which the timed runs already do.
- Write the SUMMARY with a table of baseline, after-collapse and final values: suite spawns, fixture-class spawns, the top subcommand rows, both wall times and their mean, the sum of test durations, and tests run and skipped.
- Add the inventory with each site's disposition, the go and stop rule arithmetic, and every site reverted or skipped with its reason.

Commits: `perf: build plain and pushed fixture repositories once per run and copy them` and `ci: give the suite a per-run git fixture template directory`, with no nested parentheses.
  </action>
  <verify>
    <automated>cargo nextest run -p baude-core testing:: && test "$(grep -rh -E '#\[ignore' --include='*.rs' baude-core baude bauded vendor | wc -l)" -le 2 && git diff --quiet 24ddf35 -- Cargo.lock Cargo.toml baude-core/Cargo.toml baude/Cargo.toml bauded/Cargo.toml && bash scripts/gate.sh && bash .planning/quick/261117-mep-cut-test-suite-git-spawns-via-template-f/measure-git-spawns.sh report final</automated>
  </verify>
  <acceptance_criteria>
Baselines measured at HEAD 24ddf35.

If the template was built:
- `grep -n '^pub fn committed_repo' baude-core/src/testing.rs` lines [HEAD=0 EXPECT>=1]
- `grep -n '^pub fn pushed_repo' baude-core/src/testing.rs` lines [HEAD=0 EXPECT>=1]
- `grep -n BAUDE_GIT_TEMPLATE_DIR scripts/gate.sh` lines [HEAD=0 EXPECT>=1]
- `grep -n BAUDE_GIT_TEMPLATE_DIR .github/workflows/ci.yml` lines [HEAD=0 EXPECT>=1]
- `grep -n BAUDE_GIT_TEMPLATE_DIR baude-core/src/testing.rs` lines [HEAD=0 EXPECT>=1]
- Clone argv in baude-core/src/testing.rs, `grep -n '"clone",'` lines [HEAD=1 EXPECT>=1 GUARD MUTATION: templating the clone step of cloned_topology instead of only its upstream]

In both cases:
- #[ignore] attributes, `grep -rh -E '#\[ignore' --include='*.rs' baude-core baude bauded vendor` lines [HEAD=2 EXPECT<=2 GUARD MUTATION: adding an #[ignore] to a test the template breaks]
- `git diff --quiet 24ddf35 -- Cargo.lock Cargo.toml baude-core/Cargo.toml baude/Cargo.toml bauded/Cargo.toml` exits 0
  </acceptance_criteria>
  <done>If the template was built, the four template tests pass. In both cases, the gate exits 0 with tests run equal to baseline plus only the new tests and skipped equal to baseline, and the SUMMARY holds the before and after table and every disposition.</done>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| runner environment to test-support code | BAUDE_GIT_TEMPLATE_DIR names a directory whose contents are copied into fixture repositories |
| template directory to fixture repositories | copied .git config and hooks are read and run by every git command a test makes |
| measurement wrapper to suite | a generated git shim sits first on PATH during counted runs only |

## STRIDE Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation Plan |
|-----------|----------|-----------|----------|-------------|-----------------|
| T-261117-mep-01 | Tampering | template copy in baude-core/src/testing.rs | medium | mitigate | Compiled only under cfg(any(test, feature = "test-support")). Absolute paths only. A template is used only when the READY file this code writes last exists. The copy refuses symlinks and non-regular files and falls back to an in-place build. The gate and CI create the directory fresh per run. |
| T-261117-mep-02 | Information disclosure | writes outside the fixture root | medium | mitigate | Writes go only to the template directory and the destination paths. The template directory sits beside the suite TMPDIR, never inside it or the checkout. assert-real-roots-untouched and assert-no-test-leftovers run unmodified around the suite. |
| T-261117-mep-03 | Tampering | test integrity | high | mitigate | No assertion is removed except a fixture step's exit check that moves into a panicking helper. The #[ignore] count is gated at 2. Tests run and skipped are compared with the baseline. Equivalence tests prove a templated repo matches an in-place one. A site that behaves differently when templated is reverted, never its test. |
| T-261117-mep-04 | Denial of service | concurrent template builds across nextest processes and CI shards | low | mitigate | Each build goes into a private building directory, READY is written last, the rename is atomic, the loser removes its own copy, and any failure falls back to an in-place build. |
| T-261117-mep-05 | Information disclosure | counting wrapper logs | low | accept | The logs hold only fixture paths and git arguments. They live under TMPDIR outside the checkout, the script refuses an out directory inside a work tree, and the wrapper is never committed onto any PATH. |
| T-261117-mep-SC | Tampering | npm/pip/cargo installs | high | accept | No package is installed and no crate is added. std only. Task 3's verify asserts Cargo.lock and every Cargo.toml are unchanged from 24ddf35. |
</threat_model>

<verification>
- `bash scripts/gate.sh` exits 0, captured directly and never piped.
- `measure-git-spawns.sh report baseline` and `report final` both succeed, and the SUMMARY shows the delta. Spawn counts are near-deterministic. Wall times are noisy, so two timed runs per point are reported, not one.
- Tests run equal the baseline plus only the new testing.rs tests, skipped equals the baseline, and the #[ignore] count is 2.
- `git diff 24ddf35 -- '*.rs'` shows no removed assertion outside fixture construction steps. The SUMMARY lists every removed fixture-step exit check and the helper that now panics in its place.
</verification>

<success_criteria>
- A measured baseline and a measured final count of suite git spawns, plus nextest wall time, are in the SUMMARY.
- Fixture identity costs no git process anywhere in the suite, and GitFixture::repo costs three spawns instead of six. With the template built, plain and pushed fixtures cost none.
- Clone and linked-worktree fixtures still run real git per test. No test is skipped, ignored or weakened.
- Every decision, stop, go or skip, is backed by recorded arithmetic.
</success_criteria>

<output>
Create `.planning/quick/261117-mep-cut-test-suite-git-spawns-via-template-f/261117-mep-SUMMARY.md` when done
</output>
