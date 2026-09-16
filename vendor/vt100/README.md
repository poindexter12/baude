# vt100 (baude fork)

Vendored fork of the [`vt100`](https://crates.io/crates/vt100) crate with
OSC 8 hyperlink support. This crate parses a terminal byte stream and
provides an in-memory representation of the rendered contents; baude uses it
as the authoritative terminal-state model for every pane (local claude,
local shell, and remote attach).

## Provenance

| Field | Value |
|-------|-------|
| Upstream crate | `vt100` 0.15.2 by Jesse Luehrs |
| Upstream repo | <https://github.com/doy/vt100-rust> |
| Source of this copy | local cargo registry cache of the exact Cargo.lock-pinned, already-shipping `vt100 0.15.2` package (`index.crates.io`), copied verbatim at import (plan 10-01) |
| License | MIT (upstream `LICENSE` retained unmodified) |
| Formatting | `cargo fmt` run once on the vendored code at import |

## Why this fork exists

Upstream vt100 silently discards OSC 8 hyperlink sequences — its
`osc_dispatch` handles only OSC 0/1/2 (window title), and no upstream
release through 0.16.2 adds hyperlink support. baude's clickable-terminal-
links feature (Phase 10, LINK-01..LINK-08) needs per-cell link metadata that
inherits scrollback/wrap/resize/overwrite/erasure correctness from the grid
model, so the parser itself must carry it.

The crate keeps its upstream package name `vt100` and is consumed as a
workspace path dependency (`baude-core/Cargo.toml`), so the existing
`pub use vt100;` re-export in `baude-core/src/lib.rs` and every downstream
import remain unchanged.

## baude-specific diff surface

| File | Change |
|------|--------|
| `src/attrs.rs` | `Attrs.link: Option<u16>` — per-cell interned link id (rides on `Attrs`, so grid behaviors inherit correctness) |
| `src/screen.rs` | OSC 8 arm in `osc_dispatch` (`params[2..]` rejoined with `;`); per-`Screen` `(id, uri)` intern table (`intern_link`, `parse_id_param`); `Screen::link_target(id)`; SGR reset preserves the open link (link runs end only via an empty-URI OSC 8) |
| `src/cell.rs` | `Cell::link_id()` accessor |
| `src/grid.rs` | OSC 8 re-emission in formatted output — lands in plan 10-02 (remote-attach snapshot fidelity) |
| `tests/hyperlink.rs` | New hyperlink behavior tests (target-vs-label, wrap survival, empty-URI close, `;`-in-URI rejoin) |
| `Cargo.toml` | Upstream `[dev-dependencies]` stripped — see note below |

All fork edits are marked with `BAUDE FORK (OSC 8)` comments in the source.

## Upstream tests

The crates.io registry package for 0.15.2 does **not** ship the upstream
`tests/` directory (its `include` list covers `src/**/*`, LICENSE, README,
CHANGELOG only), so the upstream test suite could not be carried over as a
regression floor. Because no upstream tests exist in this copy, the upstream
`[dev-dependencies]` (nix, quickcheck, rand, serde, serde_json,
terminal_size, vte) were removed from `Cargo.toml` — keeping them would only
have pulled new, unaudited packages into the workspace `Cargo.lock`.
Regression coverage comes from baude's own workspace suite, which exercises
this parser heavily, plus the fork's `tests/hyperlink.rs`.

## Upstream synopsis

```rust
let mut parser = vt100::Parser::new(24, 80, 0);
parser.process(b"this text is \x1b[31mRED\x1b[m");
assert_eq!(
    parser.screen().cell(0, 13).unwrap().fgcolor(),
    vt100::Color::Idx(1),
);
```
