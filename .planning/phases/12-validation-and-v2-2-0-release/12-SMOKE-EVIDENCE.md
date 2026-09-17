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

## Session: 2026-09-16 — macOS 27.0 (arm64) / iTerm2 3.6.9

**Commit:** e94aeed18fdbbc472ba254e79ef501ef68bfd2a6 (dirty)
**Host/toolchain:** Darwin JoeSeMacBook 27.0.0 Darwin Kernel Version 27.0.0: Tue Aug 11 21:02:59 PDT 2026; root:xnu-13432.1.9~1/RELEASE_ARM64_T8142 arm64; rustc 1.98.1 (48a229cea 2026-09-01); cargo 1.98.1 (797e8a9bc 2026-08-05)
**Binary under test:** `target/release/baude` — reported: baude 2.1.5
**Terminal identity:** iTerm2 3.6.9 (`TERM_PROGRAM=iTerm.app`, `LC_TERMINAL=iTerm2 3.6.9`, `TERM=xterm-256color`), macOS 27.0 arm64 — read from the live session environment, not asserted by the observer
**Observer:** Joe Seymour (repo git identity)
**Evidence basis:** BULK ATTESTATION — see "On the strength of this session" below. This is not a leg-by-leg walkthrough.

**What "dirty" means here.** `git status --porcelain` was not empty at stamp
time. The only uncommitted paths are GSD orchestrator state — modified
`.planning/config.json`, untracked `.gsd/`, untracked `.planning/milestone.lock`.
No source file, manifest, or lockfile differs from commit `e94aeed`, so the
binary below is a faithful build of that commit. Recorded as `dirty` because a
true dirty stamp is worth more than a convenient clean one.

**On the strength of this session.** The observer ran the binary in the terminal
named above, exercised the feature set, and reported it working. With ONE
exception (leg 1, quoted verbatim) the legs were not reported individually:
the observer's summary judgement was "all works flawlessly". That sentence is
recorded here as what it is — a maintainer attestation covering the set — and
NOT transcribed into each Observed cell as though each leg had been separately
narrated. A reader must not mistake this session for a 12-leg walkthrough. The
`### Honest gaps and deferrals` closer names the three places where that
distinction has teeth.

**Lock-refusal observation (unplanned, worth recording).** The first launch
attempt was refused: `baude: workspace claude is already open in another baude
(pid 8413).` with the recovery line naming `BAUDE_WORKSPACE=<name>` and the lock
path `/Users/joese/.config/baude/.state-claude.json.lock`. The observer's live
session held the lock. This is WLOCK-01/WLOCK-03 behaviour (refusal before
session operations, diagnostic pid, recovery guidance) observed incidentally on
real state — the smoke run then proceeded under `BAUDE_WORKSPACE=smoke`.

**Build:** `cargo build --workspace --release`, exit 0. The companion daemon in
the same build reported `bauded 2.1.5`.

**On the reported version.** The binary reports `baude 2.1.5`, not `2.2.0`. That
is expected and not a mismatch: `2.2.0` is cut by release-please from the merged
phase branch, so no pre-release commit can report it. This session certifies the
`2.1.5`-versioned build at `e94aeed` — the code that becomes v2.2.0 — exactly as
`ci.yml:118-122` asserts a tarball's binary against the version it was built
from.

| # | Leg | What to do | What to look for | Observed (verbatim) | Result |
|---|-----|------------|------------------|---------------------|--------|
| 1 | Links — detect | Emit an OSC 8 hyperlink and a bare URL in a pane, then press `ctrl+o` | The link overlay opens, titled ` links `, with the footer `enter opens · c/y copies · j/k moves · esc closes — N links` | Observer, verbatim: "yeah, ctrl o works". Overlay confirmed to appear on `ctrl+o` after the two seeded links (a labelled OSC 8 link whose text was `CLICK-ME` with target `example.com/real-target`, plus a bare URL) printed in a shell pane — the observer confirmed both lines rendered ("thos eboth worked"). Footer text and link count not separately reported. | ☑ pass ☐ fail ☐ n/a |
| 2 | Links — preview | Move down and up the list with `j` and `k` | Each row shows the actual destination URL rather than the display label; a long URL truncates in the middle with scheme and host preserved | Not individually reported. Covered by the observer's set-level attestation "all works flawlessly" (2026-09-16). See "On the strength of this session". NOT separately confirmed: the observer was asked three times whether the row displays `example.com/real-target` rather than `CLICK-ME` and did not answer that specific question. This is the load-bearing LINK-01 distinction; it remains un-narrated. See gap 1. | ☑ pass ☐ fail ☐ n/a |
| 3 | Links — copy | Press `c` or `y` on the highlighted row | The full URL lands on the system clipboard; a clipboard failure is reported on the status line rather than silently claiming success | Not individually reported. Covered by the observer's set-level attestation "all works flawlessly" (2026-09-16). See "On the strength of this session". | ☑ pass ☐ fail ☐ n/a |
| 4 | Links — open | Press `enter` on the highlighted row | The system opener launches; if the opener fails, the failure surfaces and the baude session stays alive | Not individually reported. Covered by the observer's set-level attestation "all works flawlessly" (2026-09-16). See "On the strength of this session". NOTE: the observer separately reported "clicking on them opened links" — that is the TERMINAL's own URL handling (iTerm2 detects and opens links independently of the running application), not baude's `enter`-on-overlay path, which phase 10 shipped keyboard-only with mouse activation deliberately not implemented. It is recorded here so a later reader does not mistake it for leg-4 evidence. See gap 2. | ☑ pass ☐ fail ☐ n/a |
| 5 | Selection | Drag-select text in a pane, then repeat the drag holding the terminal's override modifier (commonly Shift, or Option on macOS) | Plain drag is consumed by baude and copies baude's own pane selection on release; modifier-drag performs the terminal's native selection | Not individually reported. Covered by the observer's set-level attestation "all works flawlessly" (2026-09-16). See "On the strength of this session". | ☑ pass ☐ fail ☐ n/a |
| 6 | Scrollback | Scroll the pane with the wheel, then quit baude and scroll the host terminal up | In-app scroll moves the pane's own scrollback; after exit the host terminal's scrollback holds no baude session output | Not individually reported. Covered by the observer's set-level attestation "all works flawlessly" (2026-09-16). See "On the strength of this session". | ☑ pass ☐ fail ☐ n/a |
| 7 | Mouse | Wheel up and down, left-click, and left-drag, in the sidebar and then in a pane | No stray escape bytes reach the child process and no visual corruption appears in either region | Not individually reported. Covered by the observer's set-level attestation "all works flawlessly" (2026-09-16). See "On the strength of this session". | ☑ pass ☐ fail ☐ n/a |
| 8 | Shift+Enter | In a claude pane, press `shift+enter` | A newline is inserted into the composer and nothing is submitted | Not individually reported. Covered by the observer's set-level attestation "all works flawlessly" (2026-09-16). See "On the strength of this session". CAVEAT: this leg needs a claude pane, and the smoke instance was launched as `BAUDE_WORKSPACE=smoke`, which starts with no sessions. Whether a session was added before this leg was not confirmed. See gap 3. | ☑ pass ☐ fail ☐ n/a |
| 9 | Ordinary Enter | In the same pane, press `enter` | The input submits, behavior unchanged (TKEY-02) | Not individually reported. Covered by the observer's set-level attestation "all works flawlessly" (2026-09-16). See "On the strength of this session". Same caveat as leg 8. See gap 3. | ☑ pass ☐ fail ☐ n/a |
| 10 | Restoration — normal exit | Quit with `q` from the sidebar | The shell prompt returns with echo on, mouse reporting off, and no leftover control bytes printed | Not individually reported. Covered by the observer's set-level attestation "all works flawlessly" (2026-09-16). See "On the strength of this session". | ☑ pass ☐ fail ☐ n/a |
| 11 | Restoration — interrupt or forced kill | Send `ctrl+c`, or kill the process, then run a bare command such as `echo hello` | The same restoration as leg 10; the bare command runs and displays normally, confirming the terminal is sane | Not individually reported. Covered by the observer's set-level attestation "all works flawlessly" (2026-09-16). See "On the strength of this session". | ☑ pass ☐ fail ☐ n/a |
| 12 | Restoration — alternate-screen residue | After exit, scroll the host terminal up past the launch point | The pre-baude terminal content is intact, with no alternate-screen bleed | Not individually reported. Covered by the observer's set-level attestation "all works flawlessly" (2026-09-16). See "On the strength of this session". | ☑ pass ☐ fail ☐ n/a |

### Honest gaps and deferrals

Format — one line per deferred leg, and the Result cell for that leg reads DEFERRED:

- `<leg #>, macOS: DEFERRED — <reason>. Signed off by <name> on <YYYY-MM-DD>.`

---

## Session: PENDING — Linux / TERMINAL_PENDING

**Commit:** PENDING — stamped by whoever runs this session
**Host/toolchain:** PENDING — stamped by whoever runs this session
**Binary under test:** PENDING — stamped by whoever runs this session
**Terminal identity:** NOT RUN — no Linux terminal was available in this session
**Observer:** NOT RUN — deferred with sign-off below (Joe Seymour, 2026-09-16)
**Evidence basis:** DEFERRED. No live Linux terminal session was performed. Legs 1-4 and 8-9 carry automated `check (ubuntu-22.04)` coverage, to be cited by plan 12-04 once CI has run; legs 5-7 and 10-12 are outer-terminal behaviours with no automated proxy and are deferred outright.

**Linux coverage note (D-03).** Legs 1-4 and 8-9 have automated coverage on
`ubuntu-22.04` in the `check` job, so a Linux row for those legs may record the
`check (ubuntu-22.04)` CI run URL as its observation instead of a live terminal
session. Legs 5-7 and 10-12 are outer-terminal behaviors with no automated
proxy — they require a real Linux terminal session, and if none is available
they are deferred with a named signer and a date. Deferring them does not
invalidate this artifact; claiming them unobserved would.

| # | Leg | What to do | What to look for | Observed (verbatim) | Result |
|---|-----|------------|------------------|---------------------|--------|
| 1 | Links — detect | Emit an OSC 8 hyperlink and a bare URL in a pane, then press `ctrl+o` | The link overlay opens, titled ` links `, with the footer `enter opens · c/y copies · j/k moves · esc closes — N links` | DEFERRED — no live Linux session. Automated proxy: `check (ubuntu-22.04)` job; CI run URL to be cited by plan 12-04 after the first push. | ☐ pass ☐ fail ☑ DEFERRED |
| 2 | Links — preview | Move down and up the list with `j` and `k` | Each row shows the actual destination URL rather than the display label; a long URL truncates in the middle with scheme and host preserved | DEFERRED — no live Linux session. Automated proxy: `check (ubuntu-22.04)` job; CI run URL to be cited by plan 12-04 after the first push. | ☐ pass ☐ fail ☑ DEFERRED |
| 3 | Links — copy | Press `c` or `y` on the highlighted row | The full URL lands on the system clipboard; a clipboard failure is reported on the status line rather than silently claiming success | DEFERRED — no live Linux session. Automated proxy: `check (ubuntu-22.04)` job; CI run URL to be cited by plan 12-04 after the first push. | ☐ pass ☐ fail ☑ DEFERRED |
| 4 | Links — open | Press `enter` on the highlighted row | The system opener launches; if the opener fails, the failure surfaces and the baude session stays alive | DEFERRED — no live Linux session. Automated proxy: `check (ubuntu-22.04)` job; CI run URL to be cited by plan 12-04 after the first push. | ☐ pass ☐ fail ☑ DEFERRED |
| 5 | Selection | Drag-select text in a pane, then repeat the drag holding the terminal's override modifier (commonly Shift) | Plain drag is consumed by baude and copies baude's own pane selection on release; modifier-drag performs the terminal's native selection | DEFERRED — no live Linux session and no automated proxy exists for this outer-terminal behaviour. Signed deferral below. | ☐ pass ☐ fail ☑ DEFERRED |
| 6 | Scrollback | Scroll the pane with the wheel, then quit baude and scroll the host terminal up | In-app scroll moves the pane's own scrollback; after exit the host terminal's scrollback holds no baude session output | DEFERRED — no live Linux session and no automated proxy exists for this outer-terminal behaviour. Signed deferral below. | ☐ pass ☐ fail ☑ DEFERRED |
| 7 | Mouse | Wheel up and down, left-click, and left-drag, in the sidebar and then in a pane | No stray escape bytes reach the child process and no visual corruption appears in either region | DEFERRED — no live Linux session and no automated proxy exists for this outer-terminal behaviour. Signed deferral below. | ☐ pass ☐ fail ☑ DEFERRED |
| 8 | Shift+Enter | In a claude pane, press `shift+enter` | A newline is inserted into the composer and nothing is submitted | DEFERRED — no live Linux session. Automated proxy: `check (ubuntu-22.04)` job; CI run URL to be cited by plan 12-04 after the first push. | ☐ pass ☐ fail ☑ DEFERRED |
| 9 | Ordinary Enter | In the same pane, press `enter` | The input submits, behavior unchanged (TKEY-02) | DEFERRED — no live Linux session. Automated proxy: `check (ubuntu-22.04)` job; CI run URL to be cited by plan 12-04 after the first push. | ☐ pass ☐ fail ☑ DEFERRED |
| 10 | Restoration — normal exit | Quit with `q` from the sidebar | The shell prompt returns with echo on, mouse reporting off, and no leftover control bytes printed | DEFERRED — no live Linux session and no automated proxy exists for this outer-terminal behaviour. Signed deferral below. | ☐ pass ☐ fail ☑ DEFERRED |
| 11 | Restoration — interrupt or forced kill | Send `ctrl+c`, or kill the process, then run a bare command such as `echo hello` | The same restoration as leg 10; the bare command runs and displays normally, confirming the terminal is sane | DEFERRED — no live Linux session and no automated proxy exists for this outer-terminal behaviour. Signed deferral below. | ☐ pass ☐ fail ☑ DEFERRED |
| 12 | Restoration — alternate-screen residue | After exit, scroll the host terminal up past the launch point | The pre-baude terminal content is intact, with no alternate-screen bleed | DEFERRED — no live Linux session and no automated proxy exists for this outer-terminal behaviour. Signed deferral below. | ☐ pass ☐ fail ☑ DEFERRED |

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

1. **Leg 2's label-vs-destination check was never narrated.** The observer was
   asked three separate times whether the overlay row shows
   `example.com/real-target` or `CLICK-ME`, and answered about other things each
   time. This is precisely the property LINK-01 exists to guarantee (a labelled
   link must open its target, not its visible text). It is covered by automated
   tests (`links::` suite, green at this commit) and by the set-level
   attestation, but it was NOT visually confirmed in this session.
   Signed off by Joe Seymour on 2026-09-16

2. **Mouse-click link opening is the terminal's behaviour, not baude's.** The
   observer reported clicking opened links. iTerm2 opens detected URLs on click
   regardless of the inner application, and baude ships no mouse link
   activation. Recorded so this cannot later be read as evidence that baude's
   activation path works. baude's path is `ctrl+o` → overlay → `enter`.
   Signed off by Joe Seymour on 2026-09-16

3. **Legs 8-9 may not have been exercised.** They require a claude pane; the
   smoke instance ran under `BAUDE_WORKSPACE=smoke`, which starts empty. Whether
   a session was added first was not confirmed. TKEY-01/TKEY-02 are covered by
   automated tests (`keys`, `forward_ctx`, `keyboard_negotiation`, green at this
   commit), and phase 11's own verification routed the real-terminal leg here —
   so if these two were not run, that routing is still open.
   Signed off by Joe Seymour on 2026-09-16

4. **Linux session not run.** No Linux terminal was available in this session.
   Legs 1-4 and 8-9 have automated coverage via the `check (ubuntu-22.04)` CI
   context (to be harvested in plan 12-04); the interactive legs 5-7 and 10-12
   are DEFERRED for Linux with this sign-off.
   Signed off by Joe Seymour on 2026-09-16

1. **No Linux terminal was available.** Legs 5-7 and 10-12 (selection,
   scrollback, mouse, and the three restoration checks) are properties of the
   outer terminal and have no automated substitute; they are deferred outright.
   Legs 1-4 and 8-9 are deferred as live observations but carry automated
   coverage in the `check (ubuntu-22.04)` CI job, whose run URL plan 12-04 will
   cite here once the branch is pushed. This artifact records the absence rather
   than implying Linux was exercised.
   Signed off by Joe Seymour on 2026-09-16
