use unicode_width::UnicodeWidthChar as _;

const CODEPOINTS_IN_CELL: usize = 6;

/// Represents a single terminal cell.
#[derive(Clone, Debug, Default, Eq)]
pub struct Cell {
    contents: [char; CODEPOINTS_IN_CELL],
    len: u8,
    attrs: crate::attrs::Attrs,
}

impl PartialEq<Self> for Cell {
    fn eq(&self, other: &Self) -> bool {
        if self.len != other.len {
            return false;
        }
        if self.attrs != other.attrs {
            return false;
        }
        let len = self.len();
        // self.len() always returns a valid value
        self.contents[..len] == other.contents[..len]
    }
}

impl Cell {
    #[inline]
    fn len(&self) -> usize {
        usize::from(self.len & 0x0f)
    }

    pub(crate) fn set(&mut self, c: char, a: crate::attrs::Attrs) {
        self.contents[0] = c;
        self.len = 1;
        // strings in this context should always be an arbitrary character
        // followed by zero or more zero-width characters, so we should only
        // have to look at the first character
        self.set_wide(c.width().unwrap_or(1) > 1);
        self.attrs = a;
    }

    pub(crate) fn append(&mut self, c: char) {
        let len = self.len();
        if len >= CODEPOINTS_IN_CELL {
            return;
        }
        if len == 0 {
            // 0 is always less than 6
            self.contents[0] = ' ';
            self.len += 1;
        }

        let len = self.len();
        // we already checked that len < CODEPOINTS_IN_CELL
        self.contents[len] = c;
        self.len += 1;
    }

    pub(crate) fn clear(&mut self, attrs: crate::attrs::Attrs) {
        self.len = 0;
        self.attrs = attrs;
        // BAUDE FORK (OSC 8): a cleared cell must never inherit the current
        // drawing link — erase fills carry bgcolor semantics only, and a
        // phantom link on a blank cell would surface in hint mode
        // (Pitfall 4). Stripping here covers every clear/fill call site.
        self.attrs.link = None;
    }

    /// Returns the text contents of the cell.
    ///
    /// Can include multiple unicode characters if combining characters are
    /// used, but will contain at most one character with a non-zero character
    /// width.
    #[must_use]
    pub fn contents(&self) -> String {
        let mut s = String::with_capacity(CODEPOINTS_IN_CELL * 4);
        for c in self.contents.iter().take(self.len()) {
            s.push(*c);
        }
        s
    }

    /// Returns whether the cell contains any text data.
    #[must_use]
    pub fn has_contents(&self) -> bool {
        self.len > 0
    }

    /// Returns whether the text data in the cell represents a wide character.
    #[must_use]
    pub fn is_wide(&self) -> bool {
        self.len & 0x80 == 0x80
    }

    /// Returns whether the cell contains the second half of a wide character
    /// (in other words, whether the previous cell in the row contains a wide
    /// character)
    #[must_use]
    pub fn is_wide_continuation(&self) -> bool {
        self.len & 0x40 == 0x40
    }

    fn set_wide(&mut self, wide: bool) {
        if wide {
            self.len |= 0x80;
        } else {
            self.len &= 0x7f;
        }
    }

    pub(crate) fn set_wide_continuation(&mut self, wide: bool) {
        if wide {
            self.len |= 0x40;
        } else {
            self.len &= 0xbf;
        }
    }

    /// BAUDE FORK (OSC 8): carry a link id onto a cell after `clear()` —
    /// used for the wide-char continuation spacer, which must share the
    /// base cell's id so a link run stays contiguous across a wide glyph
    /// (per-cell consumers — run grouping, hit-testing spans — read ids
    /// cell by cell). `clear()` itself strips links by design (Pitfall 4).
    pub(crate) fn set_link(&mut self, link: Option<u16>) {
        self.attrs.link = link;
    }

    pub(crate) fn attrs(&self) -> &crate::attrs::Attrs {
        &self.attrs
    }

    /// BAUDE FORK (OSC 8): the interned link id this cell belongs to, if any.
    /// Resolve the destination URI via [`crate::Screen::link_target`] — the
    /// cell's visible text is never the destination.
    #[must_use]
    pub fn link_id(&self) -> Option<u16> {
        self.attrs.link
    }

    /// Returns the foreground color of the cell.
    #[must_use]
    pub fn fgcolor(&self) -> crate::attrs::Color {
        self.attrs.fgcolor
    }

    /// Returns the background color of the cell.
    #[must_use]
    pub fn bgcolor(&self) -> crate::attrs::Color {
        self.attrs.bgcolor
    }

    /// Returns whether the cell should be rendered with the bold text
    /// attribute.
    #[must_use]
    pub fn bold(&self) -> bool {
        self.attrs.bold()
    }

    /// Returns whether the cell should be rendered with the italic text
    /// attribute.
    #[must_use]
    pub fn italic(&self) -> bool {
        self.attrs.italic()
    }

    /// Returns whether the cell should be rendered with the underlined text
    /// attribute.
    #[must_use]
    pub fn underline(&self) -> bool {
        self.attrs.underline()
    }

    /// Returns whether the cell should be rendered with the inverse text
    /// attribute.
    #[must_use]
    pub fn inverse(&self) -> bool {
        self.attrs.inverse()
    }
}
