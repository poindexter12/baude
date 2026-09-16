# Phase 12 Smoke Evidence — v2.2.0

SHIP-03's evidence. Each session below records what a maintainer personally
observed in a real terminal, against a named binary built from a named commit.

**How to fill this file.** The **Observed (verbatim)** column takes what the
terminal actually printed or did — the literal text on screen, the actual
behavior. It does not take a restatement of the **What to look for** cell; an
Observed cell that paraphrases its own expectation is a fabricated pass, not
evidence. A leg that was not run is recorded as DEFERRED in the Result column
with a matching signed, dated entry under that session's `### Honest gaps and
deferrals`. A deferral with sign-off is a valid outcome. A fabricated pass is
not.

Modeled on `.planning/milestones/v2.0-phases/07-local-tui-dogfood-release/07-UAT-EVIDENCE.md`,
whose provenance header and honest-gaps closer this file reuses.

---

## Session: 2026-09-16 — macOS / TERMINAL_PENDING

**Commit:** PENDING (stamped in Task 2)
**Host/toolchain:** PENDING (stamped in Task 2)
**Binary under test:** PENDING (stamped in Task 2)
**Terminal identity:** PENDING — confirmed by the observer at fill time
**Observer:** PENDING — confirmed by the observer at fill time

| # | Leg | What to do | What to look for | Observed (verbatim) | Result |
|---|-----|------------|------------------|---------------------|--------|
| 1 | Links — detect | Emit an OSC 8 hyperlink and a bare URL in a pane, then press `ctrl+o` | The link overlay opens, titled ` links `, with the footer `enter opens · c/y copies · j/k moves · esc closes — N links` |  | ☐ pass ☐ fail ☐ n/a |
| 2 | Links — preview | Move down and up the list with `j` and `k` | Each row shows the actual destination URL rather than the display label; a long URL truncates in the middle with scheme and host preserved |  | ☐ pass ☐ fail ☐ n/a |
| 3 | Links — copy | Press `c` or `y` on the highlighted row | The full URL lands on the system clipboard; a clipboard failure is reported on the status line rather than silently claiming success |  | ☐ pass ☐ fail ☐ n/a |
| 4 | Links — open | Press `enter` on the highlighted row | The system opener launches; if the opener fails, the failure surfaces and the baude session stays alive |  | ☐ pass ☐ fail ☐ n/a |
| 5 | Selection | Drag-select text in a pane, then repeat the drag holding the terminal's override modifier (commonly Shift, or Option on macOS) | Plain drag is consumed by baude and copies baude's own pane selection on release; modifier-drag performs the terminal's native selection |  | ☐ pass ☐ fail ☐ n/a |
| 6 | Scrollback | Scroll the pane with the wheel, then quit baude and scroll the host terminal up | In-app scroll moves the pane's own scrollback; after exit the host terminal's scrollback holds no baude session output |  | ☐ pass ☐ fail ☐ n/a |
| 7 | Mouse | Wheel up and down, left-click, and left-drag, in the sidebar and then in a pane | No stray escape bytes reach the child process and no visual corruption appears in either region |  | ☐ pass ☐ fail ☐ n/a |
| 8 | Shift+Enter | In a claude pane, press `shift+enter` | A newline is inserted into the composer and nothing is submitted |  | ☐ pass ☐ fail ☐ n/a |
| 9 | Ordinary Enter | In the same pane, press `enter` | The input submits, behavior unchanged (TKEY-02) |  | ☐ pass ☐ fail ☐ n/a |
| 10 | Restoration — normal exit | Quit with `q` from the sidebar | The shell prompt returns with echo on, mouse reporting off, and no leftover control bytes printed |  | ☐ pass ☐ fail ☐ n/a |
| 11 | Restoration — interrupt or forced kill | Send `ctrl+c`, or kill the process, then run a bare command such as `echo hello` | The same restoration as leg 10; the bare command runs and displays normally, confirming the terminal is sane |  | ☐ pass ☐ fail ☐ n/a |
| 12 | Restoration — alternate-screen residue | After exit, scroll the host terminal up past the launch point | The pre-baude terminal content is intact, with no alternate-screen bleed |  | ☐ pass ☐ fail ☐ n/a |

### Honest gaps and deferrals

Format — one line per deferred leg, and the Result cell for that leg reads DEFERRED:

- `<leg #>, macOS: DEFERRED — <reason>. Signed off by <name> on <YYYY-MM-DD>.`

---

## Session: PENDING — Linux / TERMINAL_PENDING

**Commit:** PENDING — stamped by whoever runs this session
**Host/toolchain:** PENDING — stamped by whoever runs this session
**Binary under test:** PENDING — stamped by whoever runs this session
**Terminal identity:** PENDING — confirmed by the observer at fill time
**Observer:** PENDING — confirmed by the observer at fill time

**Linux coverage note (D-03).** Legs 1-4 and 8-9 have automated coverage on
`ubuntu-22.04` in the `check` job, so a Linux row for those legs may record the
`check (ubuntu-22.04)` CI run URL as its observation instead of a live terminal
session. Legs 5-7 and 10-12 are outer-terminal behaviors with no automated
proxy — they require a real Linux terminal session, and if none is available
they are deferred with a named signer and a date. Deferring them does not
invalidate this artifact; claiming them unobserved would.

| # | Leg | What to do | What to look for | Observed (verbatim) | Result |
|---|-----|------------|------------------|---------------------|--------|
| 1 | Links — detect | Emit an OSC 8 hyperlink and a bare URL in a pane, then press `ctrl+o` | The link overlay opens, titled ` links `, with the footer `enter opens · c/y copies · j/k moves · esc closes — N links` |  | ☐ pass ☐ fail ☐ n/a |
| 2 | Links — preview | Move down and up the list with `j` and `k` | Each row shows the actual destination URL rather than the display label; a long URL truncates in the middle with scheme and host preserved |  | ☐ pass ☐ fail ☐ n/a |
| 3 | Links — copy | Press `c` or `y` on the highlighted row | The full URL lands on the system clipboard; a clipboard failure is reported on the status line rather than silently claiming success |  | ☐ pass ☐ fail ☐ n/a |
| 4 | Links — open | Press `enter` on the highlighted row | The system opener launches; if the opener fails, the failure surfaces and the baude session stays alive |  | ☐ pass ☐ fail ☐ n/a |
| 5 | Selection | Drag-select text in a pane, then repeat the drag holding the terminal's override modifier (commonly Shift) | Plain drag is consumed by baude and copies baude's own pane selection on release; modifier-drag performs the terminal's native selection |  | ☐ pass ☐ fail ☐ n/a |
| 6 | Scrollback | Scroll the pane with the wheel, then quit baude and scroll the host terminal up | In-app scroll moves the pane's own scrollback; after exit the host terminal's scrollback holds no baude session output |  | ☐ pass ☐ fail ☐ n/a |
| 7 | Mouse | Wheel up and down, left-click, and left-drag, in the sidebar and then in a pane | No stray escape bytes reach the child process and no visual corruption appears in either region |  | ☐ pass ☐ fail ☐ n/a |
| 8 | Shift+Enter | In a claude pane, press `shift+enter` | A newline is inserted into the composer and nothing is submitted |  | ☐ pass ☐ fail ☐ n/a |
| 9 | Ordinary Enter | In the same pane, press `enter` | The input submits, behavior unchanged (TKEY-02) |  | ☐ pass ☐ fail ☐ n/a |
| 10 | Restoration — normal exit | Quit with `q` from the sidebar | The shell prompt returns with echo on, mouse reporting off, and no leftover control bytes printed |  | ☐ pass ☐ fail ☐ n/a |
| 11 | Restoration — interrupt or forced kill | Send `ctrl+c`, or kill the process, then run a bare command such as `echo hello` | The same restoration as leg 10; the bare command runs and displays normally, confirming the terminal is sane |  | ☐ pass ☐ fail ☐ n/a |
| 12 | Restoration — alternate-screen residue | After exit, scroll the host terminal up past the launch point | The pre-baude terminal content is intact, with no alternate-screen bleed |  | ☐ pass ☐ fail ☐ n/a |

### Honest gaps and deferrals

Format — one line per deferred leg, and the Result cell for that leg reads DEFERRED:

- `<leg #>, Linux: DEFERRED — <reason>. Signed off by <name> on <YYYY-MM-DD>.`

---

## Reference

The documented behavior these legs check against lives in `README.md`:
`### Link hints` (legs 1-4), `### Mouse, selection, and scrollback` (legs 5-7
and 12), `### Shift+Enter newlines` (legs 8-9), and `### Tested terminals`,
which promises that each release records exact terminal identities and OS
versions here.
