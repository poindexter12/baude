---
phase: "12"
slug: "validation-and-v2-2-0-release"
status: draft
nyquist_compliant: true
wave_0_complete: false
created: "2026-09-16"
---

# Phase 12 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test + cargo fmt/clippy + GitHub Actions CI parity |
| **Config file** | Cargo.toml, .github/workflows/ci.yml |
| **Quick run command** | `cargo clippy --workspace --all-targets -- -D warnings` |
| **Full suite command** | `cargo test --workspace --locked` (plus fmt, clippy, assert-real-roots-untouched bracket) |
| **Estimated runtime** | ~4.5 minutes serial (full bracket) |

---

## Sampling Rate

- **After every task commit:** Run the task's gate command
- **After every plan wave:** Run the full CI-parity bracket
- **Before `/gsd-verify-work`:** Bracket green, CI green on the pushed branch
- **Max feedback latency:** 300 seconds

Fourteen of the sixteen task gates are sub-second (grep, awk, `test`, or a single
`gh` API round trip). The two exceptions are `12-01-T2` and `12-04-T1`, which run
the full CI-parity ladder plus a locked release build — ~4.5 minutes serial for
the suite and a few more cold for the build. Those two are the phase's
deliberate long-latency gates: they exist precisely because CI runs the same
commands under `enforce_admins: true`, where the alternative feedback loop is a
full round trip.

---

## Per-Task Verification Map

Commands are given verbatim. Two reading notes:

- `$P` abbreviates the phase directory — set `P=.planning/phases/12-validation-and-v2-2-0-release` before running any command that uses it.
- Literal pipe characters inside a command are written `\|` so the markdown table renders them; drop the backslash when copying from source.

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 12-01-T1 | 12-01 | 1 | SHIP-01 | T-12-01, T-12-03 | Clippy gate exits 0 from a clean tree with the group relaxation confined to the vendored fork — no allow leaks into baude's own crates | lint + focused unit | `cargo clippy --all-targets -- -D warnings && cargo test -p baude links -- --test-threads=1 && cargo test -p vt100 --test link_fidelity && cargo test -p vt100 --test kitty_keyboard`<br>`! grep -rn --include='*.rs' -E 'allow\(clippy::(all\|pedantic\|nursery\|cargo)\)' baude/src baude-core/src bauded/src` | ✅ | ⬜ pending |
| 12-01-T2 | 12-01 | 1 | SHIP-01 | T-12-02 | Full CI-parity ladder green locally, with the workspace suite bracketed so it cannot reach the developer's four real roots | CI-parity bracket | `cargo fmt --check && cargo clippy --all-targets -- -D warnings && bash scripts/assert-real-roots-untouched.sh --self-test && bash scripts/assert-real-roots-untouched.sh before && cargo test -- --test-threads=1 && bash scripts/assert-real-roots-untouched.sh after && cargo build --workspace --release --locked` | ✅ | ⬜ pending |
| 12-01-T3 | 12-01 | 1 | SHIP-01 | — | The phase-11 deferred ledger records the real cause of the red clippy gate instead of a wrong one a future reader would trust | docs grep | `! grep -niE 'presumed green' .planning/phases/11-negotiated-multiline-input/deferred-items.md`<br>`grep -nE 'links\.rs' .planning/phases/11-negotiated-multiline-input/deferred-items.md && grep -nE 'ui\.rs' .planning/phases/11-negotiated-multiline-input/deferred-items.md && grep -nE '12-01\|phase 12\|Phase 12' .planning/phases/11-negotiated-multiline-input/deferred-items.md` | ✅ | ⬜ pending |
| 12-02-T1 | 12-02 | 1 | SHIP-02 | T-12-05 | The link gesture, what preview actually shows, and the copy gesture are findable by a user who has not read the source | docs grep | `grep -n 'ctrl+o' README.md && grep -niE 'link hints' README.md && grep -niE 'c/y\|copies' README.md && grep -niE 'inspect\|preview\|actual destination' README.md`<br>`grep -c 'ctrl+o' README.md` | ✅ | ⬜ pending |
| 12-02-T2 | 12-02 | 1 | SHIP-02 | T-12-05 | Unconditional mouse capture, the selection workaround, and the alternate-screen consequence are documented; the tested-terminal statement covers the gestures as a set; SHIP-02's Shift+Enter item survives the edit to its own subsection | docs grep + paragraph-anchored gate | `grep -niE 'mouse' README.md && grep -niE 'scrollback\|alternate screen' README.md`<br>`awk 'BEGIN{RS=""} /[Mm]odifier/ && /[Dd]rag\|[Ss]elect/ {f=1} END{exit !f}' README.md`<br>`grep -niE 'shift\+enter' README.md && grep -niE 'terminal-setup\|ctrl\+j\|extended-keys' README.md && awk 'BEGIN{RS=""} /Ghostty\|WezTerm\|Alacritty/ && /[Ll]ink\|[Mm]ouse/ {f=1} END{exit !f}' README.md` | ✅ | ⬜ pending |
| 12-02-T3 | 12-02 | 1 | SHIP-02 | T-12-04, T-12-05 | Lock recovery is findable by the exact diagnostic string, and the one instruction that would produce two writers on one state file is absent | docs grep + negative gate | `grep -niE 'already open in another baude' README.md && grep -nE '\.state-.*\.lock' README.md && grep -niE 'BAUDE_WORKSPACE=' README.md && grep -niE 'never delete the lock file' README.md && grep -niE 'ps -p\|lsof' README.md`<br>`! grep -nE '\brm\b.*\.lock' README.md` | ✅ | ⬜ pending |
| 12-03-T1 | 12-03 | 2 | SHIP-03 | T-12-07 | The evidence template is structurally complete for both operating systems and carries zero pre-filled observations | structure gate + pre-fill gate | `test -f $P/12-SMOKE-EVIDENCE.md && test "$(grep -cE '^\| *[0-9]+ *\|' $P/12-SMOKE-EVIDENCE.md)" = "24" && test "$(grep -cE '^## Session:' $P/12-SMOKE-EVIDENCE.md)" = "2" && grep -nE '^### Honest gaps and deferrals' $P/12-SMOKE-EVIDENCE.md`<br>`awk -F'\|' '/^\| *[0-9]+ *\|/ {obs=$6; gsub(/^[ \t]+\|[ \t]+$/,"",obs); if (obs!="") {print "PRE-FILLED: " $0; bad=1}} END {exit bad+0}' $P/12-SMOKE-EVIDENCE.md` | ✅ | ⬜ pending |
| 12-03-T2 | 12-03 | 2 | SHIP-03 | T-12-08, T-12-09 | The macOS session certifies an exact commit and an exact binary by its own reported version string, and still carries zero observations | provenance gate + pre-fill gate | `grep -nE '^\*\*Commit:\*\* [0-9a-f]{7,40} \((clean\|dirty)\)' $P/12-SMOKE-EVIDENCE.md && grep -nE '^\*\*Binary under test:\*\*.*baude [0-9]+\.[0-9]+\.[0-9]+' $P/12-SMOKE-EVIDENCE.md && grep -nE '^\*\*Host/toolchain:\*\*.*rustc' $P/12-SMOKE-EVIDENCE.md`<br>`awk -F'\|' '/^\| *[0-9]+ *\|/ {obs=$6; gsub(/^[ \t]+\|[ \t]+$/,"",obs); if (obs!="") {print "PRE-FILLED: " $0; bad=1}} END {exit bad+0}' $P/12-SMOKE-EVIDENCE.md` | ✅ | ⬜ pending |
| 12-03-T3 | 12-03 | 2 | SHIP-03 | T-12-07 | Every leg is either observed and recorded verbatim or marked DEFERRED with a named signer and an ISO date — no leg is both unfilled and unaccounted for | completeness gate | `awk -F'\|' 'BEGIN{bad=0;deferred=0;signed=0} /^\| *[0-9]+ *\|/ {obs=$6; res=$7; gsub(/^[ \t]+\|[ \t]+$/,"",obs); gsub(/^[ \t]+\|[ \t]+$/,"",res); if (res ~ /DEFERRED/) {deferred=1} else if (obs=="") {print "UNFILLED LEG: " $0; bad=1}} /Signed off by .* on [0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]/ {signed=1} END {if (deferred && !signed) {print "DEFERRED legs present with no signed deferral line"; bad=1} exit bad}' $P/12-SMOKE-EVIDENCE.md` | ✅ | ⬜ pending |
| 12-04-T1 | 12-04 | 3 | SHIP-01 | T-12-11 | The post-rebase tree — which nobody has tested — is green on the full ladder and a locked release build before it is pushed, and the remote carries exactly what CI will judge | CI-parity bracket + git state gate | `cargo fmt --check && cargo clippy --all-targets -- -D warnings && bash scripts/assert-real-roots-untouched.sh --self-test && bash scripts/assert-real-roots-untouched.sh before && cargo test -- --test-threads=1 && bash scripts/assert-real-roots-untouched.sh after && cargo build --workspace --release --locked`<br>`git fetch origin && git merge-base --is-ancestor origin/main HEAD && test -n "$(git rev-parse --abbrev-ref '@{upstream}' 2>/dev/null)" && test "$(git rev-list --count '@{upstream}..HEAD')" = "0"` | ✅ | ⬜ pending |
| 12-04-T2 | 12-04 | 3 | SHIP-01 | T-12-10, T-12-12 | All three required status contexts concluded SUCCESS under `enforce_admins: true`, and the run URLs are recorded as SHIP-01's two-OS evidence | GitHub API gate + checklist gate | `gh pr view --json number,url,state --jq 'if .state == "OPEN" then .url else error("no open PR for this branch") end'`<br>`gh pr view --json statusCheckRollup --jq '[.statusCheckRollup[] \| select(.name=="check (macos-14)" or .name=="check (ubuntu-22.04)" or .name=="docker")] \| if length == 3 and all(.conclusion=="SUCCESS") then "ok" else error("required contexts not all green: \(map({name,conclusion}))") end'`<br>`grep -nE '^## CI evidence \(SHIP-01\)' $P/12-RELEASE-CHECKLIST.md && grep -cE 'https://github\.com/.*/actions/runs/[0-9]+' $P/12-RELEASE-CHECKLIST.md` | ✅ | ⬜ pending |
| 12-04-T3 | 12-04 | 3 | SHIP-04 | T-12-15 | The pre-release checklist exists with the phase-8 re-stamp, the asset set, the target version, and the vendored-fork exclusion — and nothing is pre-ticked on the maintainer's behalf | checklist gate | `grep -nE '^## Pre-release checklist' $P/12-RELEASE-CHECKLIST.md && grep -nE 'gsd-verify-work 8' $P/12-RELEASE-CHECKLIST.md && grep -nE 'SHA256SUMS' $P/12-RELEASE-CHECKLIST.md && grep -nE '2\.2\.0' $P/12-RELEASE-CHECKLIST.md && grep -niE 'vendor/vt100' $P/12-RELEASE-CHECKLIST.md`<br>`test "$(grep -cE '^- \[ \]' $P/12-RELEASE-CHECKLIST.md)" != "0"` | ✅ | ⬜ pending |
| 12-05-T1 | 12-05 | 4 | SHIP-04 | T-12-14, T-12-17 | The merge is authorized by a human, with the three required contexts re-checked at gate time rather than trusted from the recorded evidence | GitHub API gate + human-check | `gh pr view --json state,statusCheckRollup --jq 'if .state != "OPEN" then error("PR is not open") elif ([.statusCheckRollup[] \| select(.name=="check (macos-14)" or .name=="check (ubuntu-22.04)" or .name=="docker")] \| length == 3 and all(.conclusion=="SUCCESS")) then "ok" else error("required contexts are not all green at gate time") end'` | ✅ | ⬜ pending |
| 12-05-T2 | 12-05 | 4 | SHIP-04 | T-12-14, T-12-15 | `release:hold` is actually present on the open `release:minor` PR well inside the soak window, and that PR proposes 2.2.0 — so the `*/30` cron cannot publish unauthorized | GitHub API gate + checklist gate | `gh pr list --state open --label 'release:minor' --json number,title,labels --jq 'if length == 0 then error("no open release:minor PR found") else (.[0] \| if ([.labels[].name] \| index("release:hold")) == null then error("release:hold is NOT applied to PR \(.number) — the soak cron can merge it") elif (.title \| test("2\\.2\\.0")) \| not then error("release PR \(.number) does not propose 2.2.0: \(.title)") else "held at 2.2.0" end) end'`<br>`grep -nE '^## Release PR' $P/12-RELEASE-CHECKLIST.md && grep -nE 'release:hold' $P/12-RELEASE-CHECKLIST.md` | ✅ | ⬜ pending |
| 12-05-T3 | 12-05 | 4 | SHIP-04 | T-12-17 | The publish decision is separately authorized, dated, and owned — addressed to a sentinel only the publish checkpoint writes, so Task 1's dated merge approval cannot stand in for it | checklist sentinel gate + human-check | `grep -nE '^## Publish Authorization' $P/12-RELEASE-CHECKLIST.md && grep -nE '^\*\*Publish authorized by:\*\* .+ on [0-9]{4}-[0-9]{2}-[0-9]{2}' $P/12-RELEASE-CHECKLIST.md` | ✅ | ⬜ pending |
| 12-05-T4 | 12-05 | 4 | SHIP-04 | T-12-16, T-12-18 | The published release is non-draft, non-prerelease, and carries the complete asset set — four tarballs plus the integrity file downloaders need | release asset gate + checklist gate | `gh release view v2.2.0 --json tagName,isDraft,isPrerelease,assets --jq 'if .isDraft or .isPrerelease then error("v2.2.0 is draft or prerelease") elif ([.assets[].name] \| map(select(test("\\.tar\\.gz$"))) \| length) < 4 then error("expected 4 tarballs, found \([.assets[].name])") elif ([.assets[].name] \| index("SHA256SUMS.txt")) == null then error("SHA256SUMS.txt is missing from the release assets") else "v2.2.0 published with \([.assets[].name] \| length) assets" end'`<br>`grep -nE '^## Published release' $P/12-RELEASE-CHECKLIST.md && grep -nE 'v2\.2\.0' $P/12-RELEASE-CHECKLIST.md` | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

*File Exists: whether the gate's target exists at gate time. `$P/12-SMOKE-EVIDENCE.md` and `$P/12-RELEASE-CHECKLIST.md` are created earlier in their own plans (12-03-T1 and 12-04-T2 respectively), so every gate above runs against a file that exists when it runs.*

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements — CI, release-please,
and the phase 8 real-roots bracket are in place. No task carries a `MISSING`
verification reference, so there is no Wave 0 scaffold to build and
`wave_0_complete` stays `false` by design rather than by omission.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Real macOS/Linux terminal smoke: links, selection, scrollback, mouse, Shift+Enter, Enter, restoration | SHIP-03 | Must be observed in a live terminal by the maintainer | Follow 12-SMOKE-EVIDENCE.md leg by leg; record terminal identity + date |
| Publish approval for v2.2.0 | SHIP-04 | Release decision is a human gate (release:hold label) | Approve only after verification passes and smoke evidence is recorded |

Both are backed by structural gates rather than left unchecked: `12-03-T3`
fails on any leg that is neither observed nor signed-off deferred, and
`12-05-T3` fails on a publish with no dated, owned authorization of its own.
Neither gate can check that an observation is *true* — which is why the
observer's name is recorded beside it.

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or are explicit manual-only rows — 16 of 16 tasks carry at least one `<automated>` gate; the two human checkpoints carry a structural gate alongside their `<human-check>`
- [x] Sampling continuity: no 3 consecutive tasks without automated verify — the longest run without one is zero
- [x] Wave 0 covers all MISSING references — there are none
- [x] No watch-mode flags — every command is single-shot; the suite runs `--test-threads=1`, not a watcher
- [x] Feedback latency < 300s — fourteen gates are sub-second; `12-01-T2` and `12-04-T1` run the full ladder (~4.5 min serial, longer cold with the locked release build) and are the recorded exception, taken because CI runs those same commands behind a merge gate that cannot be overridden
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** pending — `status` stays `draft` until `/gsd-validate-phase` flips it against observed results.
