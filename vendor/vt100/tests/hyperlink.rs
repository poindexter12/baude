//! BAUDE FORK (OSC 8): hyperlink behavior of the vendored parser.
//!
//! Pure parser tests — no fs, no PTY (same shape as baude's clipboard_tests).

mod hyperlink {
    /// LINK-01 + LINK-03: the labeled cell's destination comes from
    /// `link_target(id)`, never from the cell's visible text; a soft-wrapped
    /// label fragment shares the id; an empty-URI OSC 8 closes the run so
    /// cells written afterwards carry no link id.
    #[test]
    fn target_not_label_and_wrap_survival() {
        // 8 cols so the 10-char label "click here" soft-wraps onto row 1.
        let mut parser = vt100::Parser::new(4, 8, 50);
        parser.process(b"\x1b]8;;https://real.example/x\x1b\\click here\x1b]8;;\x1b\\done");
        let screen = parser.screen();
        let id = screen
            .cell(0, 0)
            .unwrap()
            .link_id()
            .expect("labeled cell carries link id");
        // The destination is the URI parameter — NOT the label "click here".
        assert_eq!(screen.link_target(id), Some("https://real.example/x"));
        // "click here" wraps at col 8; the row-1 fragment shares the id.
        assert!(screen.row_wrapped(0));
        assert_eq!(screen.cell(1, 0).unwrap().link_id(), Some(id));
        // The empty-URI sequence closed the run: "done" (row 1, cols 2..6,
        // after the wrapped "re") has no link id.
        assert_eq!(screen.cell(1, 2).unwrap().contents(), "d");
        assert_eq!(screen.cell(1, 2).unwrap().link_id(), None);
    }

    /// vte splits the OSC string on `;`, so a URI containing a literal `;`
    /// arrives split across `params[2..]` — the fork must rejoin them.
    #[test]
    fn semicolon_uri_rejoined() {
        let mut parser = vt100::Parser::new(4, 20, 0);
        parser.process(b"\x1b]8;;https://ex.com/a;b\x1b\\x\x1b]8;;\x1b\\");
        let screen = parser.screen();
        let id = screen
            .cell(0, 0)
            .unwrap()
            .link_id()
            .expect("labeled cell carries link id");
        assert_eq!(screen.link_target(id), Some("https://ex.com/a;b"));
    }
}
