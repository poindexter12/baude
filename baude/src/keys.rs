use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Per-keystroke encode context: the child's input mode plus the destination
/// pane. Generalizes the former `app_cursor: bool` parameter so call sites
/// thread one named struct instead of sibling positional bools.
#[derive(Clone, Copy, Debug)]
pub struct EncodeCtx {
    /// DECCKM state of the child: selects SS3 (`ESC O`) vs CSI (`ESC [`)
    /// encoding for cursor keys.
    pub app_cursor: bool,
    /// True only when the child itself pushed kitty keyboard-enhancement
    /// flags on its PTY (observed push — fail-closed false until verified).
    pub kitty_child: bool,
    /// True when the destination pane is the shell pane rather than the
    /// Claude pane.
    pub to_shell: bool,
}

/// Encode a crossterm key event into the byte sequence a terminal would send.
/// `ctx.app_cursor` selects SS3 (`ESC O`) vs CSI (`ESC [`) encoding for cursor
/// keys, matching DECCKM as set by the inner application.
pub fn encode_key(key: &KeyEvent, ctx: EncodeCtx) -> Vec<u8> {
    let app_cursor = ctx.app_cursor;
    let mods = key.modifiers;
    let ctrl = mods.contains(KeyModifiers::CONTROL);
    let alt = mods.contains(KeyModifiers::ALT);
    let shift = mods.contains(KeyModifiers::SHIFT);

    let mut out: Vec<u8> = Vec::new();

    match key.code {
        KeyCode::Char(c) => {
            if ctrl {
                let byte = match c {
                    ' ' | '@' => Some(0u8),
                    'a'..='z' => Some(c as u8 - b'a' + 1),
                    'A'..='Z' => Some(c.to_ascii_lowercase() as u8 - b'a' + 1),
                    '[' => Some(27),
                    '\\' => Some(28),
                    ']' => Some(29),
                    '^' => Some(30),
                    '_' | '/' => Some(31),
                    '?' => Some(127),
                    _ => None,
                };
                if let Some(b) = byte {
                    if alt {
                        out.push(0x1b);
                    }
                    out.push(b);
                }
            } else {
                if alt {
                    out.push(0x1b);
                }
                let mut buf = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
        }
        KeyCode::Enter => {
            if shift && !alt && !ctrl {
                if ctx.kitty_child {
                    // The child itself enabled the kitty protocol on its PTY:
                    // pass the enhanced sequence through (D-09).
                    out.extend_from_slice(b"\x1b[13;2u");
                } else if !ctx.to_shell {
                    // Claude pane, legacy child: documented fallback newline
                    // insert (D-09, RESEARCH Q3).
                    out.extend_from_slice(b"\x1b\r");
                } else {
                    // Shell pane: ESC CR is meta-CR to readline and must not
                    // reach bash — Shift degrades to plain Enter.
                    out.push(b'\r');
                }
            } else {
                if alt {
                    out.push(0x1b);
                }
                out.push(b'\r');
            }
        }
        KeyCode::Backspace => {
            if alt {
                out.push(0x1b);
            }
            out.push(0x7f);
        }
        KeyCode::Tab => out.push(b'\t'),
        KeyCode::BackTab => out.extend_from_slice(b"\x1b[Z"),
        KeyCode::Esc => out.push(0x1b),
        KeyCode::Up => cursor_key(&mut out, b'A', mods, app_cursor),
        KeyCode::Down => cursor_key(&mut out, b'B', mods, app_cursor),
        KeyCode::Right => cursor_key(&mut out, b'C', mods, app_cursor),
        KeyCode::Left => cursor_key(&mut out, b'D', mods, app_cursor),
        KeyCode::Home => cursor_key(&mut out, b'H', mods, app_cursor),
        KeyCode::End => cursor_key(&mut out, b'F', mods, app_cursor),
        KeyCode::PageUp => tilde_key(&mut out, 5, mods),
        KeyCode::PageDown => tilde_key(&mut out, 6, mods),
        KeyCode::Insert => tilde_key(&mut out, 2, mods),
        KeyCode::Delete => tilde_key(&mut out, 3, mods),
        KeyCode::F(n) => match n {
            1 => out.extend_from_slice(b"\x1bOP"),
            2 => out.extend_from_slice(b"\x1bOQ"),
            3 => out.extend_from_slice(b"\x1bOR"),
            4 => out.extend_from_slice(b"\x1bOS"),
            5 => tilde_key(&mut out, 15, mods),
            6 => tilde_key(&mut out, 17, mods),
            7 => tilde_key(&mut out, 18, mods),
            8 => tilde_key(&mut out, 19, mods),
            9 => tilde_key(&mut out, 20, mods),
            10 => tilde_key(&mut out, 21, mods),
            11 => tilde_key(&mut out, 23, mods),
            12 => tilde_key(&mut out, 24, mods),
            _ => {}
        },
        _ => {}
    }

    out
}

fn modifier_code(mods: KeyModifiers) -> u8 {
    let mut code = 1u8;
    if mods.contains(KeyModifiers::SHIFT) {
        code += 1;
    }
    if mods.contains(KeyModifiers::ALT) {
        code += 2;
    }
    if mods.contains(KeyModifiers::CONTROL) {
        code += 4;
    }
    code
}

fn cursor_key(out: &mut Vec<u8>, ch: u8, mods: KeyModifiers, app_cursor: bool) {
    let m = modifier_code(mods);
    if m > 1 {
        out.extend_from_slice(format!("\x1b[1;{m}").as_bytes());
        out.push(ch);
    } else if app_cursor {
        out.extend_from_slice(b"\x1bO");
        out.push(ch);
    } else {
        out.extend_from_slice(b"\x1b[");
        out.push(ch);
    }
}

fn tilde_key(out: &mut Vec<u8>, num: u8, mods: KeyModifiers) {
    let m = modifier_code(mods);
    if m > 1 {
        out.extend_from_slice(format!("\x1b[{num};{m}~").as_bytes());
    } else {
        out.extend_from_slice(format!("\x1b[{num}~").as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::negotiate_keyboard;

    fn shift_enter() -> KeyEvent {
        KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT)
    }

    fn ctx(kitty_child: bool, to_shell: bool) -> EncodeCtx {
        EncodeCtx {
            app_cursor: false,
            kitty_child,
            to_shell,
        }
    }

    #[test]
    fn shift_enter_legacy_child_claude_pane_inserts_esc_cr() {
        // TKEY-01 fallback insert (D-09): the child never negotiated kitty
        // mode, so the Claude pane gets the documented ESC CR insert.
        assert_eq!(encode_key(&shift_enter(), ctx(false, false)), b"\x1b\r");
    }

    #[test]
    fn shift_enter_legacy_child_shell_pane_degrades_to_plain_cr() {
        // ESC CR is meta-CR to readline and must not reach bash: Shift
        // degrades to plain Enter on the shell pane.
        assert_eq!(encode_key(&shift_enter(), ctx(false, true)), b"\r");
    }

    #[test]
    fn shift_enter_kitty_child_gets_csi_u_passthrough() {
        // The child enabled the kitty protocol itself — pass the enhanced
        // sequence through (frozen row; D-09).
        assert_eq!(encode_key(&shift_enter(), ctx(true, false)), b"\x1b[13;2u");
    }

    #[test]
    fn negotiate_keyboard_gates_on_probe_result() {
        // D-01/D-02: only an affirmative probe enables enhanced mode;
        // Ok(false) and Err (timeout, no tty, Windows) both mean legacy.
        assert!(negotiate_keyboard(|| Ok(true)));
        assert!(!negotiate_keyboard(|| Ok(false)));
        assert!(!negotiate_keyboard(|| Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "probe timed out"
        ))));
    }

    #[test]
    fn shift_enter_kitty_child_shell_pane_still_passes_through_csi_u() {
        // The remaining Shift+Enter matrix cell: the child asked for the
        // kitty protocol, so the pane does not matter — pass it through.
        assert_eq!(encode_key(&shift_enter(), ctx(true, true)), b"\x1b[13;2u");
    }

    #[test]
    fn shift_enter_encoding_is_idempotent() {
        // TKEY-01 idempotency: encode_key is pure/stateless — repeated
        // presses each produce exactly one identical newline sequence, no
        // mode toggling, no accumulated escapes.
        let key = shift_enter();
        let c = ctx(false, false);
        let first = encode_key(&key, c);
        for _ in 0..4 {
            assert_eq!(encode_key(&key, c), first);
        }
        assert_eq!(first, b"\x1b\r");
    }

    /// TKEY-02 freeze (D-10): every legacy row (kitty_child:false) expanded
    /// across BOTH panes — no non-Shift+Enter key may vary by destination.
    /// Expected bytes derived by reading the arms, not from memory.
    fn legacy_corpus() -> Vec<(KeyEvent, EncodeCtx, &'static [u8])> {
        let none = KeyModifiers::NONE;
        let shift = KeyModifiers::SHIFT;
        let alt = KeyModifiers::ALT;
        let ctrl = KeyModifiers::CONTROL;
        let k = KeyEvent::new;
        // (event, app_cursor, expected bytes)
        let base: Vec<(KeyEvent, bool, &'static [u8])> = vec![
            // Enter family (non-Shift arms)
            (k(KeyCode::Enter, none), false, b"\r"),
            (k(KeyCode::Enter, alt), false, b"\x1b\r"),
            (k(KeyCode::Enter, ctrl), false, b"\r"),
            // Ctrl chords (existing baude chords + signals)
            (k(KeyCode::Char('c'), ctrl), false, b"\x03"),
            (k(KeyCode::Char('z'), ctrl), false, b"\x1a"),
            (k(KeyCode::Char('q'), ctrl), false, b"\x11"),
            (k(KeyCode::Char('e'), ctrl), false, b"\x05"),
            (k(KeyCode::Char('n'), ctrl), false, b"\x0e"),
            (k(KeyCode::Char('o'), ctrl), false, b"\x0f"),
            (k(KeyCode::Char(' '), ctrl), false, b"\x00"),
            // Bare control bytes
            (k(KeyCode::Esc, none), false, b"\x1b"),
            (k(KeyCode::Tab, none), false, b"\t"),
            (k(KeyCode::BackTab, shift), false, b"\x1b[Z"),
            (k(KeyCode::Backspace, none), false, b"\x7f"),
            (k(KeyCode::Backspace, alt), false, b"\x1b\x7f"),
            // Arrows, normal (CSI) cursor mode
            (k(KeyCode::Up, none), false, b"\x1b[A"),
            (k(KeyCode::Down, none), false, b"\x1b[B"),
            (k(KeyCode::Right, none), false, b"\x1b[C"),
            (k(KeyCode::Left, none), false, b"\x1b[D"),
            // Arrows, application (SS3) cursor mode
            (k(KeyCode::Up, none), true, b"\x1bOA"),
            (k(KeyCode::Down, none), true, b"\x1bOB"),
            (k(KeyCode::Right, none), true, b"\x1bOC"),
            (k(KeyCode::Left, none), true, b"\x1bOD"),
            // Modified arrows use CSI 1;m in BOTH cursor modes
            (k(KeyCode::Up, shift), false, b"\x1b[1;2A"),
            (k(KeyCode::Up, shift), true, b"\x1b[1;2A"),
            (k(KeyCode::Up, ctrl), false, b"\x1b[1;5A"),
            (k(KeyCode::Left, shift), false, b"\x1b[1;2D"),
            (k(KeyCode::Right, ctrl), true, b"\x1b[1;5C"),
            // Home/End in both cursor modes
            (k(KeyCode::Home, none), false, b"\x1b[H"),
            (k(KeyCode::Home, none), true, b"\x1bOH"),
            (k(KeyCode::End, none), false, b"\x1b[F"),
            (k(KeyCode::End, none), true, b"\x1bOF"),
            // Tilde keys, plain and modified
            (k(KeyCode::PageUp, none), false, b"\x1b[5~"),
            (k(KeyCode::PageDown, none), false, b"\x1b[6~"),
            (k(KeyCode::Insert, none), false, b"\x1b[2~"),
            (k(KeyCode::Delete, none), false, b"\x1b[3~"),
            (k(KeyCode::PageUp, shift), false, b"\x1b[5;2~"),
            (k(KeyCode::Delete, ctrl), false, b"\x1b[3;5~"),
            (k(KeyCode::Insert, alt), false, b"\x1b[2;3~"),
            // Function keys: SS3 for F1-F4, tilde forms for F5-F12
            (k(KeyCode::F(1), none), false, b"\x1bOP"),
            (k(KeyCode::F(2), none), false, b"\x1bOQ"),
            (k(KeyCode::F(3), none), false, b"\x1bOR"),
            (k(KeyCode::F(4), none), false, b"\x1bOS"),
            (k(KeyCode::F(5), none), false, b"\x1b[15~"),
            (k(KeyCode::F(6), none), false, b"\x1b[17~"),
            (k(KeyCode::F(7), none), false, b"\x1b[18~"),
            (k(KeyCode::F(8), none), false, b"\x1b[19~"),
            (k(KeyCode::F(9), none), false, b"\x1b[20~"),
            (k(KeyCode::F(10), none), false, b"\x1b[21~"),
            (k(KeyCode::F(11), none), false, b"\x1b[23~"),
            (k(KeyCode::F(12), none), false, b"\x1b[24~"),
            // Plain and Alt chars, ASCII and UTF-8
            (k(KeyCode::Char('a'), none), false, b"a"),
            (k(KeyCode::Char('A'), shift), false, b"A"),
            (k(KeyCode::Char('a'), alt), false, b"\x1ba"),
            (k(KeyCode::Char('\u{e9}'), none), false, b"\xc3\xa9"),
            (k(KeyCode::Char('\u{e9}'), alt), false, b"\x1b\xc3\xa9"),
        ];
        base.into_iter()
            .flat_map(|(ev, app_cursor, bytes)| {
                [false, true].into_iter().map(move |to_shell| {
                    (
                        ev,
                        EncodeCtx {
                            app_cursor,
                            kitty_child: false,
                            to_shell,
                        },
                        bytes,
                    )
                })
            })
            .collect()
    }

    #[test]
    fn legacy_corpus_bytes_are_frozen() {
        for (ev, c, expected) in legacy_corpus() {
            let actual = encode_key(&ev, c);
            assert_eq!(
                actual, expected,
                "frozen bytes changed for event {ev:?} with ctx {c:?}"
            );
        }
    }
}
