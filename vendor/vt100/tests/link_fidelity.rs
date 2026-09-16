//! BAUDE FORK (OSC 8): link fidelity through grid operations, intern-table
//! caps, and the formatted-output round-trip (LINK-03 + LINK-01 edges).
//!
//! Pure parser tests — no fs, no PTY (same shape as `tests/hyperlink.rs`).
//! Module prefixed `link_` so `cargo test -p vt100 link_` selects the suite.

mod link_fidelity {
    /// Resolve the link target a visible cell carries, if any.
    fn cell_target(screen: &vt100::Screen, row: u16, col: u16) -> Option<String> {
        screen
            .cell(row, col)
            .and_then(|c| c.link_id())
            .and_then(|id| screen.link_target(id))
            .map(str::to_string)
    }

    /// Assert two screens report identical link targets for every cell.
    fn assert_link_parity(a: &vt100::Screen, b: &vt100::Screen, rows: u16, cols: u16) {
        for row in 0..rows {
            for col in 0..cols {
                assert_eq!(
                    cell_target(a, row, col),
                    cell_target(b, row, col),
                    "link target parity at ({row},{col})"
                );
            }
        }
    }

    /// LINK-03 (scrolling + scrollback): cells scrolled out of the visible
    /// grid keep their link id, and the scrollback view resolves the same
    /// target.
    #[test]
    fn scroll_into_scrollback_retains_link() {
        let mut parser = vt100::Parser::new(3, 20, 50);
        parser
            .process(b"\x1b]8;;https://s.example/\x1b\\LINK\x1b]8;;\x1b\\\r\nrow1\r\nrow2\r\nrow3");
        // The LINK row scrolled into scrollback; bring it back into view.
        parser.set_scrollback(1);
        let screen = parser.screen();
        assert_eq!(screen.cell(0, 0).unwrap().contents(), "L");
        let id = screen
            .cell(0, 0)
            .unwrap()
            .link_id()
            .expect("scrolled-to-scrollback cell retains its link id");
        assert_eq!(screen.link_target(id), Some("https://s.example/"));
    }

    /// LINK-03 (soft wrap): both fragments of a wrapped label share one id
    /// (regression from 10-01, kept in the fidelity suite).
    #[test]
    fn wrapped_fragments_share_one_id() {
        // 8 cols so the 10-char label "click here" soft-wraps onto row 1.
        let mut parser = vt100::Parser::new(4, 8, 0);
        parser.process(b"\x1b]8;;https://w.example/\x1b\\click here\x1b]8;;\x1b\\");
        let screen = parser.screen();
        assert!(screen.row_wrapped(0));
        let id = screen
            .cell(0, 0)
            .unwrap()
            .link_id()
            .expect("first fragment linked");
        assert_eq!(screen.cell(1, 0).unwrap().link_id(), Some(id));
        assert_eq!(screen.link_target(id), Some("https://w.example/"));
    }

    /// WR-03 — a wide char inside a label must not split the run: the
    /// continuation spacer cell shares the base cell's link id, so per-cell
    /// ids are contiguous across the whole label.
    #[test]
    fn wide_char_label_keeps_contiguous_link_ids() {
        let mut parser = vt100::Parser::new(2, 20, 0);
        parser.process("\x1b]8;;https://wide.example/\x1b\\a中b\x1b]8;;\x1b\\".as_bytes());
        let screen = parser.screen();
        let id = screen.cell(0, 0).unwrap().link_id().expect("label linked");
        assert!(
            screen.cell(0, 2).unwrap().is_wide_continuation(),
            "precondition: col 2 is the wide glyph's continuation spacer"
        );
        for col in 0..4 {
            assert_eq!(
                screen.cell(0, col).unwrap().link_id(),
                Some(id),
                "contiguous link id at col {col} (incl. the wide spacer)"
            );
        }
        assert_eq!(screen.link_target(id), Some("https://wide.example/"));
        // Past the label: no id; and an overwrite still strips the spacer's.
        assert_eq!(screen.cell(0, 4).unwrap().link_id(), None);
        parser.process(b"\x1b[1;2HXY");
        let screen = parser.screen();
        assert_eq!(
            screen.cell(0, 2).unwrap().link_id(),
            None,
            "overwriting the wide glyph strips the spacer's link id too"
        );
    }

    /// LINK-03 (resize): surviving cells keep their ids across shrink and
    /// grow; nothing panics.
    #[test]
    fn resize_keeps_surviving_ids() {
        let mut parser = vt100::Parser::new(4, 20, 0);
        parser.process(b"\x1b]8;;https://r.example/\x1b\\0123456789\x1b]8;;\x1b\\");
        parser.set_size(4, 5);
        {
            let screen = parser.screen();
            let id = screen
                .cell(0, 0)
                .unwrap()
                .link_id()
                .expect("cell surviving a shrink keeps its link id");
            assert_eq!(screen.link_target(id), Some("https://r.example/"));
            assert_eq!(screen.cell(0, 4).unwrap().link_id(), Some(id));
        }
        parser.set_size(4, 30);
        {
            let screen = parser.screen();
            let id = screen
                .cell(0, 0)
                .unwrap()
                .link_id()
                .expect("cell surviving a grow keeps its link id");
            assert_eq!(screen.link_target(id), Some("https://r.example/"));
            // Newly allocated cells carry no link.
            assert_eq!(screen.cell(0, 25).unwrap().link_id(), None);
        }
    }

    /// LINK-03 (overwrite): plain text written over a linked cell removes
    /// its link id.
    #[test]
    fn overwrite_with_plain_text_clears_link() {
        let mut parser = vt100::Parser::new(2, 20, 0);
        parser.process(b"\x1b]8;;https://o.example/\x1b\\AB\x1b]8;;\x1b\\");
        parser.process(b"\x1b[HXY");
        let screen = parser.screen();
        assert_eq!(screen.cell(0, 0).unwrap().contents(), "X");
        assert_eq!(screen.cell(0, 0).unwrap().link_id(), None);
        assert_eq!(screen.cell(0, 1).unwrap().link_id(), None);
    }

    /// LINK-03 (erasure) / Pitfall 4 — guaranteed-RED target: `\e[2J` while a
    /// link run is open must NOT fill the grid with phantom-linked blanks.
    #[test]
    fn erase_screen_leaves_no_link_ids() {
        let mut parser = vt100::Parser::new(3, 20, 0);
        // The run stays OPEN across the erase — the fill attrs would inherit it.
        parser.process(b"\x1b]8;;https://e.example/\x1b\\LINK");
        parser.process(b"\x1b[2J");
        let screen = parser.screen();
        for row in 0..3 {
            for col in 0..20 {
                assert_eq!(
                    screen.cell(row, col).unwrap().link_id(),
                    None,
                    "erased cell at ({row},{col}) must carry no link id"
                );
            }
        }
    }

    /// LINK-03 (erasure) / Pitfall 4 — line erase (`\e[K`) variant.
    #[test]
    fn erase_line_leaves_no_link_ids() {
        let mut parser = vt100::Parser::new(3, 20, 0);
        parser.process(b"\x1b]8;;https://el.example/\x1b\\LINKED");
        parser.process(b"\r\x1b[K");
        let screen = parser.screen();
        for col in 0..20 {
            assert_eq!(
                screen.cell(0, col).unwrap().link_id(),
                None,
                "line-erased cell at (0,{col}) must carry no link id"
            );
        }
    }

    /// Pitfall 6 — guaranteed-RED target: a URI longer than 2083 bytes
    /// degrades to plain text; parsing continues and later links still work.
    #[test]
    fn uri_over_2083_bytes_is_not_a_link() {
        let mut parser = vt100::Parser::new(2, 20, 0);
        let long_uri = format!("https://long.example/{}", "a".repeat(3000));
        parser.process(format!("\x1b]8;;{long_uri}\x1b\\big\x1b]8;;\x1b\\").as_bytes());
        let screen = parser.screen();
        assert_eq!(
            screen.cell(0, 0).unwrap().link_id(),
            None,
            "over-cap URI must degrade to a non-link"
        );
        assert_eq!(
            screen.cell(0, 0).unwrap().contents(),
            "b",
            "parsing continues after the oversized URI"
        );
        parser.process(b"\r\n\x1b]8;;https://ok.example/\x1b\\ok\x1b]8;;\x1b\\");
        let screen = parser.screen();
        let id = screen
            .cell(1, 0)
            .unwrap()
            .link_id()
            .expect("a valid link after an oversized one still interns");
        assert_eq!(screen.link_target(id), Some("https://ok.example/"));
    }

    /// Pitfall 6 — guaranteed-RED target: once the intern table is full,
    /// further new `(id, uri)` pairs get no link id; existing ids resolve.
    #[test]
    fn intern_table_cap_stops_new_entries() {
        let mut parser = vt100::Parser::new(2, 10, 0);
        // Fill the table to its cap with unique URIs; the cursor stays at
        // home so only the intern table grows.
        for i in 0..10_000u32 {
            parser.process(format!("\x1b[H\x1b]8;;https://e{i}.example/\x1b\\x").as_bytes());
        }
        {
            let screen = parser.screen();
            let at_cap = screen
                .cell(0, 0)
                .unwrap()
                .link_id()
                .expect("the last in-cap link interned");
            assert_eq!(screen.link_target(at_cap), Some("https://e9999.example/"));
        }
        // One past the cap: the new pair degrades to "not a link".
        parser.process(b"\x1b[H\x1b]8;;https://overflow.example/\x1b\\x");
        let screen = parser.screen();
        assert_eq!(
            screen.cell(0, 0).unwrap().link_id(),
            None,
            "a new pair past the cap must not intern"
        );
        // Existing entries still resolve.
        assert_eq!(screen.link_target(0), Some("https://e0.example/"));
        assert_eq!(screen.link_target(9999), Some("https://e9999.example/"));
    }

    /// CR-01 — guaranteed-RED target: vte's SECOND truncation vector,
    /// MAX_OSC_PARAMS (16). A URI with >=14 literal semicolons saturates the
    /// param table; vte drops everything after the 16th param at a `;`
    /// boundary, and the dispatched params sum to well under the 1024-byte
    /// raw cap — so without a param-count guard the truncated (but
    /// well-formed, still-openable) prefix would intern as a WRONG
    /// destination. A saturated table must fail closed: no link id, and
    /// parsing continues.
    #[test]
    fn param_saturated_uri_is_not_a_link() {
        let mut parser = vt100::Parser::new(2, 20, 0);
        // 15 semicolons in the URI: "8" + "" + 16 URI pieces = 18 params,
        // truncated by vte to 16 — "/TAIL" (and more) silently dropped.
        parser.process(
            b"\x1b]8;;https://evil.example/a;b;c;d;e;f;g;h;i;j;k;l;m;n;o;p/TAIL\x1b\\x\x1b]8;;\x1b\\",
        );
        let screen = parser.screen();
        assert_eq!(
            screen.cell(0, 0).unwrap().link_id(),
            None,
            "a param-saturated (possibly truncated) URI must not intern"
        );
        assert_eq!(
            screen.cell(0, 0).unwrap().contents(),
            "x",
            "parsing continues after the refused link"
        );
        // A valid link afterwards still interns.
        parser.process(b"\r\n\x1b]8;;https://ok.example/\x1b\\ok\x1b]8;;\x1b\\");
        let screen = parser.screen();
        let id = screen
            .cell(1, 0)
            .unwrap()
            .link_id()
            .expect("a valid link after a saturated one still interns");
        assert_eq!(screen.link_target(id), Some("https://ok.example/"));
    }

    /// CR-01 boundary control: 12 semicolons (15 dispatched params) stay
    /// below vte's cap and intern intact — the fail-closed guard is not
    /// over-broad.
    #[test]
    fn uri_below_param_cap_still_interns_intact() {
        let mut parser = vt100::Parser::new(2, 20, 0);
        parser.process(b"\x1b]8;;https://ok.example/a;b;c;d;e;f;g;h;i;j;k;l\x1b\\x\x1b]8;;\x1b\\");
        let screen = parser.screen();
        let id = screen
            .cell(0, 0)
            .unwrap()
            .link_id()
            .expect("an in-cap multi-semicolon URI interns");
        assert_eq!(
            screen.link_target(id),
            Some("https://ok.example/a;b;c;d;e;f;g;h;i;j;k;l")
        );
    }

    /// LINK-01 edge (Pitfall 2 regression): a URI with two literal
    /// semicolons is reconstructed intact from vte's split params.
    #[test]
    fn multi_semicolon_uri_rejoined_intact() {
        let mut parser = vt100::Parser::new(2, 20, 0);
        parser.process(b"\x1b]8;;https://ex.com/a;b;c\x1b\\x\x1b]8;;\x1b\\");
        let screen = parser.screen();
        let id = screen.cell(0, 0).unwrap().link_id().expect("linked cell");
        assert_eq!(screen.link_target(id), Some("https://ex.com/a;b;c"));
    }

    /// Edge lift: an empty-URI OSC 8 closes the run; cells written after it
    /// carry no link id.
    #[test]
    fn empty_uri_close_ends_run() {
        let mut parser = vt100::Parser::new(2, 20, 0);
        parser.process(b"\x1b]8;;https://c.example/\x1b\\in\x1b]8;;\x1b\\out");
        let screen = parser.screen();
        assert!(screen.cell(0, 0).unwrap().link_id().is_some());
        assert_eq!(screen.cell(0, 2).unwrap().contents(), "o");
        assert_eq!(screen.cell(0, 2).unwrap().link_id(), None);
    }

    /// Pitfall 3 — guaranteed-RED target: `contents_formatted()` must
    /// re-emit OSC 8 so a fresh parser reconstructs identical link targets
    /// per cell (the remote-attach snapshot path).
    #[test]
    fn formatted_round_trip_preserves_link_targets() {
        let mut a = vt100::Parser::new(4, 40, 0);
        a.process(
            b"\x1b]8;;https://one.example/\x1b\\one\x1b]8;;\x1b\\ mid \
              \x1b]8;id=x;https://two.example/\x1b\\two\x1b]8;;\x1b\\ end",
        );
        let mut b = vt100::Parser::new(4, 40, 0);
        b.process(&a.screen().contents_formatted());
        assert_link_parity(a.screen(), b.screen(), 4, 40);
        // Guard against vacuous parity: the reconstructed screen actually links.
        assert_eq!(
            cell_target(b.screen(), 0, 0).as_deref(),
            Some("https://one.example/")
        );
    }

    /// Pitfall 3 (id= param): two runs with the SAME uri but DIFFERENT `id=`
    /// params are distinct links (spec grouping rule); the round-trip must
    /// preserve the id param, keeping them distinct in the fresh parser.
    #[test]
    fn formatted_round_trip_preserves_id_param_grouping() {
        let mut a = vt100::Parser::new(2, 20, 0);
        a.process(
            b"\x1b]8;id=a;https://same.example/\x1b\\X\x1b]8;;\x1b\\\
              \x1b]8;id=b;https://same.example/\x1b\\Y\x1b]8;;\x1b\\",
        );
        let a_screen = a.screen();
        let ax = a_screen.cell(0, 0).unwrap().link_id().expect("X linked");
        let ay = a_screen.cell(0, 1).unwrap().link_id().expect("Y linked");
        assert_ne!(ax, ay, "distinct id= params are distinct links in A");
        let mut b = vt100::Parser::new(2, 20, 0);
        b.process(&a_screen.contents_formatted());
        let b_screen = b.screen();
        let bx = b_screen
            .cell(0, 0)
            .unwrap()
            .link_id()
            .expect("X linked after round-trip");
        let by = b_screen
            .cell(0, 1)
            .unwrap()
            .link_id()
            .expect("Y linked after round-trip");
        assert_ne!(bx, by, "id= grouping must survive the round-trip");
        assert_eq!(b_screen.link_target(bx), Some("https://same.example/"));
        assert_eq!(b_screen.link_target(by), Some("https://same.example/"));
    }

    /// Pitfall 3 (open run): a run still open at snapshot time stays open in
    /// the fresh parser, so live bytes arriving after attach continue it —
    /// exactly the subscribe()-then-broadcast sequence.
    #[test]
    fn formatted_round_trip_keeps_open_run_open() {
        let mut a = vt100::Parser::new(2, 20, 0);
        a.process(b"\x1b]8;;https://open.example/\x1b\\x");
        let mut b = vt100::Parser::new(2, 20, 0);
        b.process(&a.screen().contents_formatted());
        // Live bytes after the snapshot land in both parsers.
        a.process(b"y");
        b.process(b"y");
        assert_eq!(
            cell_target(a.screen(), 0, 1).as_deref(),
            Some("https://open.example/"),
            "local parser continues the open run"
        );
        assert_eq!(
            cell_target(b.screen(), 0, 1).as_deref(),
            Some("https://open.example/"),
            "reconstructed parser continues the open run"
        );
    }
}
