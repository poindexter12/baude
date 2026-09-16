//! Gesture-time link collection and validation (Phase 10).
//!
//! Pure functions over a `vt100::Screen` — the caller (app.rs) owns the
//! parser lock and the `set_scrollback` bracket. Only targets accepted by
//! [`validate_http_url`] are ever collected, so every `DetectedLink` is
//! activatable by construction (LINK-07 fail-closed).

use baude_core::vt100;

/// Where a detected link came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkSource {
    /// An OSC 8 hyperlink: destination is the sequence's URI parameter,
    /// never the visible label (LINK-01).
    Osc8,
    /// A bare `http(s)://` URL scanned out of rendered text (plan 10-03).
    Bare,
}

/// One activatable link visible on screen. `destination` is the parsed,
/// normalized URL — the same value the hint overlay displays and argv
/// receives (LINK-05/LINK-08 single-source).
#[derive(Clone, Debug)]
pub struct DetectedLink {
    pub destination: url::Url,
    /// Anchor span of the link's first visible fragment. `row`/`start_col`
    /// order the hint overlay top-to-bottom (app.rs open_link_hints).
    pub row: u16,
    pub start_col: u16,
    /// Span end and provenance: asserted by the detection tests; kept as
    /// the hit-testing surface for pointer activation if modifier-click
    /// ever ships (CONTEXT: deliberately NOT in v2.2).
    #[allow(dead_code)]
    pub end_col: u16,
    #[allow(dead_code)]
    pub source: LinkSource,
}

/// Collect every activatable link on the visible screen.
///
/// OSC 8 pass: walk visible cells, group maximal runs sharing a link id,
/// dedupe whole-screen by interned entry, resolve the destination via
/// `Screen::link_target(id)` — cell label text is never consulted — then
/// gate every candidate through [`validate_http_url`]. Candidates that fail
/// validation are simply not collected (they stay plain rendered text).
pub fn collect_links(screen: &vt100::Screen) -> Vec<DetectedLink> {
    let (rows, cols) = screen.size();
    let mut out: Vec<DetectedLink> = Vec::new();
    let mut seen: Vec<u16> = Vec::new();
    for row in 0..rows {
        let mut col = 0;
        while col < cols {
            let Some(id) = screen.cell(row, col).and_then(|c| c.link_id()) else {
                col += 1;
                continue;
            };
            // Maximal run of cells sharing this link id on this row.
            let start_col = col;
            let mut end_col = col;
            while col < cols && screen.cell(row, col).and_then(|c| c.link_id()) == Some(id) {
                end_col = col;
                col += 1;
            }
            if seen.contains(&id) {
                continue; // wrapped fragment / repeat of an interned entry
            }
            seen.push(id);
            let Some(raw) = screen.link_target(id) else {
                continue;
            };
            let Some(destination) = validate_http_url(raw) else {
                continue; // fail closed: not activatable, not collected
            };
            out.push(DetectedLink {
                destination,
                row,
                start_col,
                end_col,
                source: LinkSource::Osc8,
            });
        }
    }
    out.extend(collect_bare_links(screen));
    out
}

/// How many rows past the view edge a `row_wrapped` continuation is
/// followed so an off-screen URL tail still joins completely (RESEARCH
/// Open Question 2 resolution: bounded, <= 4 extra rows). The hint anchor
/// always stays on the visible fragment.
const BARE_CONTINUATION_BOUND: usize = 4;

/// Bare-URL pass (LINK-02): build logical lines by joining row `r+1` onto
/// row `r` while `screen.row_wrapped(r)` — the grid's exact wrap metadata,
/// the same authority selection copy trusts (app.rs:5460-5462); never a
/// column-width heuristic. Scan each logical line for case-insensitive
/// `http(s)://` anchors, extend across the RFC 3986 charset, trim unbalanced
/// trailing prose punctuation, then gate through [`validate_http_url`].
/// Cells inside an OSC 8 run are excluded (the explicit link wins).
fn collect_bare_links(screen: &vt100::Screen) -> Vec<DetectedLink> {
    let (rows, cols) = screen.size();
    let mut out = Vec::new();
    // Logical-line accumulator: one char per grid glyph, with the visible
    // (row, col) of each char — `None` for off-screen continuation chars
    // and for OSC8-run placeholders (never anchorable).
    let mut chars: Vec<char> = Vec::new();
    let mut cells: Vec<Option<(u16, u16)>> = Vec::new();
    for row in 0..rows {
        push_row_text(screen, row, Some(row), cols, &mut chars, &mut cells);
        if !screen.row_wrapped(row) {
            scan_line_for_urls(&chars, &cells, &mut out);
            chars.clear();
            cells.clear();
        }
    }
    if !chars.is_empty() {
        // The bottom visible row is wrapped: its continuation rows sit below
        // the view edge (they exist only when scrolled back — at offset 0
        // the view bottom IS the grid bottom). Shift a clone's view window
        // down one row at a time to read them, bounded.
        let offset = screen.scrollback();
        let reachable = offset.min(BARE_CONTINUATION_BOUND);
        if reachable > 0 {
            let mut peek = screen.clone();
            for k in 1..=reachable {
                peek.set_scrollback(offset - k);
                // The k-th row below the original edge is the bottom row of
                // the view shifted down by k.
                push_row_text(&peek, rows - 1, None, cols, &mut chars, &mut cells);
                if !peek.row_wrapped(rows - 1) {
                    break;
                }
            }
        }
        // Flush whatever joined — a chain longer than the bound yields the
        // truncated candidate, collected only if it still validates.
        scan_line_for_urls(&chars, &cells, &mut out);
    }
    out
}

/// Append one grid row to the logical-line accumulator. `span_row` is the
/// visible row index recorded for span anchoring (`None` for off-screen
/// continuation rows). OSC8-run cells contribute a space placeholder so no
/// bare candidate can start on or extend through them.
fn push_row_text(
    screen: &vt100::Screen,
    row: u16,
    span_row: Option<u16>,
    cols: u16,
    chars: &mut Vec<char>,
    cells: &mut Vec<Option<(u16, u16)>>,
) {
    for col in 0..cols {
        let Some(cell) = screen.cell(row, col) else {
            continue;
        };
        if cell.link_id().is_some() {
            chars.push(' ');
            cells.push(None);
            continue;
        }
        let contents = cell.contents();
        if contents.is_empty() {
            // Blank (or wide-continuation) cell: a space terminates any URL
            // charset run, which is correct — URLs never contain spaces.
            chars.push(' ');
            cells.push(span_row.map(|r| (r, col)));
        } else {
            for ch in contents.chars() {
                chars.push(ch);
                cells.push(span_row.map(|r| (r, col)));
            }
        }
    }
}

/// Scan one logical line for scheme-anchored candidates and collect every
/// one that survives trimming and validation.
fn scan_line_for_urls(
    chars: &[char],
    cells: &[Option<(u16, u16)>],
    out: &mut Vec<DetectedLink>,
) {
    let mut i = 0;
    while i < chars.len() {
        if !(starts_with_ci(chars, i, "http://") || starts_with_ci(chars, i, "https://")) {
            i += 1;
            continue;
        }
        // Extend across the RFC 3986 charset.
        let mut j = i;
        while j < chars.len() && is_rfc3986_char(chars[j]) {
            j += 1;
        }
        let end = trim_trailing_punctuation(chars, i, j);
        if end > i {
            let candidate: String = chars[i..end].iter().collect();
            if let Some(destination) = validate_http_url(&candidate) {
                // Anchor span: the first visible fragment. A candidate whose
                // scheme starts off-screen has no visible anchor — skipped
                // (hints label visible links only).
                if let Some((row, start_col)) = cells[i] {
                    let mut end_col = start_col;
                    for cell in &cells[i..end] {
                        if let Some((r, c)) = cell {
                            if *r == row {
                                end_col = *c;
                            }
                        }
                    }
                    out.push(DetectedLink {
                        destination,
                        row,
                        start_col,
                        end_col,
                        source: LinkSource::Bare,
                    });
                }
            }
        }
        i = j.max(i + 1);
    }
}

/// Case-insensitive ASCII prefix match at a char index.
fn starts_with_ci(chars: &[char], at: usize, prefix: &str) -> bool {
    let mut idx = at;
    for p in prefix.chars() {
        match chars.get(idx) {
            Some(c) if c.eq_ignore_ascii_case(&p) => idx += 1,
            _ => return false,
        }
    }
    true
}

/// RFC 3986 URI characters: unreserved / gen-delims / sub-delims / `%`.
fn is_rfc3986_char(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            '-' | '.'
                | '_'
                | '~'
                | ':'
                | '/'
                | '?'
                | '#'
                | '['
                | ']'
                | '@'
                | '!'
                | '$'
                | '&'
                | '\''
                | '('
                | ')'
                | '*'
                | '+'
                | ','
                | ';'
                | '='
                | '%'
        )
}

/// Iteratively strip unbalanced trailing prose punctuation (the locked list
/// `.,;:!?)]}'"`) from `chars[start..end]`. A closing `)`/`]`/`}` is kept
/// when its matching opener occurs within the candidate (balanced), so
/// `.../Foo_(bar)` keeps its paren while a wrapping `(...)` loses the outer
/// one. Returns the new exclusive end index.
fn trim_trailing_punctuation(chars: &[char], start: usize, mut end: usize) -> usize {
    while end > start {
        let c = chars[end - 1];
        if !matches!(
            c,
            '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '}' | '\'' | '"'
        ) {
            break;
        }
        if let Some(open) = match c {
            ')' => Some('('),
            ']' => Some('['),
            '}' => Some('{'),
            _ => None,
        } {
            let opens = chars[start..end].iter().filter(|&&x| x == open).count();
            let closes = chars[start..end].iter().filter(|&&x| x == c).count();
            if closes <= opens {
                break; // balanced: this closer matches an opener in the URL
            }
        }
        end -= 1;
    }
    end
}

/// LINK-07: only parsed http/https, no control chars pre- or post-percent-
/// decode, no whitespace. Returns the normalized URL that will be displayed
/// AND passed to argv.
pub fn validate_http_url(raw: &str) -> Option<url::Url> {
    // WHATWG parsing is more permissive than the raw string (RESEARCH
    // Pitfall 7): `http:/one-slash` and `http:one-slash` both parse to a
    // well-formed URL. Require the canonical `http(s)://` prefix on the raw
    // input so malformed spellings stay plain text (fail closed).
    let bytes = raw.as_bytes();
    let canonical_prefix = (bytes.len() >= 7 && bytes[..7].eq_ignore_ascii_case(b"http://"))
        || (bytes.len() >= 8 && bytes[..8].eq_ignore_ascii_case(b"https://"));
    if !canonical_prefix {
        return None;
    }
    if raw.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return None; // pre-decode check on the raw string
    }
    let decoded = percent_encoding::percent_decode_str(raw)
        .decode_utf8()
        .ok()?;
    if decoded.chars().any(char::is_control) {
        return None; // post-decode check
    }
    let parsed = url::Url::parse(raw).ok()?;
    matches!(parsed.scheme(), "http" | "https").then_some(parsed)
}

/// LINK-02 bare-URL pass: logical-line joining over `row_wrapped`, scheme-
/// anchored scan, punctuation trim, span anchoring, OSC8 exclusion, and the
/// bounded off-screen continuation (RESEARCH Open Question 2 resolution).
#[cfg(test)]
mod bare_url {
    use super::{collect_links, DetectedLink, LinkSource};
    use baude_core::vt100;

    /// Pure-parser shape (app.rs `clipboard_tests` precedent): feed bytes,
    /// call the pure function on `parser.screen()`.
    fn links_on(input: &[u8], rows: u16, cols: u16) -> Vec<DetectedLink> {
        let mut parser = vt100::Parser::new(rows, cols, 0);
        parser.process(input);
        collect_links(parser.screen())
    }

    fn url(s: &str) -> url::Url {
        url::Url::parse(s).expect("test expectation URL parses")
    }

    #[test]
    fn detects_url_embedded_in_prose_with_span() {
        let links = links_on(b"see https://example.com/path today", 2, 40);
        assert_eq!(links.len(), 1, "exactly one bare URL detected");
        assert_eq!(links[0].destination.as_str(), "https://example.com/path");
        assert_eq!(
            (links[0].row, links[0].start_col, links[0].end_col),
            (0, 4, 27),
            "span covers exactly the URL cells"
        );
        assert_eq!(links[0].source, LinkSource::Bare);
    }

    /// The named RED target: a URL soft-wrapped across rows (row_wrapped
    /// true) is joined into ONE complete URL via the grid's wrap metadata —
    /// the same authority selection copy trusts (app.rs:5460-5462).
    #[test]
    fn soft_wrapped_url_joined_via_row_wrapped() {
        // "https://ex.com/abc" (18 chars) wraps 8/8/2 on an 8-col screen.
        let mut parser = vt100::Parser::new(4, 8, 0);
        parser.process(b"https://ex.com/abc");
        assert!(parser.screen().row_wrapped(0), "precondition: row 0 wraps");
        assert!(parser.screen().row_wrapped(1), "precondition: row 1 wraps");
        let links = collect_links(parser.screen());
        assert_eq!(links.len(), 1, "wrapped fragments join to one URL");
        assert_eq!(links[0].destination.as_str(), "https://ex.com/abc");
        // Anchor span: the first visible fragment (row 0, full width).
        assert_eq!(
            (links[0].row, links[0].start_col, links[0].end_col),
            (0, 0, 7)
        );
        assert_eq!(links[0].source, LinkSource::Bare);
    }

    #[test]
    fn explicit_newline_is_never_joined() {
        // row_wrapped(0) is false across a real newline: the two sides are
        // scanned independently — the row-0 URL stands alone and "yz" never
        // becomes part of it.
        let mut parser = vt100::Parser::new(2, 40, 0);
        parser.process(b"https://a.example/x\r\nyz");
        assert!(!parser.screen().row_wrapped(0), "precondition: no wrap flag");
        let links = collect_links(parser.screen());
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].destination.as_str(), "https://a.example/x");
    }

    #[test]
    fn unbalanced_trailing_paren_stripped_balanced_retained() {
        let links = links_on(b"(https://en.wikipedia.org/wiki/Foo_(bar))", 2, 60);
        assert_eq!(links.len(), 1);
        assert_eq!(
            links[0].destination,
            url("https://en.wikipedia.org/wiki/Foo_(bar)"),
            "outer ) stripped once (unbalanced); inner balanced pair kept"
        );
    }

    #[test]
    fn trailing_prose_punctuation_stripped() {
        let links = links_on(b"https://example.com/a., end", 2, 40);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].destination.as_str(), "https://example.com/a");
    }

    #[test]
    fn case_insensitive_scheme_detected_and_normalized() {
        let links = links_on(b"HTTPS://EXAMPLE.COM/A and Http://ex.com/b", 2, 50);
        assert_eq!(links.len(), 2);
        assert_eq!(
            links[0].destination,
            url("HTTPS://EXAMPLE.COM/A"),
            "uppercase scheme/host detected; parse normalizes"
        );
        assert_eq!(links[1].destination, url("Http://ex.com/b"));
    }

    #[test]
    fn percent_encoded_utf8_survives_detection_intact() {
        let links = links_on(b"get https://ex.com/%E2%9C%93 now", 2, 40);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].destination, url("https://ex.com/%E2%9C%93"));
    }

    /// Off-screen tail (RESEARCH Open Question 2): the view is scrolled back
    /// so the wrapped URL's last row sits below the view edge; the bounded
    /// continuation joins it, and the anchor stays on the visible fragment.
    #[test]
    fn offscreen_tail_joined_via_bounded_continuation() {
        let mut parser = vt100::Parser::new(4, 8, 50);
        // 5 logical rows: "one", "two", then the URL wrapping 8/8/6.
        parser.process(b"one\r\ntwo\r\nhttps://e.com/abcdefgh");
        parser.set_scrollback(1);
        // Visible: one / two / https:// / e.com/ab — tail "cdefgh" is below
        // the edge, reachable only through the wrap flag on the bottom row.
        assert!(parser.screen().row_wrapped(3), "precondition: bottom row wraps");
        let links = collect_links(parser.screen());
        assert_eq!(links.len(), 1, "off-screen tail joined into one URL");
        assert_eq!(links[0].destination.as_str(), "https://e.com/abcdefgh");
        assert_eq!(
            (links[0].row, links[0].start_col, links[0].end_col),
            (2, 0, 7),
            "anchor span stays on the visible fragment"
        );
    }

    /// A continuation chain longer than the bound (4 rows past the edge)
    /// yields the truncated candidate — which is collected only because it
    /// still validates.
    #[test]
    fn continuation_chain_longer_than_bound_truncates() {
        let mut parser = vt100::Parser::new(4, 8, 50);
        // 6 filler rows, then a 66-char URL spanning 9 rows (8/8/.../2).
        let mut input = Vec::new();
        for f in ["f1", "f2", "f3", "f4", "f5", "f6"] {
            input.extend_from_slice(f.as_bytes());
            input.extend_from_slice(b"\r\n");
        }
        input.extend_from_slice(b"https://e.com/");
        input.extend_from_slice("a".repeat(52).as_bytes());
        parser.process(&input);
        // Offset 8: visible = f4/f5/f6/"https://"; 8 URL rows below the edge.
        parser.set_scrollback(8);
        assert!(parser.screen().row_wrapped(3), "precondition: bottom row wraps");
        let links = collect_links(parser.screen());
        assert_eq!(links.len(), 1, "truncated candidate still validates");
        // Visible row + 4 continuation rows: "https://e.com/" + 26 a's.
        let expected = format!("https://e.com/{}", "a".repeat(26));
        assert_eq!(links[0].destination.as_str(), expected);
        assert_eq!(
            (links[0].row, links[0].start_col, links[0].end_col),
            (3, 0, 7)
        );
    }

    /// A program printing its URL as its own OSC8 label yields ONE link:
    /// cells inside an OSC8 run are excluded from the bare pass.
    #[test]
    fn osc8_run_cells_excluded_from_bare_pass() {
        let mut parser = vt100::Parser::new(2, 30, 0);
        parser
            .process(b"\x1b]8;;https://printed.example\x1b\\https://printed.example\x1b]8;;\x1b\\");
        let links = collect_links(parser.screen());
        assert_eq!(links.len(), 1, "one link, not an OSC8 + bare duplicate");
        assert_eq!(links[0].source, LinkSource::Osc8, "the explicit link wins");
        assert_eq!(links[0].destination, url("https://printed.example"));
    }

    #[test]
    fn screen_without_urls_yields_empty() {
        assert!(
            links_on(b"plain text, no links here at all", 2, 40).is_empty(),
            "feeds 10-04's 'no links visible' state"
        );
    }
}

/// LINK-07 validation matrix: exhaustive acceptance/rejection table for
/// `validate_http_url` (string-table test — regression coverage of the
/// 10-01 implementation; failing rows become GREEN obligations).
#[cfg(test)]
mod validate {
    use super::validate_http_url;

    const ACCEPT: &[&str] = &[
        "https://example.com",
        "http://example.com:8080/a?b=c#d",
        "https://ex.com/%E2%9C%93",
        "http://user@host/p",
    ];

    #[test]
    fn accepts_wellformed_http_and_https() {
        for raw in ACCEPT {
            assert!(
                validate_http_url(raw).is_some(),
                "must accept: {raw:?}"
            );
        }
    }

    #[test]
    fn accepted_normalized_form_is_returned() {
        // The collected destination is the parsed normalized form that will
        // be displayed and passed to argv (percent-encoded UTF-8 survives).
        let parsed = validate_http_url("https://ex.com/%E2%9C%93")
            .expect("percent-encoded UTF-8 accepted");
        assert_eq!(parsed, url::Url::parse("https://ex.com/%E2%9C%93").unwrap());
    }

    #[test]
    fn rejects_non_http_schemes() {
        for raw in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html;x",
            "ftp://x",
            "mailto:a@b",
        ] {
            assert!(validate_http_url(raw).is_none(), "must reject: {raw:?}");
        }
    }

    #[test]
    fn rejects_raw_control_chars_and_whitespace() {
        for raw in [
            "https://a\x07b",
            "https://a\x1bb",
            "https://a b",
            "https://a\tb",
            "https://a\nb",
        ] {
            assert!(validate_http_url(raw).is_none(), "must reject: {raw:?}");
        }
    }

    #[test]
    fn rejects_percent_encoded_controls_post_decode() {
        for raw in [
            "https://ex.com/%00",
            "https://ex.com/%0A",
            "https://ex.com/%1B",
        ] {
            assert!(validate_http_url(raw).is_none(), "must reject: {raw:?}");
        }
    }

    #[test]
    fn rejects_empty_and_malformed() {
        for raw in ["", "https://", "http:/one-slash", "not-a-url"] {
            assert!(validate_http_url(raw).is_none(), "must reject: {raw:?}");
        }
    }

    /// Property: every accepted input yields a URL whose serialized form
    /// contains no control chars and whose scheme is exactly http or https.
    #[test]
    fn accepted_urls_are_control_free_and_http_only() {
        for raw in ACCEPT {
            let parsed = validate_http_url(raw).expect("accept-list row");
            assert!(
                !parsed.as_str().chars().any(char::is_control),
                "no control chars in serialized form: {raw:?}"
            );
            assert!(
                matches!(parsed.scheme(), "http" | "https"),
                "scheme allowlist holds: {raw:?}"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{collect_links, validate_http_url};
    use crate::app::App;
    use baude_core::vt100;
    use std::cell::RefCell;
    use std::path::PathBuf;

    /// Tracer: OSC 8 bytes -> cell link id -> gesture-time collection ->
    /// overlay model carrying the actual destination -> validated open
    /// through the injected seam. The spy closure receives exactly the
    /// parsed URI as the single argv string; a non-http target never
    /// reaches the spy (LINK-01/05/07/08).
    #[test]
    fn tracer_end_to_end() {
        let mut parser = vt100::Parser::new(4, 8, 50);
        parser.process(b"\x1b]8;;https://real.example/x\x1b\\click here\x1b]8;;\x1b\\");
        let links = collect_links(parser.screen());
        assert_eq!(links.len(), 1, "exactly one OSC8 link collected");
        assert_eq!(links[0].destination.as_str(), "https://real.example/x");
        // Anchor span: the first visible fragment ("click he", row 0 of the
        // 8-col screen); the wrapped row-1 fragment shares the interned id
        // and must NOT produce a second entry.
        assert_eq!(
            (links[0].row, links[0].start_col, links[0].end_col),
            (0, 0, 7)
        );
        assert_eq!(links[0].source, super::LinkSource::Osc8);

        // Activation path with a spy: records argv, spawns nothing.
        // Phase-8 containment: App::new resolves the config dir, so the test
        // holds a fixture redirect (never the real user paths).
        let root = PathBuf::from("/nonexistent/baude-links-tracer");
        let _redirect = baude_core::testing::TestRedirect::new(&root);
        let mut app = App::new(PathBuf::from("/not-a-repository"));
        let recorded: RefCell<Vec<String>> = RefCell::new(Vec::new());
        app.activate_link(&links[0].destination, |url| {
            recorded.borrow_mut().push(url.to_string());
            Ok(())
        });
        assert_eq!(
            recorded.borrow().as_slice(),
            ["https://real.example/x"],
            "spy receives exactly the validated URL as one argv string"
        );

        // A non-http target is rejected by the validation gate and never
        // becomes a DetectedLink, so it can never reach the opener.
        assert!(validate_http_url("file:///etc/passwd").is_none());
        let mut parser = vt100::Parser::new(4, 8, 50);
        parser.process(b"\x1b]8;;file:///etc/passwd\x1b\\pwd\x1b]8;;\x1b\\");
        assert!(
            collect_links(parser.screen()).is_empty(),
            "non-http OSC8 target is not collected"
        );
    }

    /// WR-03: a wide char inside an OSC 8 label must not split the recorded
    /// span — the continuation spacer carries the run's id (vendored fork),
    /// so `end_col` covers the full label through and past the wide glyph.
    #[test]
    fn wide_char_label_records_full_span() {
        let mut parser = vt100::Parser::new(2, 20, 0);
        parser.process("\x1b]8;;https://wide.example/\x1b\\a中b\x1b]8;;\x1b\\".as_bytes());
        let links = collect_links(parser.screen());
        assert_eq!(links.len(), 1, "one link — no split at the wide char");
        assert_eq!(links[0].destination.as_str(), "https://wide.example/");
        assert_eq!(
            (links[0].row, links[0].start_col, links[0].end_col),
            (0, 0, 3),
            "span extends across the wide continuation to the label's end"
        );
    }
}
