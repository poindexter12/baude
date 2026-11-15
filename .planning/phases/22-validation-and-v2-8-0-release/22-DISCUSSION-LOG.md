# Phase 22: Validation and v2.8.0 Release - Discussion Log

Audit trail for humans. No downstream agent reads this file. Smart discuss, autonomous mode, 2026-11-13.

## Shipping v2.8.0

| Question | Options offered | Answer |
|----------|-----------------|--------|
| How far should the autonomous run take the release? | PR and stop before merge, recommended; PR and squash-merge; stop before any push | PR, stop before merge |
| Phases 18 to 20 have stale or missing verification. What happens before release? | Re-verify first, recommended; release without them | Re-verify first |
| Where does the intermittent macOS teardown 500 go? | Backlog it; fix in Phase 22; you decide | Backlog it |

## Decided autonomously

[auto] [Tests] — Q: "Where do the REL-01 tests live?" → Selected: "In-crate test modules" (reversible technical — decided autonomously: moving a test module is a local edit)
[auto] [Tests] — Q: "What matrix?" → Selected: "Four row kinds, live and no runtime, cloned and git init fixtures" (reversible technical — decided autonomously: test-only)
[auto] [Tests] — Q: "Browser automation for the PWA?" → Selected: "None; a Rust test on /events over real HTTP" (reversible technical — decided autonomously: no new dependency)
[auto] [Docs] — Q: "Where do the docs go?" → Selected: "README sections" (reversible technical — decided autonomously: prose edits)
[auto] [Release] — Q: "Changelog and version?" → Selected: "release-please owns both" (reversible technical — decided autonomously: nothing written by hand)
[auto] [Release] — Q: "Gates?" → Selected: "both gates on the final commit before any push" (reversible technical — decided autonomously: process only)
