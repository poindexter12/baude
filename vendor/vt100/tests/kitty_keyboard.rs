//! BAUDE FORK (kitty keyboard): child-side kitty keyboard-protocol tracking.
//!
//! Pure parser tests — no fs, no PTY (same shape as hyperlink.rs). Bytes go
//! in via `Parser::process`; the observed state comes out via
//! `screen().kitty_keyboard()`. TKEY-05's child leg: plan 11-04 gates
//! enhanced Shift+Enter passthrough on this accessor reporting a nonzero
//! flags value (the child itself pushed enhanced mode on the PTY).

mod kitty_keyboard {
    /// A fresh parser is legacy: no kitty flags active.
    #[test]
    fn fresh_parser_is_inactive() {
        let parser = vt100::Parser::new(4, 8, 0);
        assert_eq!(parser.screen().kitty_keyboard(), 0);
    }

    /// CSI > 1 u pushes flags 1 — the child enabled disambiguation.
    #[test]
    fn push_sets_flags() {
        let mut parser = vt100::Parser::new(4, 8, 0);
        parser.process(b"\x1b[>1u");
        assert_eq!(parser.screen().kitty_keyboard(), 1);
    }

    /// CSI < 1 u after a push returns to inactive (full pop → 0).
    #[test]
    fn push_then_pop_returns_to_inactive() {
        let mut parser = vt100::Parser::new(4, 8, 0);
        parser.process(b"\x1b[>1u\x1b[<1u");
        assert_eq!(parser.screen().kitty_keyboard(), 0);
    }

    /// Nested pushes: current flags are the top of the stack.
    #[test]
    fn nested_push_reports_top() {
        let mut parser = vt100::Parser::new(4, 8, 0);
        parser.process(b"\x1b[>1u\x1b[>5u");
        assert_eq!(parser.screen().kitty_keyboard(), 5);
    }

    /// Popping one level restores the prior entry.
    #[test]
    fn pop_one_level_restores_prior() {
        let mut parser = vt100::Parser::new(4, 8, 0);
        parser.process(b"\x1b[>1u\x1b[>5u\x1b[<1u");
        assert_eq!(parser.screen().kitty_keyboard(), 1);
    }

    /// T-11-05 (hostile input): a huge pop count on a fresh parser
    /// saturates to empty — no panic, no underflow.
    #[test]
    fn huge_pop_on_empty_stack_saturates() {
        let mut parser = vt100::Parser::new(4, 8, 0);
        parser.process(b"\x1b[<99999u");
        assert_eq!(parser.screen().kitty_keyboard(), 0);
    }

    /// T-11-05 (hostile input): 40 consecutive pushes stay bounded at the
    /// spec cap of 32 entries, evicting the OLDEST entry on overflow. The
    /// stack depth is not directly observable, so the cap is proven
    /// behaviorally: after pushes 1..=40 only entries 9..=40 survive —
    /// popping 31 exposes push #9, and one more pop empties the stack.
    #[test]
    fn push_depth_capped_at_32_with_oldest_eviction() {
        let mut parser = vt100::Parser::new(4, 8, 0);
        for flags in 1..=40u16 {
            parser.process(format!("\x1b[>{flags}u").as_bytes());
        }
        // Most recent flags value is still reported.
        assert_eq!(parser.screen().kitty_keyboard(), 40);
        // 32 survivors: pushes 9..=40. Pop 31 → oldest survivor (#9).
        parser.process(b"\x1b[<31u");
        assert_eq!(parser.screen().kitty_keyboard(), 9);
        // One more pop proves nothing beyond the cap was retained.
        parser.process(b"\x1b[<1u");
        assert_eq!(parser.screen().kitty_keyboard(), 0);
    }

    /// CSI = 2 ; 1 u on an empty stack establishes the active entry
    /// (mode 1 = replace).
    #[test]
    fn set_on_empty_stack_establishes_entry() {
        let mut parser = vt100::Parser::new(4, 8, 0);
        parser.process(b"\x1b[=2;1u");
        assert_eq!(parser.screen().kitty_keyboard(), 2);
    }

    /// CSI = flags ; 2 u ORs the given bits into the current entry;
    /// CSI = flags ; 3 u clears them (AND-NOT).
    #[test]
    fn set_or_and_andnot_modes() {
        let mut parser = vt100::Parser::new(4, 8, 0);
        parser.process(b"\x1b[>1u\x1b[=4;2u");
        assert_eq!(parser.screen().kitty_keyboard(), 1 | 4);
        parser.process(b"\x1b[=4;3u");
        assert_eq!(parser.screen().kitty_keyboard(), 1);
    }

    /// T-11-06: the child's CSI ? u query probe stays DROPPED. The state
    /// does not change and nothing is queued in reply — the fork has no
    /// reply channel and must not gain one (observation without
    /// advertisement; answering would entitle the child to CSI-u encodings
    /// baude does not emulate).
    #[test]
    fn query_probe_stays_unanswered() {
        let mut parser = vt100::Parser::new(4, 8, 0);
        parser.process(b"\x1b[?u");
        assert_eq!(parser.screen().kitty_keyboard(), 0);
        // The probe must not perturb any observable screen state either.
        assert_eq!(parser.screen().contents(), "");
    }

    /// WR-02 (per-screen stacks): a push made on the alternate screen dies
    /// with the alternate screen. An alt-screen app killed WITHOUT popping
    /// must not poison main-screen input — after exit, `kitty_keyboard()`
    /// reads the main screen's (empty) stack (fail-closed).
    #[test]
    fn alt_screen_push_does_not_survive_alt_exit() {
        let mut parser = vt100::Parser::new(4, 8, 0);
        parser.process(b"\x1b[?1049h\x1b[>1u");
        assert_eq!(parser.screen().kitty_keyboard(), 1);
        parser.process(b"\x1b[?1049l");
        assert_eq!(
            parser.screen().kitty_keyboard(),
            0,
            "unpopped alt-screen push must not leak onto the main screen"
        );
    }

    /// WR-02: main-screen flags pushed before entering the alternate
    /// screen are untouched by alt-screen push/exit — restored on return,
    /// matching kitty's independent per-screen stacks.
    #[test]
    fn main_screen_flags_restored_after_alt_screen() {
        let mut parser = vt100::Parser::new(4, 8, 0);
        parser.process(b"\x1b[>1u\x1b[?1049h\x1b[>5u");
        assert_eq!(parser.screen().kitty_keyboard(), 5);
        parser.process(b"\x1b[?1049l");
        assert_eq!(
            parser.screen().kitty_keyboard(),
            1,
            "main-screen flags must be restored on alternate-screen exit"
        );
    }

    /// WR-02: the alternate stack is emptied on (re-)activation — a fresh
    /// alt-screen session always starts legacy, even after a prior alt
    /// app left unpopped residue.
    #[test]
    fn alt_stack_emptied_on_reentry() {
        let mut parser = vt100::Parser::new(4, 8, 0);
        parser.process(b"\x1b[?1049h\x1b[>1u\x1b[?1049l\x1b[?1049h");
        assert_eq!(parser.screen().kitty_keyboard(), 0);
    }

    /// Same `>` intermediate but a different final (CSI > 0 c, secondary
    /// device attributes) is NOT a kitty push — it keeps falling through
    /// to the existing debug-log path.
    #[test]
    fn unrelated_final_under_gt_intermediate_ignored() {
        let mut parser = vt100::Parser::new(4, 8, 0);
        parser.process(b"\x1b[>0c");
        assert_eq!(parser.screen().kitty_keyboard(), 0);
    }
}
