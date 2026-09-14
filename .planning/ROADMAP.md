# Roadmap: baude

## Overview

v2.2 Reliability and Terminal Usability continues after completed Phase 7. Establish isolated test fixtures first, then deliver safe hook and workspace-lock behavior, clickable HTTP(S) terminal links, negotiated multiline input, and validation through the existing v2.2.0 release workflow.

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

## v2.2 Phases

- [ ] **Phase 8: Test Isolation and Fixture Ownership**: Confine tests and cleanup to owned temporary roots.
- [ ] **Phase 9: Hook Registration and Workspace Lock Diagnostics**: Reconcile owned hooks safely and report contention without takeover.
- [ ] **Phase 10: Clickable Terminal Links**: Expose safe link activation, destination preview, and copy using authoritative screen metadata.
- [ ] **Phase 11: Negotiated Multiline Input**: Support Shift+Enter where capability is verified, with honest fallback elsewhere.
- [ ] **Phase 12: Validation and v2.2.0 Release**: Complete regression, CI, terminal smoke, documentation, and release validation.

## Phase Details

### Phase 8: Test Isolation and Fixture Ownership

**Goal**: Developers can run repository and worktree tests concurrently without touching real user data or sharing fixture state.
**Depends on**: Nothing within v2.2. This phase precedes test-heavy milestone work.
**Requirements**: TISO-01, TISO-02, TISO-03, TISO-04

**Success Criteria**:
1. Every test-created repository, worktree, config, and state file is confined to a unique test-owned temporary root.
2. Concurrent tests do not modify parent HOME/XDG environment or share cached workspace identity between fixtures.
3. A tested creation path that escapes its fixture root fails while the real user data directory remains untouched.
4. Suspected historical leaks can be previewed without deletion; removal requires separate approval and verified ownership, not a missing gitdir alone.

**Plans**: TBD

### Phase 9: Hook Registration and Workspace Lock Diagnostics

**Goal**: Users retain their configuration and receive safe, actionable behavior when hooks or workspace locks require reconciliation.
**Depends on**: Phase 8 for isolated regression coverage. Hook and lock contracts are otherwise independent.
**Requirements**: HREG-01, HREG-02, HREG-03, HREG-04, WLOCK-01, WLOCK-02, WLOCK-03, WLOCK-04

**Success Criteria**:
1. Opening or reopening from different baude/bauded executable paths converges each lifecycle event to one recognized owned registration using the current executable.
2. Custom hooks, mixed groups, ambiguous registrations, unrelated settings, and malformed settings remain preserved, with an actionable warning when safe reconciliation is impossible.
3. Executables whose paths contain spaces or shell metacharacters are invoked exactly and safely, without duplicate registration.
4. A second instance reports explicit lock contention before session operations and cannot replace, remove, or interfere with the live owner's state or sessions.
5. Diagnostics identify the affected workspace/path, include a reliably recorded PID only as advisory metadata, provide recovery guidance, distinguish invalid state, and allow reopening after OS lock release even if the lock file remains.

**Plans**: TBD

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
| 8. Test Isolation and Fixture Ownership | 0/TBD | Not started | - |
| 9. Hook Registration and Workspace Lock Diagnostics | 0/TBD | Not started | - |
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
- Proxy monitoring, extra URL schemes, forced lock takeover, full terminal-engine replacement, and PWA redesign remain out of scope.

## Backlog

See `.planning/BACKLOG.md`:

- **BL-01** — sidebar "idle"/status accuracy (addressed by v0.7 Phase 2; confirm in UAT)
- **BL-02** — model / permission-mode / planning-mode not shown for every session (Phase 1 follow-up)
- **BL-03** — wire GSD phase/state into the sidebar (new feature idea)

---
*Last updated: 2026-09-08. v2.2 roadmap approval pending.*
