# Local TUI `v2.0.0-beta` dogfood runbook

This is a command-first, local-only morning certification runbook for an
explicitly checked-out `v2.0.0-beta` source tree. It isolates HOME, config,
data, state, install output, Git repositories, and the harmless fake backend.
It does not replace an installed `baude` or authorize any remote distribution.

Manual dogfood and image screenshots are pending. Record only observed outcomes in
`.planning/phases/07-local-tui-dogfood-release/07-UAT-EVIDENCE.md`; create that
file only when evidence exists. Do not create an empty anticipatory file.

## 1. Create isolated roots

Run these commands from the baude source root:

```sh
export BAUDE_SOURCE="$(pwd -P)"
export DOGFOOD_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/baude-beta-dogfood.XXXXXX")"
export HOME="$DOGFOOD_ROOT/home"
export XDG_CONFIG_HOME="$DOGFOOD_ROOT/config"
export XDG_DATA_HOME="$DOGFOOD_ROOT/data"
export XDG_STATE_HOME="$DOGFOOD_ROOT/state"
export INSTALL_ROOT="$DOGFOOD_ROOT/install"
export ORIGIN="$DOGFOOD_ROOT/origin.git"
export SEED="$DOGFOOD_ROOT/seed"
export REPO="$DOGFOOD_ROOT/repository"
export EVIDENCE_DIR="$DOGFOOD_ROOT/evidence"
mkdir -p "$HOME" "$XDG_CONFIG_HOME" "$XDG_DATA_HOME" "$XDG_STATE_HOME" "$INSTALL_ROOT" "$EVIDENCE_DIR"
printf 'isolated root: %s\n' "$DOGFOOD_ROOT"
```

Keep that shell open. Every later command inherits these values. Confirm that
all printed paths begin with the temporary root before proceeding.

## 2. Create a temporary bare origin and main checkout

```sh
git init --initial-branch=main "$SEED"
git -C "$SEED" config user.name "baude dogfood"
git -C "$SEED" config user.email "baude-dogfood@example.invalid"
printf 'local beta dogfood\n' > "$SEED/README.md"
git -C "$SEED" add README.md
git -C "$SEED" commit -m "seed local dogfood repository"
git clone --bare "$SEED" "$ORIGIN"
git clone "$ORIGIN" "$REPO"
test "$(git -C "$REPO" branch --show-current)" = main
git -C "$REPO" worktree list --porcelain | tee "$EVIDENCE_DIR/worktrees-initial.txt"
```

This repository is entirely temporary. The local bare origin supplies an
unambiguous `main` default without network access.

## 3. Build and install only the checked-out source

```sh
cargo build --workspace --release --locked
test "$(target/release/baude --version)" = 'baude 2.0.0-beta'
test "$(target/release/bauded --version)" = 'bauded 2.0.0-beta'
cargo install --path baude --root "$INSTALL_ROOT" --locked
test "$("$INSTALL_ROOT/bin/baude" --version)" = 'baude 2.0.0-beta'
```

The final install writes only beneath `$INSTALL_ROOT`; continue using the
absolute `$INSTALL_ROOT/bin/baude` path so another `baude` on PATH is untouched.

## 4. Configure a harmless backend and isolated workspace

The fake backend owns a PTY but performs no coding-agent or repository work:

```sh
export BAUDE_WORKSPACE=dogfood
export BAUDE_BACKEND=claude
export BAUDE_CLAUDE_CMD="sh -c 'while :; do sleep 3600; done'"
export BAUDE_NOTIFY=0
export BAUDED_AUTO_ARCHIVE_MIN=0
unset BAUDE_DAEMON_URL
mkdir -p "$XDG_CONFIG_HOME/baude"
printf '%s\n' '{"workspace":"dogfood","workspaces":{"dogfood":{"backend":"claude"}},"auto_daemon":false,"desktop_notifications":false}' > "$XDG_CONFIG_HOME/baude/config.json"
```

The durable file for this run is
`$XDG_CONFIG_HOME/baude/state-dogfood.json`. Do not hand-edit it.

## 5. Open the repository and record durable identity

Start at a wide terminal size, ideally `160x40`:

```sh
"$INSTALL_ROOT/bin/baude" "$REPO"
```

Observe one primary main-checkout row with its repository shown as muted,
indented context. The repository remains visible independently of runtime state
but navigation skips it while an available checkout exists. Capture a wide
screenshot, then quit with `q` and inspect the observed state:

```sh
cp "$XDG_CONFIG_HOME/baude/state-dogfood.json" "$EVIDENCE_DIR/state-after-open.json"
python3 -c 'import json,os; p=os.path.join(os.environ["XDG_CONFIG_HOME"],"baude","state-dogfood.json"); s=json.load(open(p))["state"]; print("RepositoryKey/order:",[(r["key"],r["first_seen_order"]) for r in s["repositories"]]); print("CheckoutKey/order:",[(c["key"],c["first_seen_order"],c["observed_branch"]) for c in s["checkouts"]])'
```

Copy the observed `RepositoryKey`, main `CheckoutKey`, and ordering values into
the morning evidence. Keys, not labels or runtime IDs, are the identities.

## 6. Create or activate, close, and prove retained state

Restart the same command at `160x40`, select the main checkout, then:

1. Press `w`, enter `feature/dogfood-beta`, and submit.
2. Observe one managed child beneath the same parent, after the older main
   child. Repeating `w` with that branch must focus the existing child rather
   than add another.
3. Record a wide screenshot showing muted repository context, primary main and
   managed checkout rows, selection band, status glyphs, and action hints.
4. Press lowercase `x`, verify that the confirmation says the checkout is
   kept, then confirm. Observe the same child in place as closed.
5. Quit with `q`.

Record exact inventory and durable order:

```sh
git -C "$REPO" worktree list --porcelain | tee "$EVIDENCE_DIR/worktrees-before-restart.txt"
cp "$XDG_CONFIG_HOME/baude/state-dogfood.json" "$EVIDENCE_DIR/state-after-close.json"
python3 -c 'import json,os; p=os.path.join(os.environ["XDG_CONFIG_HOME"],"baude","state-dogfood.json"); s=json.load(open(p))["state"]; print([(c["repository_key"],c["key"],c["first_seen_order"],c["active_intent"],c["observed_branch"]) for c in s["checkouts"]])'
git -C "$REPO" show-ref --verify -- refs/heads/feature/dogfood-beta | tee "$EVIDENCE_DIR/branch-before-remove.txt"
```

The managed child must retain the same `RepositoryKey`, `CheckoutKey`, and
order while its active intent becomes false.

## 7. Restart, reselect, reopen, and check narrow rendering

```sh
"$INSTALL_ROOT/bin/baude" "$REPO"
```

On restart, assert that selection initializes at the **first available local checkout**.
If a repository has no available checkout, its repository context row is the
fallback target. Selection is not persisted across processes. Parent name order
and child persisted oldest-first order must match the prior state. Explicitly
reselect the managed child with `j`/`k`, press `enter` to reopen it, and verify
that exactly one child and one fake-backend runtime represent it.

Resize the terminal to `40x12`. Capture a narrow screenshot showing that the
selected target, status, hierarchy context, and distinct `enter reopen` /
`X remove` hints remain legible without a panic or hidden-pane input. Resize
back to wide and confirm the durable selection still names the same
`CheckoutKey`; in-process selection survives rendering and status changes.

## 8. Prove standalone non-Git persistence

Create an existing folder that is not a Git repository, then press `n`, press
`ctrl+u` to clear the launch-directory prefill, and open it. Verify that it
appears once as a root-level `folder` row with no synthetic
repository/checkout parent. Reopen the same folder through a relative or symlink
alias and verify that no duplicate durable row or runtime appears.

Verify agent, shell, editor, info, activity, GSD, archive, lowercase `x` close,
and `enter` reopen behavior. Press `w` and uppercase `X`; both must refuse with
folder-specific copy and must not invoke Git or remove the folder. Quit and
restart, then verify the same `StandaloneKey`, canonical path, first-seen order,
resume identity, and archive state. Temporarily rename the folder and verify a
durable missing state with no spawn; restore the exact path and press `enter` to
reconcile and reopen it.

Capture the schema-v3 state before and after restart:

```sh
cp "$XDG_CONFIG_HOME/baude/state-dogfood.json" "$EVIDENCE_DIR/state-standalone.json"
python3 -c 'import json,os; p=os.path.join(os.environ["XDG_CONFIG_HOME"],"baude","state-dogfood.json"); s=json.load(open(p)); print("schema:",s["schema_version"]); print("standalone:",[(x["key"],x["first_seen_order"],x["canonical_path"],x["lifecycle"]) for x in s["state"]["standalone_sessions"]])'
```

## 9. Prove clean managed removal and branch survival

The Claude backend seeds `.claude/settings.local.json` in every launched
checkout. Removal preflight deliberately treats ignored files as blockers too,
so remove only that known generated file from this isolated fixture before the
physical worktree action. Resolve the path from durable state rather than
guessing it, then save the exact inventory:

```sh
export MANAGED_WORKTREE="$(python3 -c 'import json,os; p=os.path.join(os.environ["XDG_CONFIG_HOME"],"baude","state-dogfood.json"); s=json.load(open(p))["state"]; c=next(c for c in s["checkouts"] if c.get("observed_branch")=="refs/heads/feature/dogfood-beta"); print(bytes(c["observed_path"]).decode())')"
case "$MANAGED_WORKTREE" in "$DOGFOOD_ROOT"/*) ;; *) printf 'refusing unexpected path: %s\n' "$MANAGED_WORKTREE" >&2; exit 1;; esac
rm "$MANAGED_WORKTREE/.claude/settings.local.json"
rmdir "$MANAGED_WORKTREE/.claude"
git -C "$REPO" worktree list --porcelain | tee "$EVIDENCE_DIR/worktrees-before-remove.txt"
```

In baude, select the managed child and press uppercase `X`. Verify that this is
distinct from lowercase `x` and that the red confirmation names the exact
target, full `refs/heads/feature/dogfood-beta` ref, exact path, and branch
retention. Confirm only if the preflight reports the worktree clean. Observe
that the child disappears, the parent/main child remain, and selection falls
back deterministically. Quit with `q`, then record:

```sh
git -C "$REPO" worktree list --porcelain | tee "$EVIDENCE_DIR/worktrees-after-remove.txt"
git -C "$REPO" show-ref --verify -- refs/heads/feature/dogfood-beta | tee "$EVIDENCE_DIR/branch-after-remove.txt"
cp "$XDG_CONFIG_HOME/baude/state-dogfood.json" "$EVIDENCE_DIR/state-after-remove.json"
test "$(git -C "$REPO" worktree list --porcelain | awk '/^worktree / {n++} END {print n+0}')" = 1
```

Compare before/after inventory and state. The linked worktree and its
`CheckoutKey` must be absent afterward; the branch ref, repository parent, and
main child must survive.

## 10. Optional flat remote compatibility observation

If an already isolated test daemon is available for morning certification,
record that its rows appear in a separate flat remote section and retain only
their existing non-destructive compatibility actions. Do not synthesize local
parents or expose uppercase `X` for remote rows. Skip this observation when no
isolated daemon exists; do not substitute an unobserved claim.

## 11. Morning evidence checklist

Create `07-UAT-EVIDENCE.md` only when the corresponding evidence exists. Label
every omitted or failed item honestly.

- [ ] Exact source commit, host OS/architecture, Rust/Cargo versions, and
      `baude 2.0.0-beta` / `bauded 2.0.0-beta` command output.
- [ ] Temporary root and proof that HOME, config, data, state, repository, and
      install paths were isolated.
- [ ] Wide `160x40` screenshots for open, managed child, retained close,
      reopen, and safe-removal confirmation/result.
- [ ] Narrow `40x12` screenshots showing hierarchy context and distinct reopen
      versus remove hints.
- [ ] Observed `RepositoryKey`, each `CheckoutKey`, persisted oldest-first
      order, and unchanged identities through close/restart/reopen.
- [ ] First available local checkout restart initialization, repository fallback
      only when no checkout is available, and explicit managed-child reselection;
      no persisted-selection claim.
- [ ] Before/after `git worktree list --porcelain` output and exact branch-ref
      survival after managed worktree removal.
- [ ] Manual no-duplicate parent, child, or runtime observations.
- [ ] Schema-v3 standalone key/path/order evidence, alias deduplication,
      close/reopen/restart/missing-folder behavior, and `w`/`X` refusals.
- [ ] Supported CI and Linux/runtime certification, including Phase 6 process
      registration and descendant process-group extinction checks.
- [ ] Independent deep review, phase verification, Nyquist approval, UI-SPEC
      checker sign-off, requirement/phase completion, and publication decision.

## 12. Cleanup

First ensure baude has exited normally with `q`. From the original shell:

```sh
test -n "$DOGFOOD_ROOT"
test "$DOGFOOD_ROOT" != /tmp
test "$DOGFOOD_ROOT" != /
rm -rf "$DOGFOOD_ROOT"
```

Cleanup removes only the temporary dogfood root. Preserve copied morning
evidence before this step; otherwise rerun and observe it again rather than
reconstructing or fabricating results.
