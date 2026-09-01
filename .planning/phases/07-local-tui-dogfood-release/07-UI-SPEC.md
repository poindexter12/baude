---
phase: 7
slug: local-tui-dogfood-release
status: draft
shadcn_initialized: false
preset: none
created: 2026-08-30
---

# Phase 7 — Local TUI Dogfood Release UI Design Contract

> Visual and interaction source of truth for the Phase 7 local Ratatui surface. This phase extends the established baude TUI; it does not redesign it.

---

## Scope and Product Contract

The local sidebar becomes a stable repository hierarchy. Durable repository parents remain visible with no runtime. Main checkouts, managed default worktrees, and other retained checkout/worktree children remain under their parent. Local hierarchy ordering is structural and never changes because a child starts working, waits, exits, closes, archives, or needs attention.

This contract includes local repository and checkout selection, branch create/activate, retained close/reopen, separately confirmed safe managed-worktree removal, existing applicable session actions, terminal resize behavior, and release-gate presentation/testing.

Explicitly excluded:

- Dormant branch rows, dormant-branch deletion, or any branch browser.
- Daemon repository hierarchy, remote hierarchy actions, and PWA work.
- Force removal, automatic stash/commit/reset/clean, branch deletion, fetch, or main-checkout switching.
- Release publishing, pushing a release, or presenting a “publish” action.

The flat daemon list remains a visually separate compatibility section. Do not infer or expose repository parents for remote rows and do not add destructive daemon behavior.

---

## Design System

| Property | Value |
|----------|-------|
| Tool | Existing hand-built Ratatui widgets; no web design system |
| Preset | Not applicable |
| Component library | Ratatui 0.30 (`Block`, `Paragraph`, `Line`, `Span`, `Clear`, `Rect`) |
| Icon library | Existing Unicode terminal glyph vocabulary; no icon dependency |
| Font | User-configured terminal monospace font; the application must not assume a specific face |
| Border language | Rounded borders; cyan focused border, dark-gray inactive border |
| Selection language | Full-row xterm-237 background plus `▌ ` gutter; cyan gutter when sidebar-focused, dark-gray when focus is elsewhere |

No new crate, font, icon set, or theme layer is authorized by this UI contract.

---

## Spacing Scale

The design scale remains the project’s compact 4-unit scale. In a terminal, one horizontal cell or one row is the indivisible rendering unit; the pixel values below are logical design tokens, not a request to control terminal font metrics.

| Token | Value | Terminal application |
|-------|-------|----------------------|
| xs | 4px | One-cell icon-to-label gap or compact inline separator |
| sm | 8px | Two-cell selection gutter and hierarchy indentation step |
| md | 16px | Four-cell child indentation / compact modal inset |
| lg | 24px | Six-cell section separation where width permits |
| xl | 32px | Eight-cell wide-layout separation |
| 2xl | 48px | Modal breathing room on wide terminals |
| 3xl | 64px | Maximum prompt width baseline |

Exceptions: terminal borders are one cell; each row is one terminal line; the selected-row gutter is exactly two cells; modal and pane geometry may use odd cell counts to center within an odd-width terminal. These are terminal-grid constraints, not arbitrary spacing tokens.

---

## Typography

The TUI controls emphasis, not font size. Exactly four semantic roles and two weights are allowed.

| Role | Size | Weight | Line Height |
|------|------|--------|-------------|
| Body | 1 terminal em | Regular (400 equivalent) | 1 row |
| Label | 1 terminal em | Bold (700 terminal equivalent) | 1 row |
| Heading | 1 terminal em | Bold | 1 row |
| Display/status | 1 terminal em | Regular | 1 row |

Allowed weights: regular and bold only. Do not add italic-only distinctions, tiny text, simulated large ASCII headings, or more than one row of vertical leading.

---

## Color Contract

| Role | Terminal value | Usage |
|------|----------------|-------|
| Dominant (60%) | `Color::Reset` | Terminal/content background and unselected rows |
| Secondary (30%) | `Color::Indexed(237)`, `DarkGray`, `Gray` | Selected-row band, inactive borders, metadata, hierarchy connectors, retained/archived states |
| Accent (10%) | `Cyan` | Focused border, focused selection gutter, input cursor, non-destructive modal border |
| Destructive | `Red` | Safe-remove confirmation border and destructive title only |
| Attention | `Yellow` | Waiting pulse/timer and transient actionable message background |
| Working | `Cyan` | Animated working spinner and active checkout name |
| Completed | `Green` | Calm completed check and existing GSD-positive title only |
| Managed worktree role | `Magenta` | Topology role chip only |
| Default checkout role | `Cyan` | Topology role chip only |
| Main checkout role | `Blue` | Topology role chip only |

Accent is reserved for focused pane borders, the focused selected-row gutter, input cursor, and informational/input modal borders. It must not color every action or every repository parent.

Status may never be communicated by color alone. Every state has a glyph and/or text label. Red is not used for ordinary close; it is reserved for safe physical worktree removal and existing severe alarm metadata.

---

## Information Architecture

### Sidebar sections

Render in this order:

1. Local durable targets: repository blocks and standalone non-Git folder rows interleaved by stable top-level path order.
2. Existing flat remote section, when configured, headed `⇄ remote` or `⇄ remote (offline)`.
3. Usage footer when vertical space allows.

Do not create a separate local `archived` section. Archive must not move a checkout out of its repository or move a standalone folder from its top-level position. Remote rows may retain their current flat compatibility ordering until daemon hierarchy work is explicitly scoped.

### Stable sort contract

| Collection | Primary sort | Deterministic tie-break | Forbidden sort inputs |
|------------|--------------|-------------------------|-----------------------|
| Repository parents | Case-insensitive display name ascending | Canonical main-worktree path ascending, then `RepositoryKey` | Runtime presence, status, attention, archive, health |
| Children within one parent | Persisted `first_seen_order` ascending | `CheckoutKey` ascending | Name, branch, role, runtime presence, status, attention, archive, health |
| Standalone folder rows | Case-insensitive display name ascending | Canonical path ascending, then `StandaloneKey` | Runtime presence, status, attention, archive, lifecycle |
| Flat remote compatibility rows | Preserve existing active/remote/archive compatibility behavior | Existing stable name ordering | Local repository state |

Selection follows stable durable identity, not row index. After any refresh or state transition, keep the same selected repository, checkout, or standalone key if it still exists. After successful worktree removal, select the next sibling; if none, the previous sibling; if none, the parent. Never jump to another target because a row changed status.

---

## Component Inventory

| Component | Purpose | Selectable | Height | Required states |
|-----------|---------|------------|--------|-----------------|
| `RepositoryRow` | Muted indented repository context and fallback action target | Only when no available child exists | 1 row | available, unavailable, selected fallback, aggregate waiting count, no-runtime |
| `CheckoutRow` | Main checkout or retained/active worktree identity | Yes | 2 rows normally; 1 compact row | running, waiting, working, completed, exited, retained/closed, archived, unavailable/recovery, selected focused/unfocused |
| `StandaloneRow` | Durable existing non-Git folder session | Yes | 2 rows normally; 1 compact row | running, waiting, working, completed, exited, retained/closed, archived, missing/recovery, selected focused/unfocused |
| `HierarchyConnector` | Repository context relationship independent of color | No | Inline | Indented `repo ·` context label and child metadata `↳` marker |
| `RoleChip` | Clarifies topology | No | Inline | `main`, `default`, `worktree`, `external` as applicable |
| `StatusGlyph` | At-a-glance runtime state | No | Inline | existing `●`, animated `◐/◓/◑/◒`, `✓`, `✗`; add `○` retained and `!` unavailable |
| `ActionHintBar` | Context-aware discoverability | No | 1 row | repository, live child, retained child, managed worktree, unavailable child, narrow overflow |
| `BranchInputModal` | Create new or activate eligible existing local branch | Modal | Dynamic | empty, editing, invalid/refused, submitted |
| `CloseConfirmation` | Stop runtime while retaining checkout | Modal | 4–6 rows | main, managed, external checkout target |
| `RemoveConfirmation` | Distinct physical safe-removal confirmation | Modal | 7+ responsive rows | clean managed target only |
| `TransientMessage` | Action result/refusal | No | Status row | success, blocked/actionable, degraded/recovery |
| Existing session overlays | Info, activity, GSD, help | Modal | Existing responsive size | Child context only where data applies |

---

## Row Anatomy

### Repository parent

```text
    repo · repository-name                  2 waiting
```

1. Two-cell selection gutter (`▌ ` or two spaces), normally blank because an available checkout is preferred.
2. Indented dim `repo ·` context label; repository rows are visually subordinate to checkouts.
3. Repository display name: italic/dim basename of canonical main worktree. If duplicate basenames exist, append the shortest unique parent-path suffix in dim text; identity never depends on this label.
4. Optional right-aligned aggregate text: `1 waiting`, `2 waiting`, or `unavailable`. Do not aggregate working/completed counts into the parent.
5. Parent text becomes bold cyan only when selected as the no-available-checkout fallback. It remains visible but muted while an available checkout exists.

### Checkout/worktree child, normal density

```text
▌ ● repo:main                            4m
    ↳ main · running · fable-5 · 63%
```

1. Two-cell selection gutter.
2. Existing status glyph at the primary visual level; no parent-to-child tree indentation precedes it.
4. Target name: preserve the established `repo:branch` session naming where available. For a non-default main checkout, use `repo:<branch>`; detached/unavailable branch displays `repo:<checkout-leaf>` plus the unavailable chip.
5. Existing wait/completed duration remains right aligned when width permits.
6. Metadata line starts under the child with a dim `↳`. Colored topology chips precede volatile session metadata: role, ownership/state, then model/context/permission/GSD.
7. The selected background spans both child lines and includes connector cells. Selection must not erase status colors.

### Compact child

```text
▌ ● repo:feature/a 4m
```

At constrained height or width, omit the metadata line first. Never omit the status glyph, target name, or selected gutter. Truncate the middle/least-important suffix before truncating the branch leaf. End truncation uses `…`; never allow text to overwrite the duration or border.

### State glyph and text matrix

| State | Glyph/style | Required text/chip | Motion |
|-------|-------------|--------------------|--------|
| Waiting for input | Pulsing `●`, yellow/dark-gray | `waiting` in detail/title; timer where space permits | Existing 360 ms pulse; row/name never flashes |
| Working | Blue quarter-circle spinner | `working` in detail/title | Existing 130 ms spinner |
| Completed | Dim green `✓` | `completed` | None |
| Exited runtime | Dark-gray `✗` | `exited` | None |
| Retained/closed, no runtime | Gray `○` | `closed` | None |
| Archived | Dark-gray `·` | `archived` | None; stays in place |
| Unavailable/protected recovery | Yellow `!` | concise cause such as `missing`, `changed`, `recovery`, or `state unsaved` | None |

Protected recovery and unavailable states must look blocked, not dead or removable. Their full cause and next action appear in the status message and Info overlay.

---

## Selection, Focus, and Navigation Contract

- `j`/`k` and `↑`/`↓` move through checkout/worktree rows in rendered order. A repository row is included only when that repository has no available checkout. Movement clamps at the ends.
- `alt+←/→` cycles checkout/session children only and wraps, preserving existing session-cycle behavior. Repository parents are skipped because they cannot receive PTY input. Closed or unavailable children remain reachable; attempting to attach produces the correct reopen/refusal behavior.
- `enter`, `l`, or `→` on a repository parent selects/focuses its default child: focus a live runtime, reopen an eligible retained default child, or show an actionable unavailable message. It must not create a duplicate.
- `enter`, `l`, or `→` on a live child focuses its agent pane. On an eligible retained child it performs Reopen Checkout. On an unavailable/protected child it refuses without mutation and names a repair path; it names `r` only when the lifecycle projection explicitly authorizes `RetryReopen` or `RetryRecovery`.
- `ctrl+q` always returns to the sidebar and retains the same durable selection.
- Focus remains encoded by border color plus selected-gutter color, never color alone.
- Mouse behavior for PTY selection/scroll remains unchanged. Sidebar mouse selection is not required by Phase 7 and must not be partially introduced.

---

## Action Matrix

`Shown` means include in the context hint bar and help. `Accepted` means the key dispatches. Hidden actions may still be defensively refused if a stale key event reaches dispatch.

| Action / key | Repository parent | Main checkout child | Managed worktree child | External/unmanaged worktree | Remote flat row |
|--------------|-------------------|---------------------|------------------------|-----------------------------|-----------------|
| Attach/reopen `enter` | Default child focus/reopen; exact unavailable copy below | Focus live or reopen retained; exact recovery copy below | Focus live or reopen retained; exact recovery copy below | Focus live or reopen retained | Existing attach |
| Create/activate branch `w` | Shown + accepted; exact typed failures below | Shown + accepted using parent repo; exact typed failures below | Shown + accepted using parent repo; exact typed failures below | Shown + accepted using parent repo | Hidden; preserve existing remote-only behavior |
| Close runtime `x` / `ctrl+x` | Hidden; exact repository refusal below | Accepted only when runtime exists; exact closed/recovery refusal below | Accepted only when runtime exists; exact closed/recovery refusal below | Accepted only when runtime exists | Existing flat compatibility close behavior; no physical worktree removal |
| Reopen/restart `r` | Accepted only with default-child `RetryReopen`/`RetryRecovery` capability | Accepted only with `RetryReopen`/`RetryRecovery` capability | Accepted only with `RetryReopen`/`RetryRecovery` capability | Existing eligible reopen behavior | Existing restart |
| Safe remove `X` (Shift+x) | Hidden; exact repository refusal below | Hidden; exact main-checkout refusal below | Shown only when `managed_by_baude`; exact preflight/recovery refusals below | Hidden; exact unmanaged refusal below | Hidden; no new destructive API |
| Shell `t`, `ctrl+\` | Hidden; refuse: `Cannot open a shell for “<repository>”: a repository parent has no checkout directory or live session. Select a live checkout child under “<repository>”, then press t.` | Existing behavior only with live local runtime | Existing behavior only with live local runtime | Existing behavior only with live local runtime | Existing refusal |
| Editor `e`, `ctrl+e` | Open canonical main checkout | Open selected checkout path, even retained if available | Same | Same | Existing host-location refusal |
| Info `i` | Repository summary | Checkout/session summary | Checkout/session summary | Checkout/session summary | Existing remote info |
| Activity `v` | Hidden | Runtime data when present; empty retained state is allowed | Same | Same | Existing remote activity |
| GSD `g` | Read from canonical main checkout | Existing repository GSD state | Existing repository GSD state | Existing repository GSD state | Existing refusal |
| Archive `a` | Hidden; refuse: `Cannot archive “<repository>”: a repository parent is a durable container, not checkout session state. Select a checkout child under “<repository>” with applicable session state, then press a.` | Accepted when runtime/session state applies | Accepted when runtime/session state applies | Accepted when runtime/session state applies | Existing behavior |
| New/open `n`, clone `c`, help `?`, quit `q` | Global sidebar behavior | Global sidebar behavior | Global sidebar behavior | Global sidebar behavior | Global sidebar behavior |

`X` is intentionally distinct from `x`. Close retains the checkout; remove physically removes only a verified clean baude-managed linked worktree and retains its branch. Do not combine these into one ambiguous “close/remove” choice.

### Standalone folder action contract

A standalone folder is a top-level session row with no repository parent. It
supports attach/reopen, close, shell, editor, info, activity, GSD, and archive
using its canonical folder as the working/project root. `w` always refuses
because no Git branch authority exists. `X` always refuses and never removes the
folder. A missing folder remains visible and `enter` rechecks the exact canonical
path before reopening; identity is never transferred to a different path.

### Exact selection × lifecycle-action contract

The following matrix is exhaustive for repository, main-checkout, and managed-worktree selections. `<repository>` and `<target>` are resolved display names for the durable keys; they are never generic words such as “item” or “selection.” “Accepted” means dispatch to the shared lifecycle protocol. A listed refusal is exact copy and performs no mutation.

| Selection | Create or activate `w` | Close `x` | Reopen `enter` / `r` | Remove `X` |
|-----------|-------------------------|-----------|--------------------------|------------|
| Repository parent `<repository>` | Accepted; typed branch failures use the table below | Refuse: `Cannot close “<repository>”: a repository parent is not a session. Select a running checkout, then press x.` `r`: not accepted | `enter`/`r` targets the durable default child when lifecycle-authorized. If none is usable: `Cannot reopen “<repository>”: its default checkout is unavailable. Open details with i, repair the reported Git topology, then use the action authorized there.` `r`: accepted only with `RetryReopen` capability | Refuse: `Cannot remove “<repository>”: repository parents are never removed by this action. Select a baude-managed worktree, then press X.` `r`: not accepted |
| Main checkout `<target>` | Accepted using its repository parent; typed branch failures use the table below | If running, accepted. If already retained: `Cannot close “<target>”: its session is already closed and the checkout is kept. Press enter to reopen it.` `r`: accepted only with `RetryReopen` capability | Live runtime focuses; eligible retained child reopens. Typed unavailable/recovery failures use the table below | Refuse: `Cannot remove “<target>”: the main checkout is never removable from baude. Keep it in Git and select a baude-managed linked worktree if removal is intended.` `r`: not accepted for removal |
| Managed worktree `<target>` | Accepted using its repository parent; typed branch failures use the table below | If running, accepted. If already retained: `Cannot close “<target>”: its session is already closed and the worktree is kept. Press enter to reopen it.` `r`: accepted only with `RetryReopen` capability | Live runtime focuses; eligible retained child reopens. Typed unavailable/recovery failures use the table below | Accepted only after clean managed-linked preflight; typed removal and recovery failures use the table below |

`r` is never a synonym for `w` or `X`. It retries only the lifecycle operation named by an explicit core capability on the selected durable target.

### Exact typed branch and safety refusals

| Typed failure | Action and exact refusal copy | Safe next input | Is `r` accepted? |
|---------------|---------------------------------|-----------------|------------------|
| Invalid ref | Create/activate: `Cannot create or activate “<branch>” in “<repository>”: “<branch>” is not a valid literal local branch name. Press w and enter a name accepted by Git.` | `w` | No |
| Remote-only branch | Create/activate: `Cannot activate “<branch>” in “<repository>”: only a remote-tracking branch exists. Create an explicit local branch outside baude, then press w to activate it.` | External Git, then `w` | No |
| Managed-path or filesystem collision | Create/activate: `Cannot create or activate “<branch>” in “<repository>”: the managed worktree path “<path>” collides with existing filesystem or Git state. Move or reconcile that path, then press w to retry.` | Repair path, then `w` | No |
| Branch occupied by protected checkout | Create/activate: `Cannot activate “<branch>” in “<repository>”: checkout “<target>” is in protected <recovery-state> state. Open details with i and complete the lifecycle-authorized recovery before pressing w again.` | `i`, then only the capability shown there | Only if the selected protected checkout separately exposes `RetryRecovery`; never as branch retry |
| Dirty tracked or untracked worktree | Remove: `Cannot remove “<target>”: dirty tracked or untracked files are present. Commit, move, or clean those files yourself, then press X to run a new safety check; nothing was removed.` | Resolve work, then `X` | No |
| Conflicted worktree | Remove: `Cannot remove “<target>”: unresolved Git conflicts are present. Resolve or abort the Git operation yourself, then press X to run a new safety check; nothing was removed.` | Resolve conflicts, then `X` | No |
| Locked worktree | Remove: `Cannot remove “<target>”: Git reports this worktree as locked. Review and unlock it with Git if safe, then press X to run a new safety check; nothing was removed.` | Review/unlock, then `X` | No |
| Submodule-unsafe worktree | Remove: `Cannot remove “<target>”: recursive submodule state makes non-force removal unsafe. Resolve the submodule worktrees yourself, then press X to run a new safety check; nothing was removed.` | Resolve submodules, then `X` | No |
| Indeterminate status/topology | Remove: `Cannot remove “<target>”: baude could not conclusively verify clean Git status and topology. Inspect the repository with Git, repair the reported error, then press X to run a new safety check; nothing was removed.` | Inspect/repair, then `X` | No |

### Exact unavailable-topology and recovery refusals

The UI receives retry capability as a derived lifecycle value. It must not derive permission from a yellow `!`, missing runtime, cause string, or button visibility. The only retry capabilities used here are `RetryReopen` and `RetryRecovery`; absence means `r` is refused and omitted from hints.

| Typed state | Attempted action | Exact refusal copy | Is `r` accepted? |
|-------------|------------------|--------------------|------------------|
| Unavailable topology: missing, moved, branch-changed, detached, locked/prunable, or repository identity changed | Create/activate | `Cannot create or activate a branch in “<repository>”: checkout “<target>” no longer matches the recorded Git topology (<cause>). Repair or restore the checkout at “<path>”, open details with i, then use only the authorized action shown there.` | Only if core also exposes `RetryRecovery`; otherwise no |
| Same unavailable topology | Close | `Cannot close “<target>”: its recorded runtime/topology state is unavailable (<cause>). Open details with i and repair the checkout; no session or checkout was changed.` | Only if core exposes `RetryRecovery`; otherwise no |
| Same unavailable topology | Reopen | With `RetryReopen`: `Cannot reopen “<target>”: Git topology is still unavailable (<cause>). Repair the checkout at “<path>”, then press r to recheck and reopen.` Without it: `Cannot reopen “<target>”: Git topology is unavailable (<cause>) and this state is not retryable from the TUI. Open details with i and repair the checkout; no runtime was started.` | Yes only with `RetryReopen`; otherwise no |
| Same unavailable topology | Remove | `Cannot remove “<target>”: its Git topology is unavailable (<cause>), so safe removal cannot be proven. Repair or reconcile it with Git, then press X for a fresh preflight; nothing was removed.` | No |
| Activation pending/recovery | Create/activate | `Cannot create or activate a branch in “<repository>”: “<target>” has an unfinished activation. Open details with i and complete the lifecycle-authorized recovery before starting another branch action.` | Yes only with `RetryRecovery`; otherwise no |
| Activation pending/recovery | Close | `Cannot close “<target>”: activation recovery must finish before close is legal. Open details with i; no runtime or checkout was changed.` | Yes only with `RetryRecovery`; otherwise no |
| Activation pending/recovery | Reopen | With `RetryRecovery`: `Cannot reopen “<target>” until activation recovery completes. Press r to continue the authorized recovery.` Without it: `Cannot reopen “<target>”: activation recovery is blocked. Open details with i; no runtime was started.` | Conditional exactly as copy indicates |
| Activation pending/recovery | Remove | `Cannot remove “<target>”: activation recovery must finish before removal can be inspected. Open details with i; nothing was removed.` | Yes only with `RetryRecovery`; otherwise no |
| Teardown pending | Create/activate | `Cannot create or activate a branch in “<repository>”: “<target>” still has teardown ownership to resolve. Open details with i and complete the authorized teardown recovery first.` | Yes only with `RetryRecovery`; otherwise no |
| Teardown pending | Close | With `RetryRecovery`: `Cannot start a new close for “<target>”: teardown is already pending. Press r to continue the authorized teardown recovery.` Without it: `Cannot close “<target>”: teardown recovery is blocked. Open details with i; process ownership was preserved.` | Conditional exactly as copy indicates |
| Teardown pending | Reopen | `Cannot reopen “<target>”: teardown recovery must reach a stable closed state first. Open details with i and use r only if retry is shown.` | Yes only with `RetryRecovery`; otherwise no |
| Teardown pending | Remove | `Cannot remove “<target>”: teardown recovery must complete before a fresh removal preflight. Open details with i; nothing was removed.` | Yes only with `RetryRecovery`; otherwise no |
| Removal pending/tombstone/committed recovery | Create/activate | `Cannot create or activate a branch in “<repository>”: “<target>” has protected removal state. Open details with i and let lifecycle recovery reconcile the committed Git facts first.` | Yes only with `RetryRecovery`; otherwise no |
| Removal pending/tombstone/committed recovery | Close | `Cannot close “<target>”: protected removal recovery owns this checkout. Open details with i; no process or persisted child was changed.` | Yes only with `RetryRecovery`; otherwise no |
| Removal pending/tombstone/committed recovery | Reopen | `Cannot reopen “<target>”: protected removal recovery may represent already-committed Git topology. Open details with i; baude will not recreate or launch it.` | Yes only with `RetryRecovery`; never `RetryReopen` |
| Removal pending/tombstone/committed recovery | Remove | `Cannot start another removal for “<target>”: protected removal recovery is already in progress. Open details with i; nothing else was removed.` | Yes only with `RetryRecovery`; otherwise no |
| Stopped-active/rollback recovery or unsaved lifecycle ownership | Create/activate | `Cannot create or activate a branch in “<repository>”: “<target>” has unresolved runtime ownership or unsaved lifecycle state. Repair persistence, open details with i, and complete the authorized recovery first.` | Yes only with `RetryRecovery`; otherwise no |
| Same stopped-active/rollback/unsaved state | Close | `Cannot close “<target>”: runtime ownership recovery is unresolved. Repair persistence and open details with i; no process ownership was discarded.` | Yes only with `RetryRecovery`; otherwise no |
| Same stopped-active/rollback/unsaved state | Reopen | With `RetryRecovery`: `Cannot reopen “<target>” as a new runtime while ownership recovery is unresolved. Repair persistence, then press r to continue the authorized recovery.` Without it: `Cannot reopen “<target>”: runtime ownership recovery is blocked. Repair persistence and open details with i; no runtime was started.` | Conditional exactly as copy indicates |
| Same stopped-active/rollback/unsaved state | Remove | `Cannot remove “<target>”: runtime ownership or persistence recovery is unresolved. Repair persistence and complete the authorized recovery before pressing X; nothing was removed.` | Yes only with `RetryRecovery`; otherwise no |

---

## Interaction and Modal Contracts

### Create or activate branch

- Primary label: `Create or activate branch`.
- Prompt title: `create or activate branch in <repository> — local branch name`.
- `w` is valid from a repository or any local child and always uses the durable parent as context, not the selected child as a base.
- New branch creation uses the verified default base. Entering an eligible existing local branch activates/reuses it. Remote-only branches, invalid refs, collisions, or a branch checked out outside an eligible reusable worktree are refused.
- `Enter` submits; `Esc` closes the input and keeps the repository unchanged; `Ctrl+U` clears. Do not show dormant branch rows or a branch-deletion affordance.
- Success message must name the branch and outcome: `created worktree for feature/a`, `activated feature/a`, or `focused existing feature/a`.
- Busy state: `Cannot create or activate a branch in “<name>”: another lifecycle action is in progress. Wait for it to finish, then press w to retry.` `r` is not accepted unless the selected protected child separately exposes `RetryRecovery`.

### Close checkout session

- `x` opens a non-destructive confirmation only when a runtime exists.
- Title: `close checkout session`.
- Body: `Close session “<target>” and keep its checkout for reopening?`
- Hints: `y/enter close · n/esc keep session open`.
- Success: `session closed — checkout kept`.
- The row remains under its parent, in the same persisted position, with `○ closed`.
- Closing a main checkout or worktree never offers physical removal in this modal.

### Reopen checkout

- `Enter` or `r` on a retained child performs the same durable reopen path.
- While pending, retain selection and show `reopening “<target>”…`; do not insert a duplicate row or spinner runtime before durable registration.
- Success focuses the agent pane and keeps the row in the same position.
- Reconciliation refusal uses the exact capability-sensitive unavailable-topology copy above. Never mention `r` unless `RetryReopen` or `RetryRecovery` is present.

### Safe remove managed worktree

- Entry key is `X` (Shift+x), separate from close.
- First preflight occurs before opening the confirmation. If blocked, no confirmation appears and no runtime, row, or persisted state changes.
- Confirmation title: `remove managed worktree` with red border.
- Body, in this order:
  1. `Remove this clean baude-managed linked worktree?`
  2. `target: <repo>:<branch>`
  3. `branch: refs/heads/<branch>`
  4. `path: <exact path>`
  5. `The local branch is retained. The repository parent and siblings are unchanged.`
  6. `y/enter remove · n/esc keep worktree`
- Long branch and path values wrap onto continuation rows; labels remain visible. The modal grows vertically up to the viewport, then clips the path middle with `…` while preserving its basename and branch.
- Confirmation performs the required stop and second preflight. If the second preflight or Git refuses, restore/preserve the runtime according to lifecycle authority and keep the row selected.
- Success: `worktree removed — local branch refs/heads/<branch> retained`; then select the deterministic neighbor described above.
- Never use “delete” for this action; no branch is deleted.

### Refusal and no-partial-mutation contract

Every refused/failed action must:

1. Name the action and actual target.
2. State the cause in user terms.
3. State the next safe step.
4. Leave selection on the target.
5. Leave row order unchanged.
6. Never optimistically remove a row or clear a status before the lifecycle outcome authorizes it.

The typed tables above are the complete refusal copy source of truth. An unmanaged-worktree defensive refusal remains: `Cannot remove “<target>”: it is not a baude-managed linked worktree. Keep it unchanged or remove it manually with Git if intended; nothing was removed.` `r` is not accepted for this action.

Transient messages keep the established yellow status-bar treatment and 5-second lifetime. Degraded/recovery state must also remain visible on the row after the transient expires.

---

## State Matrix

| Durable/UI state | Parent visible | Child visible | Attach/reopen | Close | Remove | Presentation |
|------------------|----------------|---------------|---------------|-------|--------|--------------|
| Repository, no children running | Yes | All durable checkouts | Eligible retained child only | Hidden | Eligible managed child only | Parent normal; children `○ closed` |
| Live running child | Yes | Yes | Focus | Confirm close | Preflight if managed | Existing live status glyph and metadata |
| Exited child | Yes | Yes | `r`/Enter restart-reopen | Close only if runtime ownership still applies | Preflight if lifecycle allows | `✗ exited` |
| Retained inactive child | Yes | Yes | Reopen | Hidden/refused as already closed | Preflight if managed | `○ closed` |
| Archived child | Yes | Yes, same position | Existing re-engage semantics | Context-dependent | Context-dependent and safety-gated | Dim `· archived`; never moved |
| Repository unavailable | Yes | Existing durable children | Refuse unsafe actions | No mutation | No mutation | Parent `! unavailable`; actionable message/info |
| Child missing/identity changed | Yes | Yes | Refuse | No mutation | No mutation | `! missing` or `! changed` |
| Activation/teardown/removal recovery | Yes | Yes | Typed refusal; retry only with explicit core capability | Typed refusal; retry only with explicit core capability | Only lifecycle-authorized recovery | `! recovery`; never appears retryable or removable merely because runtime is absent |
| Successful safe removal | Yes | No removed child row | Not applicable | Not applicable | Complete | Parent/siblings retain exact order; branch has no dormant row |
| Flat remote session | No synthesized local parent | Existing remote row | Existing compatibility behavior | Existing compatibility behavior | No new remove affordance | Existing `⇄ remote` section |

---

## Status Bar and Keyboard Hints

Hints are context-aware and ordered by immediate relevance. They must not advertise actions that are unsafe for the selected target.

| Selection/focus | Full-width hint text |
|-----------------|----------------------|
| Repository | `enter open default · w branch · e edit · i info · ? help` |
| Live local child | `enter attach · x close · X remove* · t shell · e edit · a archive · ? help` |
| Retained local child | `enter reopen · X remove* · e edit · i info · ? help` (`r` remains an accepted alias when `RetryReopen` exists, but is not duplicated in hints) |
| Unavailable child with `RetryReopen` | `i details · r recheck and reopen · ? help` |
| Recovery child with `RetryRecovery` | `i details · r continue recovery · ? help` |
| Unavailable/recovery child without retry capability | `i details · ? help` |
| Claude pane | Preserve `ctrl+q sidebar · ctrl+\ shell · alt+←/→ cycle` |
| Shell pane | Preserve `ctrl+q sidebar · ctrl+\ close shell · alt+←/→ cycle` |
| Remote flat row | Preserve remote attach/restart/close hints; omit local hierarchy actions |

`*` means include `X remove` only for a baude-managed linked worktree. At narrow widths, retain distinct actions rather than aliases: a retained managed child shows `enter reopen · X remove · ? more`; a retained non-removable child shows `enter reopen · e edit · ? more`. Never spend both narrow slots on `enter reopen` and `r reopen`. The Help modal always contains the complete matrix. Help wording changes from “select session” to “select repository or checkout” and states that `x` keeps the checkout while `X` removes only a verified clean managed worktree and retains its branch.

---

## Responsive Terminal Behavior

All geometry uses saturating arithmetic. Rendering at any terminal size must not panic, index outside the buffer, or request a zero-sized PTY.

| Terminal size | Contract |
|---------------|----------|
| Width ≥ 120 columns | Sidebar is 42 columns; content receives the remainder. Two-line children, full hints, path/branch status, and usage footer render when height allows. |
| Width 80–119 | Sidebar is `clamp(width / 3, 28, 38)`; two-line children remain unless height is constrained. Drop parent aggregate text, wait timer, and low-priority metadata chips in that order. |
| Width 60–79 | Sidebar is 26 columns. Child metadata is one compact second line only when it fits; status bar shows at most two actions plus `? more`. Content remains at least 34 columns. |
| Width < 60 | Single-pane mode. Sidebar focus renders hierarchy full width; agent/shell focus renders content full width. `Enter` moves to content; `Ctrl+Q` returns to the full-width hierarchy. Status bar remains one row. |
| Height ≥ 20 rows | Normal two-line children and six-row usage footer when at least four list rows remain. |
| Height 13–19 | Hide usage footer first; render compact one-line child rows when needed to keep the selected row visible. |
| Height < 13 | One-line hierarchy rows only; shell pane is visually suppressed without closing or mutating persisted `shell_open`; modal uses the full body and clips nonessential explanatory lines before target/action lines. |

Additional rules:

- The selected row must always be scrolled into view. Reserve one row above and below it when possible.
- Repository parent and selected child must remain visible together when they fit. If not, the child keeps a dim parent label prefix so hierarchy context is not lost.
- Horizontal truncation is Unicode-width-aware. Do not use byte length or naive `chars().count()` for buffer placement once hierarchy text is introduced.
- Resize updates all live PTYs to the actual visible content pane. In single-pane hierarchy mode, do not resize a hidden PTY to zero; retain the last valid size until content is shown, then resize.
- If height falls below 13 while `Focus::Shell` is active, immediately transfer input focus to the live agent pane and show `shell hidden at this terminal height — resize to 13+ rows or press ctrl+\ to close it`. From that frame onward, ordinary keys go to the agent PTY, never the hidden shell. If no live agent pane is attachable, transfer focus to the sidebar and show `shell hidden at this terminal height — resize to 13+ rows; session input is paused`. `Ctrl+\` may still intentionally close the hidden shell. Growing back to 13+ rows renders the still-open shell but does not steal focus; the user selects the child and presses `t` to focus it.
- Existing minimum remote attach dimensions (2 rows × 10 columns) remain intact.
- Modal width is `min(preferred, viewport width)` and height is `min(content height, viewport height)` with saturating centering.

---

## Accessibility Contract

- Every status uses text/glyph in addition to color.
- Selection uses background plus gutter; focus uses border plus gutter color.
- Hierarchy uses indentation plus connectors and parent-first reading order.
- Waiting animation is limited to the icon/timer; never flash the row or name. Archived rows never animate.
- Respect terminal color capabilities by remaining legible with colors reduced to monochrome. Parent/child, selected/unselected, live/closed, and destructive confirmation must still be distinguishable.
- Keyboard-only completion is mandatory for every action. No action may require a mouse.
- `?` is always discoverable in the status bar or `? more` overflow.
- Destructive confirmation defaults to preserving the target: unrelated keys do nothing, `n`/`Esc` keeps the worktree, and only `y`/`Enter` after explicit confirmation proceeds. Non-destructive close uses `n`/`Esc` to keep the session open.
- Use concise sentence case. Avoid ambiguous verbs such as “kill” for retained close and “delete” for worktree removal.

---

## Copywriting Contract

| Element | Exact copy/pattern |
|---------|--------------------|
| Primary CTA | `Create or activate branch` |
| Empty state heading | `no repositories yet` |
| Empty state body | `press n to open a repository or c to clone one` |
| Repository with no running child | `<N> checkouts · none running` or `no running sessions` when space is constrained |
| Generic action error | `Cannot <action> “<target>”: <cause>. <safe next step>.` |
| Persistence error | `State is not safely saved. Repair the named state file, save, then retry; no lifecycle action was started.` |
| Close confirmation | `Close session “<target>” and keep its checkout for reopening?` |
| Close negative action | `n/esc keep session open` |
| Remove confirmation | `Remove this clean baude-managed linked worktree?` plus exact target, branch, path, and retained-branch statement |
| Remove negative action | `n/esc keep worktree` |
| Remove success | `worktree removed — local branch <full-ref> retained` |
| Branch success | `created worktree for <branch>` / `activated <branch>` / `focused existing <branch>` |

No release-publish CTA or copy is allowed. Release documentation may say `ready for v2.0.0-beta` only after all gates pass; it must not say `released` or `published`.

---

## Registry Safety

| Registry | Blocks Used | Safety Gate |
|----------|-------------|-------------|
| shadcn official | None | Not applicable — Rust/Ratatui project |
| Third-party registries | None | No registry code authorized |

---

## Visual Regression and Test Guidance

Use Ratatui `TestBackend` or direct `Buffer` assertions in existing inline Rust test modules. Do not add a snapshot dependency solely for this phase.

### Required deterministic render fixtures

1. Two repositories whose names sort opposite their admission order.
2. One repository with a non-default main checkout, a separate managed default worktree, an older retained managed worktree, and a newer live worktree.
3. Waiting, working, completed, exited, closed, archived, unavailable, and recovery children in one stable first-seen sequence.
4. Duplicate repository basenames at different paths.
5. Long slash branch names, paths with spaces, and wide/combining Unicode labels.
6. A repository parent with no running children.
7. A clean managed worktree remove modal and each blocked preflight category.
8. A configured flat remote section proving no repository hierarchy is synthesized.

### Required viewport matrix

Render each relevant fixture at `160×40`, `100×30`, `79×24`, `59×20`, and `40×12`. Assert:

- No panic and no cell write outside the viewport.
- Expected rounded borders and focused/inactive border colors.
- Parent/child connectors, selected gutter, selected xterm-237 band, and status glyph styles.
- Selected two-line child receives one continuous background band.
- Narrow mode preserves target, status, selection, and `? more` before metadata.
- Single-pane mode switches predictably between hierarchy and content.
- Modal target/action lines remain visible at the smallest viewport.

### Behavioral test matrix

- Sorting tests mutate status, waiting age, archive, attention, runtime presence, and health, then assert identical durable row-key order.
- Restart tests serialize/reload and assert parent name order and child first-seen order are unchanged.
- Selection tests assert durable-key retention through every state change and deterministic fallback after removal.
- Action-dispatch table tests cover every selection kind × key, including defensive refusal with zero state/process/Git mutation.
- Close tests assert the child remains visible in place as `closed` and can reopen once.
- Removal tests assert first-preflight block, `n`/`Esc` keep-worktree outcome, second-preflight race, Git refusal, degraded postcondition, and success UI outcomes.
- Copy tests assert branch/path/target naming and retained-branch language in confirmations.
- Resize tests assert hidden PTYs never receive zero dimensions and regain correct dimensions when content returns.
- Compatibility tests assert old flat daemon/session projections stay non-destructive and no new hierarchy endpoint or remove action is invoked.

### Manual dogfood visual pass

Run an isolated real repository through open → create/activate → close → restart baude → reopen → safe remove. At each step record terminal screenshots at one wide and one narrow size. Confirm the selected target never jumps, parent/children never reorder, attention remains on the correct child, and the branch remains after worktree removal. This is local verification only; do not publish or push a release.

---

## Acceptance Predicates by Requirement

| Requirement | UI acceptance predicate |
|-------------|-------------------------|
| REPO-05 | When main is not the default checkout, one parent visibly contains both a `main` child and a distinct managed `default` child with truthful paths/branches. |
| HIER-01 | Every local repository renders one selectable parent followed immediately by its selectable main-checkout/worktree children and unambiguous connectors. |
| HIER-02 | Closing all runtimes and restarting leaves the repository parent and durable child rows visible. |
| HIER-03 | Parent order is case-insensitive name order with deterministic ties; child order is persisted `first_seen_order` ascending with key tie-break and survives restart. |
| HIER-04 | Waiting, working, archive, attention, completion, exit, close, and unavailable changes alter only row styling/content, never local row order; the correct child retains its status. |
| WORK-01 | `w` from a parent or local child opens the named branch prompt; valid new and eligible existing local branches produce one managed/reused child and one focused runtime. |
| WORK-02 | Invalid refs, remote-only refs, collisions, and unsafe occupancy show actionable target-specific refusal with unchanged hierarchy, runtime map, persistence, and Git topology. |
| WORK-03 | `x` confirmation closes the runtime, leaves the child in place as `○ closed`, and explicitly says the checkout is kept. |
| WORK-04 | Enter/`r` on an eligible retained main or worktree child reconciles, reopens exactly once, focuses the runtime, and preserves row identity/order. |
| WORK-05 | `X` is available only for baude-managed linked worktrees, performs a separate exact-target confirmation, removes the child only after success, and states that the full branch ref is retained. |
| WORK-06 | Dirty, conflicted, locked, submodule-unsafe, indeterminate, or changed-after-confirmation state blocks removal before optimistic row/process mutation and explains the safe next step. |
| SURF-01 | Repository, main, managed, external, unavailable, retained, and remote selections each expose only their contextual hints/actions; every confirmation names the actual target. |
| SURF-02 | Applicable open, clone, shell, editor, resume/restart, archive, attention, and session-cycle behaviors remain keyboard-accessible; local archive no longer breaks hierarchy order. |
| SURF-05 | Flat remote rows remain in a separate compatibility section; no daemon hierarchy is shown and no new destructive daemon action is reachable. |
| REL-01 | Isolated wide and narrow dogfood runs complete open/create-or-activate/close/restart/reopen/remove without duplicate parents, children, or runtimes, and without losing work or selection context. |
| REL-02 | Formatting, Clippy, full tests, package checks, and supported artifact builds pass; the render/action/resize matrices above are part of the test evidence. |
| REL-03 | TUI version text, Cargo/package metadata, changelog/release notes, and local install/dogfood docs consistently say `v2.0.0-beta` readiness; no UI/docs claim publication and no release is pushed. |

---

## Implementation Guardrails

- The UI consumes durable repository/checkout keys and derived lifecycle views. Display names, branches, paths, and runtime IDs are not selection identity.
- No UI handler performs direct Git mutation. It requests the existing shared lifecycle authority and renders its typed outcome/refusal.
- Do not reintroduce status-based sorting, local archived regrouping, or optimistic row deletion.
- Preserve exact agent/shell ownership and shared lifecycle effect ordering from Phase 6; visual responsiveness does not authorize speculative state.
- Keep daemon/PWA code changes limited to compatibility tests or release metadata strictly required by SURF-05/REL requirements.
- Preserve existing `●` waiting pulse, blue spinner, green completed check, xterm-237 selection band, two-cell gutter, rounded borders, pane focus, terminal passthrough, mouse selection, and transient message language unless this contract explicitly supersedes their placement.

---

## Checker Sign-Off

- [ ] Dimension 1 Copywriting: PASS
- [ ] Dimension 2 Visuals: PASS
- [ ] Dimension 3 Color: PASS
- [ ] Dimension 4 Typography: PASS
- [ ] Dimension 5 Spacing: PASS
- [ ] Dimension 6 Registry Safety: PASS

**Approval:** pending
