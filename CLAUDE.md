# baude

Cargo workspace: `baude-core` (no UI deps), `baude` (ratatui TUI), `bauded` (axum daemon).

## Gate

Run `scripts/gate.sh` before every push. It is the `check` job of `ci.yml`, in order, with unpiped
exit codes. `scripts/gate.sh --linux` runs the same gate in `rust:1-bookworm` as uid 1000; use it
for anything touching platform code (`cfg`, procfs, file timing), because macOS has hidden
Linux-only defects before. Never judge a gate step by piping it to `tail` or `grep` and reading `$?`.

## Releases

release-please owns versions, tags and the GitHub Release. Merging conventional commits to `main`
opens a release PR: patch bumps auto-merge once required checks pass, minor bumps after a soak,
and majors are merged by hand. `main` is protected, so every change lands by PR.

Never nest parentheses in a commit body, squash message or PR title. release-please silently
skips such a commit and no release PR opens. `scripts/check-commit-message.sh` catches it; enable
it locally with `git config core.hooksPath .githooks`.

## Tests

Escaped bugs here come from fixtures that model a friendlier world than production, not from low
coverage. Build fixtures in the shapes real users produce (cloned vs. `git init` repos, stale
runtime rows, command doubles that accept every appended argument), and assert on fixture output,
never by grepping source.
