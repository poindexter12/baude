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
    #[allow(dead_code)] // constructed by the 10-03 bare-URL pass
    Bare,
}

/// One activatable link visible on screen. `destination` is the parsed,
/// normalized URL — the same value the hint overlay displays and argv
/// receives (LINK-05/LINK-08 single-source).
#[derive(Clone, Debug)]
pub struct DetectedLink {
    pub destination: url::Url,
    /// Anchor span of the link's first visible fragment.
    pub row: u16,
    pub start_col: u16,
    pub end_col: u16,
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

/// Bare-URL pass seam: scheme-anchored scan with wrap-joining and trailing
/// punctuation trimming. Implemented in plan 10-03; until then no bare
/// candidates are produced.
fn collect_bare_links(_screen: &vt100::Screen) -> Vec<DetectedLink> {
    Vec::new()
}

/// LINK-07: only parsed http/https, no control chars pre- or post-percent-
/// decode, no whitespace. Returns the normalized URL that will be displayed
/// AND passed to argv.
pub fn validate_http_url(raw: &str) -> Option<url::Url> {
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
}
