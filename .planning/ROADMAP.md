# Roadmap: baude

## Overview

v2.2 Reliability and Terminal Usability continues after completed Phase 7. Close the remaining test-isolation and hook-seeding gaps first, then deliver clickable HTTP(S) terminal links, negotiated multiline input, and validation through the existing v2.2.0 release workflow.

**Re-scoped 2026-09-13.** Releases v2.1.2 through v2.1.5 shipped fixes for issues #70, #71, #72 and #78 before this milestone began executing. Every Phase 8 and Phase 9 requirement was re-verified against main at v2.1.5: six are delivered, four are partial, two were never started. Phases 8 and 9 are narrowed to the verified remainder. Phases 10 through 12 are unchanged.

## Milestones

- [x] **v0.7 Session Visibility**: Phases 1-4, shipped 2026-07-02 ([archive](milestones/v0.7-ROADMAP.md))
- [x] **v2.0 Local TUI Dogfood Release**: Phases 5-7, shipped 2026-09-03 ([archive](milestones/v2.0-ROADMAP.md))
- [ ] **v2.2 Reliability and Terminal Usability**: Phases 8-12, proposed

v2.1.0 is the source baseline for this milestone and does not add a roadmap phase. Historical milestone labels and dates above retain the previous roadmap index; archived records remain authoritative for prior work.

## Completed Phases

<details>
<summary>✅ v0.7 Session Visibility (Phases 1-4) — SHIPPED 2026-07-02</summary>

- [x] Phase 1: Session Metadata (see milestones/v0.7-ROADMAP.md)
- [x] Phase 2: Working/Waiting Signal
- [x] Phase 3: Session Actions
- [x] Phase 4: Remote Permission Approval

</details>

<details>
<summary>✅ v2.0 Local TUI Dogfood Release (Phases 5-7) — SHIPPED 2026-09-03</summary>

- [x] Phase 5: Durable Repository Admission (3/3 plans) — completed 2026-08-30
- [x] Phase 6: Shared Lifecycle Core Refactor (7/7 plans) — completed 2026-09-02
- [x] Phase 7: Local TUI Dogfood Release (6/6 plans) — completed 2026-09-03

Published as the `v2.0.0-beta` prerelease bootstrap and handed to
release-please at `v2.0.0-beta.1`. Full phase detail:
[milestones/v2.0-ROADMAP.md](milestones/v2.0-ROADMAP.md).

</details>

## v2.2 Reliability and Terminal Usability

- [ ] **Phase 8: Test Isolation and Fixture Ownership** (narrowed): Close the config, workspace-identity, escape-guard and leak-preview gaps left after v2.1.4.
- [ ] **Phase 9: Hook Seeding Safety** (narrowed): Stop silently replacing unparseable settings and seed a shell-safe executable path.
- [ ] **Phase 10: Clickable Terminal Links**: Expose safe link activation, destination preview, and copy using authoritative screen metadata.
- [ ] **Phase 11: Negotiated Multiline Input**: Support Shift+Enter where capability is verified, with honest fallback elsewhere.
- [ ] **Phase 12: Validation and v2.2.0 Release**: Complete regression, CI, terminal smoke, documentation, and release validation.

## Phase Details

### Phase 8: Test Isolation and Fixture Ownership

**Goal**: Running the suite cannot read or write the developer's real config, state, or `~/.claude`, and suspected historical leaks can be inspected before anyone deletes anything.
**Depends on**: Nothing within v2.2. This phase precedes test-heavy milestone work.
**Requirements**: TISO-01 (remainder), TISO-02 (remainder), TISO-03 (remainder), TISO-04

**Already delivered (v2.1.4, PR #82)**: managed worktrees and repos are confined to per-label temp roots; `REQUIRE_WORKTREES_OVERRIDE` turns a managed-worktree escape into a failing assert; state persistence is redirectable; test redirects are thread-local and no test mutates the parent process HOME/XDG.

**Success Criteria**:

1. Config resolution (`persist::config_dir`, `meta::claude_config_dir`) accepts a test redirect, and no test run reads or writes the real `~/.config/baude` or `~/.claude` — including `bauded` push-subscription and VAPID key storage.
2. Workspace identity is resolvable per fixture rather than through a process-wide `OnceLock` seeded from the developer's real environment, so concurrent fixtures cannot share or race one identity.
3. The escape guard covers config, state, and `~/.claude` paths as it already covers managed worktrees, and is armed independently of whether some earlier fixture in the same test binary happened to arm it.
4. A developer can enumerate and preview suspected leaked test worktrees under the real data root without deleting them; removal requires verified ownership plus separate approval, and a missing gitdir alone never authorizes it.

**Plans**: 8/8 plans executed

Plans:

**Wave 1**

- [x] 08-01-PLAN.md — Tracer: cross-crate test-support gate, unified RAII redirect guard, complete setter migration (11-file scope warning remains)

**Wave 2 (after 08-01)**

- [x] 08-02-PLAN.md — `~/.claude` redirect and `bauded` push/VAPID resolver dedup
- [x] 08-03-PLAN.md — Per-fixture identity, explicit initialization, retained app/API/UI guard owners; owner-only verification

**Wave 3 (after identity/resolver prerequisites)**

- [x] 08-04-PLAN.md — Leak scan: evidence/verdict model and read-only enumeration (pending human decision)
- [x] 08-08-PLAN.md — Inert App workers, UI ownership regressions and contained PTY child environments

**Wave 4**

- [x] 08-05-PLAN.md — Complete state inventory and report-bound re-verifying prune (after 08-04, pending human decision)
- [x] 08-06-PLAN.md — Shared `bauded` fixture helper and suite-level assertion (after 08-08; first broad-test boundary)

**Wave 5 (after 08-05 and 08-06)**

- [x] 08-07-PLAN.md — `baude worktrees` CLI surface: grouped report, JSON preview input, two-flag prune

### Phase 9: Hook Seeding Safety

**Goal**: Seeding a project's hooks never destroys a user's existing settings and never emits a command string the shell will mis-execute.
**Depends on**: Phase 8 for isolated regression coverage.
**Requirements**: HREG-03, HREG-04 (remainder)

**Already delivered (v2.1.2 PR #77, v2.1.3 PR #80, v2.1.5 PR #84)**: all four lifecycle events converge to one baude-owned registration regardless of install path (HREG-01); custom hooks, mixed groups, matcher groups, the bare fallback and unrelated keys survive reconciliation verbatim (HREG-02); and the full workspace-lock contract — refusal before session operations, no takeover, diagnostic pid with recovery guidance, and `try_lock` rather than file existence deciding contention (WLOCK-01 through WLOCK-04).

**Success Criteria**:

1. An existing `.claude/settings.local.json` or `.mcp.json` that cannot be read or parsed is left untouched rather than overwritten with baude's seed alone, and the user receives an actionable warning naming the file.
2. The seeded hook command quotes or otherwise escapes the executable path, so an install path containing a space, `$`, `;`, or a backtick invokes exactly that executable. Verified 2026-09-13: hook commands are executed through a shell, so the current unquoted `format!("{} hook", ...)` is a live defect.
3. The seed recognizer matches the quoted form, so quoting does not reintroduce the per-path accumulation that HREG-01 fixed.
4. Regression tests cover a malformed settings file, a spaced install path end to end, and the two behaviors verified by inspection only in v2.1.3: reopening a workspace whose lock file remains after the OS lock released, and `bauded` encountering a held lock.

**Plans**: 3/4 plans executed

Plans:

**Wave 1**

- [x] 09-01-PLAN.md — Tracer: guarded `seed_settings` + `SeedWarning` seam end to end (trait ripple, TUI add-session surface)
- [x] 09-02-PLAN.md — Lock regression tests: leftover-lock-file reopen (persist) + bauded held-lock pid diagnostic

**Wave 2**

- [x] 09-03-PLAN.md — TDD: POSIX-quoted hook command, both-form recognizer with round-trip validation, `sh -c` E2E
- [ ] 09-04-PLAN.md — `.mcp.json` guard (command stays argv data), remaining spawn-path surfaces, app-level malformed-settings tests

### Phase 10: Clickable Terminal Links

**Goal**: Users can inspect, copy, and explicitly open validated HTTP(S) destinations while terminal rendering, selection, and remote attachment remain authoritative and safe.
**Depends on**: Phase 8 for isolated parser and interaction tests; not technically dependent on Phase 9.
**Requirements**: LINK-01, LINK-02, LINK-03, LINK-04, LINK-05, LINK-06, LINK-07, LINK-08

**Success Criteria**:

1. Labeled OSC8 links open their actual HTTP(S) destination, and users can preview or copy the destination before opening.
2. Bare HTTP(S) URLs, including soft-wrapped URLs, retain valid characters without surrounding prose punctuation.
3. Link metadata remains attached to the correct cells through scrolling, scrollback, wrapping, resizing, overwrites, and erasure in local and attached remote terminals.
4. A documented explicit gesture activates links while selection, drag-copy, scrolling, and supported child mouse behavior remain usable; output alone never opens a link.
5. Unsupported, malformed, or control-bearing targets remain non-activatable; allowed targets are passed as data to the opener, and failures leave the session running with a useful error.

**Plans**: TBD
**UI hint**: yes

### Phase 11: Negotiated Multiline Input

**Goal**: Users can insert newlines with Shift+Enter on verified terminal paths without changing existing key behavior or leaking terminal modes.
**Depends on**: Phase 8 for isolated input/lifecycle tests; not technically dependent on Phase 10.
**Requirements**: TKEY-01, TKEY-02, TKEY-03, TKEY-04, TKEY-05

**Success Criteria**:

1. Shift+Enter inserts a newline without submitting Claude/claudex prompts on documented, tested terminal paths.
2. Ordinary Enter, Ctrl-C, navigation keys, and existing baude shortcuts retain their behavior.
3. Terminals that cannot distinguish Shift+Enter retain legacy behavior, with documented setup or fallback guidance.
4. The prior outer-terminal keyboard mode is restored on controlled exit, failure, and suspend paths, then re-established on resume.
5. Negotiation cannot block startup/input indefinitely, and enhanced sequences are sent only on a verified outer-terminal/child-input path.

**Plans**: TBD

### Phase 12: Validation and v2.2.0 Release

**Goal**: Maintainers have observed regression, CI, documentation, and terminal evidence sufficient to publish v2.2.0 through the existing release process.
**Depends on**: Phases 8, 9, 10, and 11.
**Requirements**: SHIP-01, SHIP-02, SHIP-03, SHIP-04

**Success Criteria**:

1. Focused and workspace regression tests, formatting, clippy, and supported-platform CI checks pass after test isolation is in place.
2. Users can find documented activation/preview/copy gestures, tested terminal support, multiline setup/fallback, and lock recovery.
3. Real macOS/Linux terminal smoke evidence covers links, selection, scrollback, mouse behavior, Shift+Enter, ordinary Enter, and restoration.
4. v2.2.0 can be published through the existing release workflow with matching versions, release notes, and supported binary/container outputs, only after verification passes.

**Plans**: TBD

## Progress

**Execution Order**: Phase 8 → Phase 9 → Phase 10 → Phase 11 → Phase 12

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 8. Test Isolation and Fixture Ownership (narrowed) | 8/8 | In Progress|  |
| 9. Hook Seeding Safety (narrowed) | 3/4 | In Progress|  |
| 10. Clickable Terminal Links | 0/TBD | Not started | - |
| 11. Negotiated Multiline Input | 0/TBD | Not started | - |
| 12. Validation and v2.2.0 Release | 0/TBD | Not started | - |

## Planning Notes

- The existing vt100 screen model remains authoritative. A pinned parser fork is an option to verify, not a mandated dependency.
- Link metadata follows one grid through local and remote paths, not a second terminal model.
- Outer terminal keyboard negotiation stays separate from child input encoding.
- Repository/worktree test effects must stay inside injected fixture roots, never real user HOME/XDG directories.
- Historical leak cleanup is preview-only unless ownership and approval are separately verified; normal cleanup of newly created owned fixtures remains automatic.
- No SIGKILL or power-loss restoration guarantee is claimed.
- Release approval requires observed tests, CI, and terminal smoke evidence.
- A closed GitHub issue does not retire a requirement. Phases 8 and 9 were narrowed only after each requirement was re-verified against code on main; see REQUIREMENTS.md for per-requirement evidence.
- Proxy monitoring, extra URL schemes, forced lock takeover, full terminal-engine replacement, and PWA redesign remain out of scope.

## Backlog

See `.planning/BACKLOG.md`:

- **BL-01** — sidebar "idle"/status accuracy (addressed by v0.7 Phase 2; confirm in UAT)
- **BL-02** — model / permission-mode / planning-mode not shown for every session (Phase 1 follow-up)
- **BL-03** — wire GSD phase/state into the sidebar (new feature idea)

---
*Last updated: 2026-09-13. Phases 8 and 9 re-scoped against shipped v2.1.2-v2.1.5 code.*
