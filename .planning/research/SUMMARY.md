# Project Research Summary

**Project:** baude v2.2 Reliability and Terminal Usability  
**Domain:** Reliability fixes and terminal interaction in a Rust ratatui and VT application  
**Researched:** 2026-09-08  
**Confidence:** HIGH for scope and safety contracts, MEDIUM for terminal interoperability

## Executive Summary

baude v2.2 is a reliability and terminal usability milestone for an established Rust TUI and daemon that owns child PTY sessions. Preserve the current workspace boundaries, shared core seams, vt100 screen model, ratatui renderer, and crossterm lifecycle rather than introducing a new terminal engine. The approved scope is test isolation (#72), hook reconciliation (#70), lock diagnostics (#71), clickable links, and Shift+Enter input, followed by release validation.

The central architectural recommendation is one vt100-owned screen model with bounded link metadata, shared by local PTY and remote WebSocket paths. Do not create two terminal grids or make ratatui cell text carry escape sequences. A targeted pinned vt100 fork may be necessary because current cells discard OSC8 targets, but fork mechanics and API feasibility require a planning spike. An adjacent annotation adapter is viable only if it follows the authoritative grid through every mutation; it must not independently emulate the screen. Keep outer terminal keyboard enhancement negotiation and cleanup separate from child key encoding. Unsupported or ambiguous terminals retain legacy behavior honestly rather than guessing.

The highest risks are destructive configuration recovery, unsafe lock takeover, test leakage, link activation becoming a command channel, and terminal mode leakage. Preserve malformed settings, treat the OS lock as authoritative and PID metadata as advisory, inject fixture roots and child environments, validate HTTP(S) targets and open them through argv on an explicit gesture, and restore keyboard modes on controlled exit paths.

## Key Findings

### Recommended Stack

Keep the locked Rust 2021 workspace, ratatui 0.30.2, crossterm 0.29.0, serde and serde_json, and standard library filesystem and process APIs. No general-purpose dependency is currently justified. Platform launchers must receive validated URLs as separate arguments, never through a shell.

**Core technologies:**

- **Rust standard library:** Fixture roots, filesystem safeguards, and argv-based process launching.
- **serde and serde_json:** Existing configuration serialization and preservation boundaries.
- **ratatui 0.30.2:** Existing frame, selection, and rendering lifecycle. Its cells are not a hyperlink metadata solution.
- **crossterm 0.29.0:** Support queries and keyboard enhancement push/pop APIs for modified Enter.
- **vt100 0.15.2 baseline:** Authoritative characters, scrolling, wrapping, erasure, and colors. Verify whether a small pinned fork or patch can expose OSC8 metadata.

The fork is an option to verify during planning, not a user-approved mandate or an already available dependency. An upgrade to vt100 or ratatui alone is not expected to provide link metadata.

### Expected Features

**Must have:**

- Idempotent owned hook reconciliation preserving custom and mixed groups.
- Explicit lock contention diagnostics without removal, takeover, or PID-based authority.
- Isolated tests with unique fixture repositories, worktrees, config, state, and child environments.
- User-gesture-only OSC8 and bare HTTP(S) links with safe targets and preserved selection.
- Shift+Enter newline when the outer terminal reports a distinguishable supported event and the child input contract supports it.
- Focused tests, CI gates, and macOS/Linux terminal smoke before v2.2.0 publication.

**Should have:**

- Target preview or copy for hidden OSC8 targets.
- Link affordance and unsupported-terminal guidance.
- Structured lock diagnostics.
- Fixture cleanup preview requiring explicit approval.

**Defer:**

- File, SSH, mailto, custom-scheme, or arbitrary-protocol links.
- Automatic cleanup based only on a missing gitdir.
- Forced lock recovery, read-only lock mode, and unrelated product expansion.
- Proxy-monitor integration.

### Architecture Approach

Extend existing seams rather than adding a second subsystem. Test-root injection supports all reliability work. Persistence should return typed lock contention before hydration or mutation. Hook seeding remains one shared core operation for TUI and daemon. Local and remote PTY byte paths feed the same vt100 model and annotation adapter. The renderer and mouse handler query one synchronized snapshot. Outer terminal mode ownership stays in main lifecycle code, while the pure key encoder receives a negotiated capability or explicit fallback.

**Major components:**

1. **Test and persistence boundaries:** Injected roots, child environments, typed lock errors, and durable state preservation.
2. **Hook ownership reconciler:** Narrow structural ownership classification and idempotent merge.
3. **Shared terminal annotation adapter:** Bounded OSC8 metadata and visible bare-URL discovery.
4. **Renderer and gesture policy:** Coordinate-aware hit testing, selection, scrollback, child mouse reporting, and direct validated opening.
5. **Outer terminal guard and key policy:** Capability negotiation, restoration, pure child encoding, and legacy fallback.

### Critical Pitfalls

1. **Brittle hook ownership:** Recognize only a proven baude executable plus literal hook registration. Never delete a containing custom or mixed group.
2. **Invalid settings replaced with empty JSON:** Existing invalid content remains byte-preserved and seeding fails closed.
3. **PID treated as lock authority:** Acquire and retain the OS advisory lock. PID metadata is diagnostic only.
4. **Global test state and unsafe cleanup:** Use injected roots, child environments, unique fixture ownership, and explicit cleanup approval.
5. **Unsafe links or keyboard protocols:** Keep labels separate from targets, validate HTTP(S), avoid shells, preserve selection, negotiate modified keys, and restore terminal state.

## Implications for Roadmap

Suggested five-phase sequence, continuing after completed Phase 7. The roadmapper may combine or split these groups; final numbering and scope remain subject to roadmap approval.

### Phase 8: Test Isolation and Fixture Ownership

**Rationale:** Later reliability and terminal tests need deterministic roots and must not race through global state.  
**Delivers:** Injected roots, unique repositories and worktrees, child `Command.env` setup, ownership-aware fixture cleanup, and missing-gitdir safety tests.  
**Addresses:** #72 and safe cleanup.  
**Avoids:** Environment races, OnceLock contamination, shared worktrees, and destructive cleanup.

### Phase 9: Reliability Contracts for Hooks and Locks

**Rationale:** These are shared core boundaries and release-blocking data safety fixes.  
**Delivers:** Idempotent owned hook seeding, custom and mixed group preservation, invalid JSON preservation, typed lock diagnostics, and no forced takeover.  
**Addresses:** #70 and #71.  
**Avoids:** Duplicate registrations, lost settings, generic empty-state fallback, PID authority, and lock overwrites.

### Phase 10: Clickable Link Metadata and Safe Gestures

**Rationale:** Link activation depends on a coherent rendered-cell coordinate model.  
**Delivers:** Shared vt100 annotations, OSC8 targets, bounded bare HTTP(S) discovery, selection/scrollback-aware hit testing, child mouse compatibility, and argv-based opening.  
**Addresses:** Labeled links, bare URLs, selection preservation, and safe activation.  
**Avoids:** A second screen model, label spoofing, shell execution, arbitrary schemes, and click stealing.

Verify the smallest viable vt100 extension here. Adjacent metadata must follow the authoritative grid; do not silently substitute a full terminal engine.

### Phase 11: Negotiated Multiline Input

**Rationale:** Modified Enter has a distinct outer terminal lifecycle and must not be conflated with child encoding.  
**Delivers:** Support-gated negotiation, scoped restoration, pure child encoding, Shift+Enter newline on supported paths, unchanged Enter/Ctrl-C, and honest legacy fallback.  
**Addresses:** Reliable Shift+Enter.  
**Avoids:** Unconditional CSI-u bytes, guessed distinctions, broken child input, and leaked terminal modes.

### Phase 12: Validation and v2.2.0 Release

**Rationale:** Cross-surface regressions require integrated evidence.  
**Delivers:** Focused and workspace tests, fmt, clippy, CI, lock subprocess tests, parser/selection vectors, and manual terminal smoke for links, mouse behavior, Shift+Enter, normal Enter, restoration, and failure exits. Publish only after checks pass.  
**Addresses:** Release evidence and regression prevention.

### Phase Ordering Rationale

- Isolation comes first because later tests can otherwise leak state or hide races.
- Hooks and locks are grouped as reliability work with independent contracts.
- Links require one authoritative screen model and coordinate mapping.
- Keyboard negotiation has separate outer-terminal and child-protocol responsibilities; grouping after links is a sequencing suggestion, not a strict technical dependency.
- Validation follows implementation so smoke tests exercise integrated paths.

### Research Flags

- **Phase 8:** Confirm global roots, OnceLock seams, subprocess boundaries, and cleanup ownership.
- **Phase 10:** Verify vt100 extension feasibility, OSC8 behavior through scrollback/overwrite, and terminal mouse interoperability.
- **Phase 11:** Verify bounded support negotiation and restoration for panic, suspend, failed setup, and alternate screens.
- **Phase 12:** Confirm the release workflow and cross-platform smoke evidence.

Phase 9 uses standard JSON preservation and OS lock contention patterns, but implementation tests remain mandatory.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | MEDIUM | Existing versions are verified; a vt100 patch remains a planning decision. |
| Features | HIGH | Approved scope and safety boundaries are clear. |
| Architecture | HIGH for boundaries, MEDIUM for link metadata | One model is supported; parser extension details remain unresolved. |
| Pitfalls | HIGH for project evidence, MEDIUM for terminal behavior | Failure modes are concrete, but terminal support varies. |

**Overall confidence:** HIGH for sequencing and safety contracts, MEDIUM for terminal implementation details.

### Gaps to Address

- **OSC8 storage/rendering:** Decide whether a pinned patch is maintainable and carries metadata across local/remote paths without a second screen model.
- **Gesture ownership:** Choose a modifier or context action compatible with child mouse reporting and outer-terminal interception.
- **Keyboard capability timing:** Verify bounded negotiation and restoration across controlled exits.
- **Hook ownership syntax:** Settle shell-safe registration and conservative legacy ownership detection.
- **Release matrix:** Confirm supported terminal/OS combinations. Do not claim universal Shift+Enter detection.

## Sources

### Primary

- `.planning/PROJECT.md` for baseline, approved scope, constraints, and release gates.
- `.planning/research/{STACK,FEATURES,ARCHITECTURE,PITFALLS}.md` for supporting findings and source references.
- `Cargo.lock` and current hook, persistence, PTY, app, key, remote, UI, and manager sources.
- [Rust environment safety](https://doc.rust-lang.org/std/env/fn.set_var.html)
- [crossterm keyboard enhancement APIs](https://docs.rs/crossterm/0.29.0/crossterm/event/struct.KeyboardEnhancementFlags.html)
- [Kitty keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/)
- [vt100 Cell API](https://docs.rs/vt100/0.15.2/vt100/struct.Cell.html)
- [ratatui Cell](https://docs.rs/ratatui/0.30.2/ratatui/buffer/struct.Cell.html)
- [OSC8 specification](https://gist.github.com/egmontkob/eb114294efbcd5adb1944c9f3cb5feda)

### Supporting

- iTerm2, Contour, and Solo terminal documentation linked in the dimension reports.

---
*Research completed: 2026-09-08*  
*Ready for roadmap: yes, after requirements and roadmap approval*
