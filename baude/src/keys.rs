use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Encode a crossterm key event into the byte sequence a terminal would send.
/// `app_cursor` selects SS3 (`ESC O`) vs CSI (`ESC [`) encoding for cursor
/// keys, matching DECCKM as set by the inner application.
pub fn encode_key(key: &KeyEvent, app_cursor: bool) -> Vec<u8> {
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
                out.extend_from_slice(b"\x1b[13;2u");
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
