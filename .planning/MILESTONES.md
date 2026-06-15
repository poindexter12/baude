# Milestones

Shipped history of baude, reconstructed from git tags and release notes for the
lean GSD scaffold (baude predates GSD tracking; pre-v0.7 milestones were not run
through GSD phases).

| Version | Theme | Shipped | Headline |
|---------|-------|---------|----------|
| v0.1.0 | Multi-session TUI | 2026-06-11 | Embedded PTYs, worktree sessions, persist/resume, `BAUDE_CLAUDE_CMD` |
| v0.2.0 | Sidebar UX | 2026-06-12 | Stable session order, in-place waiting flash, unified global chords, shell pane + editor key |
| v0.3.0 | Observability | 2026-06-12 | Live Claude metadata (model/context/mode/tokens/GSD), cost + rate-limit panel, `baude statusline` bridge |
| v0.4.0 | Remote daemon + PWA | 2026-06-12 | `bauded` (REST/SSE), containerized deploy behind Tailscale, phone PWA (triage list, chat, terminal peek) |
| v0.5.0 | Remote attach + push | 2026-06-12 | TUI raw-PTY attach to daemon sessions over WebSocket; Web Push on waiting/exited |
| v0.6.0–v0.6.1 | Archiving | 2026-06-13 | Idle-session auto-archive (30m) + manual archive everywhere; slim image; archive-bug fixes |

## Current

- **v0.7 — Native Claude integration** (in planning): replace inferred session
  state with first-party Claude Code data. Full plan: `docs/plans/tier-1-native-claude-integration.md`.

## Notes

- Pre-v0.7 work landed without GSD phases — the above is a record, not a set of
  GSD-verified milestones.
- Web Push (v0.5) has not yet been verified on a real phone (needs
  `tailscale serve` HTTPS + an installed PWA).
