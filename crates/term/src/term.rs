//! The terminal: the screen a shell draws on, kept from the bytes it writes. No I/O: bytes go in
//! through [`Term::feed`]; what a program asked to be told back (where the cursor is, what this
//! terminal is) waits in [`Term::take_replies`] for its owner to write to the shell.
//!
//! What's here is what `TERM=linux` claims (`infocmp linux`), and the alternate screen and
//! bracketed paste besides, which programs send whatever the terminfo says.

use std::collections::VecDeque;

use crate::cell::{Attrs, Cell, Color, Row, Width};
use crate::parser::{Params, Parser, Perform};
use crate::width::width;

#[cfg(test)]
mod tests;

/// A tab stop every so many columns, until a program sets its own.
const TAB: usize = 8;

/// The modes a program can set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Modes {
    /// DECAWM: text reaching the right edge runs on into the next row (on at start).
    pub wrap: bool,
    /// DECOM: rows are counted from the scroll region's top, and the cursor kept in it.
    pub origin: bool,
    /// IRM: printing pushes the rest of the row right.
    pub insert: bool,
    /// LNM: a line feed returns to the first column too.
    pub newline: bool,
    /// DECCKM: the arrows send `ESC O` rather than `ESC [`.
    pub app_cursor: bool,
    /// DECKPAM: the keypad sends its application sequences.
    pub app_keypad: bool,
    /// DECTCEM: the cursor is shown (on at start).
    pub cursor_visible: bool,
    /// DECSCNM: the whole screen in reverse (terminfo's `flash`).
    pub reverse: bool,
    /// The alternate screen, which has no scrollback (`?1049`, `?1047`, `?47`).
    pub alt_screen: bool,
    /// `?2004`: a paste is sent between `ESC [200~` and `ESC [201~`.
    pub bracketed_paste: bool,
}

impl Default for Modes {
    fn default() -> Modes {
        Modes {
            wrap: true,
            origin: false,
            insert: false,
            newline: false,
            app_cursor: false,
            app_keypad: false,
            cursor_visible: true,
            reverse: false,
            alt_screen: false,
            bracketed_paste: false,
        }
    }
}

/// Where the cursor is, from the top left, 0 based.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub row: usize,
    pub col: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum Charset {
    #[default]
    Ascii,
    /// DEC's special graphics: the lines and corners of terminfo's `acsc`.
    Graphics,
}

/// What printing draws with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
struct Pen {
    fg: Color,
    bg: Color,
    attrs: Attrs,
}

/// What DECSC keeps and DECRC brings back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
struct Saved {
    row: usize,
    col: usize,
    pen: Pen,
    pending: bool,
    origin: bool,
    charsets: [Charset; 2],
    gl: usize,
}

/// A terminal's emulator: the parser and the screen it keeps.
#[derive(Clone, Debug)]
pub struct Term {
    parser: Parser,
    s: Screen,
}

#[derive(Clone, Debug)]
struct Screen {
    cols: usize,
    rows: usize,
    /// The screen shown, and the other one (primary or alternate).
    lines: Vec<Row>,
    other: Vec<Row>,
    scrollback: VecDeque<Row>,
    max_scrollback: usize,
    row: usize,
    col: usize,
    /// The cursor is past the last column, waiting: the next character wraps first (`xenl`).
    pending: bool,
    pen: Pen,
    /// DECSC's, one for each screen.
    saved: [Saved; 2],
    /// The scroll region, its first and last rows.
    top: usize,
    bottom: usize,
    modes: Modes,
    tabs: Vec<bool>,
    /// G0 and G1, and which of them is in use (SI and SO).
    charsets: [Charset; 2],
    gl: usize,
    dirty: Vec<bool>,
    replies: Vec<u8>,
    bells: u32,
    title: String,
    palette: [Option<[u8; 3]>; 16],
}

impl Term {
    /// A blank terminal of `cols` by `rows` cells (each at least 1), keeping up to `scrollback`
    /// rows that scroll off the top.
    pub fn new(cols: usize, rows: usize, scrollback: usize) -> Term {
        Term { parser: Parser::default(), s: Screen::new(cols.max(1), rows.max(1), scrollback) }
    }

    /// Reads what the shell wrote. A sequence cut between two calls carries on in the next.
    pub fn feed(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.parser.advance(&mut self.s, b);
        }
    }

    /// A new size. Nothing is reflowed: rows are cut or padded on the right, and rows go from
    /// the bottom, or from the top (into the scrollback) as far as keeps the cursor on screen.
    /// The scroll region becomes the whole screen.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.s.resize(cols.max(1), rows.max(1));
    }

    pub fn cols(&self) -> usize {
        self.s.cols
    }

    pub fn rows(&self) -> usize {
        self.s.rows
    }

    /// Row `i` of the screen, from the top.
    pub fn row(&self, i: usize) -> &Row {
        &self.s.lines[i]
    }

    /// How many rows the scrollback holds.
    pub fn scrollback_len(&self) -> usize {
        self.s.scrollback.len()
    }

    /// Row `i` of the scrollback, 0 the oldest. A row keeps the width it had when it went.
    pub fn scrollback(&self, i: usize) -> &Row {
        &self.s.scrollback[i]
    }

    pub fn cursor(&self) -> Cursor {
        Cursor { row: self.s.row, col: self.s.col }
    }

    pub fn modes(&self) -> Modes {
        self.s.modes
    }

    /// The window's title, as OSC 0 or 2 last set it.
    pub fn title(&self) -> &str {
        &self.s.title
    }

    /// The sixteen colours as Linux's `ESC ] P` set them; `None` is the drawer's own.
    pub fn palette(&self) -> &[Option<[u8; 3]>; 16] {
        &self.s.palette
    }

    /// The rows changed since last asked, top to bottom. Asking clears them.
    pub fn take_dirty(&mut self) -> Vec<usize> {
        let rows = self.s.dirty.iter().enumerate().filter(|(_, d)| **d).map(|(i, _)| i).collect();
        self.s.dirty.iter_mut().for_each(|d| *d = false);
        rows
    }

    /// What the terminal answers, to be written to the shell. Asking clears it.
    pub fn take_replies(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.s.replies)
    }

    /// How many bells rang since last asked.
    pub fn take_bells(&mut self) -> u32 {
        std::mem::take(&mut self.s.bells)
    }
}

impl Screen {
    fn new(cols: usize, rows: usize, max_scrollback: usize) -> Screen {
        Screen {
            cols,
            rows,
            lines: vec![Row::new(cols, Color::Default); rows],
            other: vec![Row::new(cols, Color::Default); rows],
            scrollback: VecDeque::new(),
            max_scrollback,
            row: 0,
            col: 0,
            pending: false,
            pen: Pen::default(),
            saved: [Saved::default(); 2],
            top: 0,
            bottom: rows - 1,
            modes: Modes::default(),
            tabs: (0..cols).map(|c| c % TAB == 0).collect(),
            charsets: [Charset::Ascii; 2],
            gl: 0,
            dirty: vec![true; rows],
            replies: Vec::new(),
            bells: 0,
            title: String::new(),
            palette: [None; 16],
        }
    }

    fn touch(&mut self, row: usize) {
        self.dirty[row] = true;
    }

    fn touch_rows(&mut self, lo: usize, hi: usize) {
        self.dirty[lo..=hi].iter_mut().for_each(|d| *d = true);
    }

    fn touch_all(&mut self) {
        self.touch_rows(0, self.rows - 1);
    }

    fn blank_row(&self) -> Row {
        Row::new(self.cols, self.pen.bg)
    }

    // The cursor.

    /// To a row and column of the screen, kept on it.
    fn goto(&mut self, row: usize, col: usize) {
        self.row = row.min(self.rows - 1);
        self.col = col.min(self.cols - 1);
        self.pending = false;
    }

    /// To a row counted as the program counts it: from the region's top in origin mode, and
    /// kept in the region.
    fn goto_origin(&mut self, row: usize, col: usize) {
        if self.modes.origin {
            self.goto((self.top + row).min(self.bottom), col);
        } else {
            self.goto(row, col);
        }
    }

    fn up(&mut self, n: usize) {
        let limit = if self.row >= self.top { self.top } else { 0 };
        self.goto(self.row.saturating_sub(n).max(limit), self.col);
    }

    fn down(&mut self, n: usize) {
        let limit = if self.row <= self.bottom { self.bottom } else { self.rows - 1 };
        self.goto((self.row + n).min(limit), self.col);
    }

    /// Down a row, scrolling the region at its foot.
    fn index(&mut self) {
        if self.row == self.bottom {
            self.scroll_up(1);
        } else if self.row + 1 < self.rows {
            self.row += 1;
        }
        self.pending = false;
    }

    /// Up a row, scrolling the region down at its head.
    fn reverse_index(&mut self) {
        if self.row == self.top {
            self.scroll_down(1);
        } else if self.row > 0 {
            self.row -= 1;
        }
        self.pending = false;
    }

    fn tab(&mut self, n: usize) {
        for _ in 0..n {
            match (self.col + 1..self.cols).find(|&c| self.tabs[c]) {
                Some(c) => self.col = c,
                None => {
                    self.col = self.cols - 1;
                    break;
                }
            }
        }
    }

    fn back_tab(&mut self, n: usize) {
        for _ in 0..n {
            self.col = (0..self.col).rev().find(|&c| self.tabs[c]).unwrap_or(0);
        }
        self.pending = false;
    }

    fn save_cursor(&mut self) {
        self.saved[usize::from(self.modes.alt_screen)] = Saved {
            row: self.row,
            col: self.col,
            pen: self.pen,
            pending: self.pending,
            origin: self.modes.origin,
            charsets: self.charsets,
            gl: self.gl,
        };
    }

    fn restore_cursor(&mut self) {
        let s = self.saved[usize::from(self.modes.alt_screen)];
        self.goto(s.row, s.col);
        self.pending = s.pending && self.col == self.cols - 1;
        self.pen = s.pen;
        self.modes.origin = s.origin;
        self.charsets = s.charsets;
        self.gl = s.gl;
    }

    // Scrolling.

    /// The region up `n` rows. What leaves the top of the whole primary screen goes to the
    /// scrollback.
    fn scroll_up(&mut self, n: usize) {
        let n = n.min(self.bottom - self.top + 1);
        for _ in 0..n {
            let gone = self.lines.remove(self.top);
            if self.top == 0 && !self.modes.alt_screen && self.max_scrollback > 0 {
                if self.scrollback.len() == self.max_scrollback {
                    self.scrollback.pop_front();
                }
                self.scrollback.push_back(gone);
            }
            let blank = self.blank_row();
            self.lines.insert(self.bottom, blank);
        }
        self.touch_rows(self.top, self.bottom);
    }

    fn scroll_down(&mut self, n: usize) {
        let n = n.min(self.bottom - self.top + 1);
        for _ in 0..n {
            self.lines.remove(self.bottom);
            let blank = self.blank_row();
            self.lines.insert(self.top, blank);
        }
        self.touch_rows(self.top, self.bottom);
    }

    // Editing.

    /// Columns `lo..hi` of a row made blank, in the current background.
    fn erase(&mut self, row: usize, lo: usize, hi: usize) {
        let hi = hi.min(self.cols);
        if lo >= hi {
            return;
        }
        let blank = Cell::blank(self.pen.bg);
        let line = &mut self.lines[row];
        line.cells[lo..hi].fill(blank);
        line.mend(lo, hi);
        if hi == self.cols {
            line.wrapped = false;
        }
        self.touch(row);
    }

    fn erase_display(&mut self, how: u16) {
        match how {
            0 => {
                self.erase(self.row, self.col, self.cols);
                for r in self.row + 1..self.rows {
                    self.erase(r, 0, self.cols);
                }
            }
            1 => {
                for r in 0..self.row {
                    self.erase(r, 0, self.cols);
                }
                self.erase(self.row, 0, self.col + 1);
            }
            2 => {
                for r in 0..self.rows {
                    self.erase(r, 0, self.cols);
                }
            }
            3 => self.scrollback.clear(),
            _ => return,
        }
        self.pending = false;
    }

    fn erase_line(&mut self, how: u16) {
        match how {
            0 => self.erase(self.row, self.col, self.cols),
            1 => self.erase(self.row, 0, self.col + 1),
            2 => self.erase(self.row, 0, self.cols),
            _ => return,
        }
        self.pending = false;
    }

    /// `n` blanks pushed in at the cursor, the rest of the row moving right.
    fn insert_chars(&mut self, n: usize) {
        let (row, col, cols) = (self.row, self.col, self.cols);
        let n = n.min(cols - col);
        let blank = Cell::blank(self.pen.bg);
        let line = &mut self.lines[row];
        line.cells.copy_within(col..cols - n, col + n);
        line.cells[col..col + n].fill(blank);
        line.mend(0, cols);
        self.pending = false;
        self.touch(row);
    }

    /// `n` cells taken out at the cursor, the rest of the row moving left.
    fn delete_chars(&mut self, n: usize) {
        let (row, col, cols) = (self.row, self.col, self.cols);
        let n = n.min(cols - col);
        let blank = Cell::blank(self.pen.bg);
        let line = &mut self.lines[row];
        line.cells.copy_within(col + n..cols, col);
        line.cells[cols - n..].fill(blank);
        line.mend(0, cols);
        self.pending = false;
        self.touch(row);
    }

    /// `n` blank rows in at the cursor's, within the region; the cursor goes to the first
    /// column, as DEC's do.
    fn insert_lines(&mut self, n: usize) {
        if self.row < self.top || self.row > self.bottom {
            return;
        }
        let n = n.min(self.bottom - self.row + 1);
        for _ in 0..n {
            self.lines.remove(self.bottom);
            let blank = self.blank_row();
            self.lines.insert(self.row, blank);
        }
        self.touch_rows(self.row, self.bottom);
        self.goto(self.row, 0);
    }

    fn delete_lines(&mut self, n: usize) {
        if self.row < self.top || self.row > self.bottom {
            return;
        }
        let n = n.min(self.bottom - self.row + 1);
        for _ in 0..n {
            self.lines.remove(self.row);
            let blank = self.blank_row();
            self.lines.insert(self.bottom, blank);
        }
        self.touch_rows(self.row, self.bottom);
        self.goto(self.row, 0);
    }

    /// A character the font draws joined to the one before: put with that cell.
    fn join_mark(&mut self, c: char) {
        let mut col = if self.pending || (!self.modes.wrap && self.col == self.cols - 1) {
            self.col
        } else if self.col > 0 {
            self.col - 1
        } else {
            return;
        };
        let line = &mut self.lines[self.row];
        if line.cells[col].width == Width::Tail {
            col -= 1;
        }
        if let Some(slot) = line.cells[col].marks.iter_mut().find(|m| m.is_none()) {
            *slot = Some(c);
        }
        self.touch(self.row);
    }

    /// The edge reached: on to the next row, marking this one as running on.
    fn wrap_line(&mut self) {
        self.lines[self.row].wrapped = true;
        self.col = 0;
        self.index();
    }

    // Screens and modes.

    fn switch_screen(&mut self, alt: bool) {
        if self.modes.alt_screen != alt {
            std::mem::swap(&mut self.lines, &mut self.other);
            self.modes.alt_screen = alt;
            self.touch_all();
        }
    }

    fn clear_screen(&mut self) {
        for r in 0..self.rows {
            self.erase(r, 0, self.cols);
        }
    }

    fn set_mode(&mut self, private: u8, params: &Params, on: bool) {
        for i in 0..params.len() {
            match (private, params.get(i)) {
                (b'?', 1) => self.modes.app_cursor = on,
                (b'?', 5) => {
                    self.modes.reverse = on;
                    self.touch_all();
                }
                (b'?', 6) => {
                    self.modes.origin = on;
                    self.goto_origin(0, 0);
                }
                (b'?', 7) => {
                    self.modes.wrap = on;
                    if !on {
                        self.pending = false;
                    }
                }
                (b'?', 25) => self.modes.cursor_visible = on,
                (b'?', 47) => self.switch_screen(on),
                (b'?', 1047) => {
                    if !on && self.modes.alt_screen {
                        self.clear_screen();
                    }
                    self.switch_screen(on);
                }
                (b'?', 1048) => {
                    if on {
                        self.save_cursor();
                    } else {
                        self.restore_cursor();
                    }
                }
                (b'?', 1049) => {
                    if on {
                        if !self.modes.alt_screen {
                            self.save_cursor();
                            self.switch_screen(true);
                        }
                        self.clear_screen();
                    } else if self.modes.alt_screen {
                        self.switch_screen(false);
                        self.restore_cursor();
                    }
                }
                (b'?', 2004) => self.modes.bracketed_paste = on,
                (0, 4) => self.modes.insert = on,
                (0, 20) => self.modes.newline = on,
                _ => {}
            }
        }
    }

    fn set_region(&mut self, params: &Params) {
        let top = usize::from(params.or(0, 1));
        let bottom = usize::from(params.or(1, self.rows as u16)).min(self.rows);
        if top < bottom {
            self.top = top - 1;
            self.bottom = bottom - 1;
            self.goto_origin(0, 0);
        }
    }

    fn sgr(&mut self, p: &Params) {
        let mut i = 0;
        while i < p.len() {
            // A parameter and the subparameters joined to it.
            let mut end = i + 1;
            while p.is_sub(end) {
                end += 1;
            }
            let attrs = &mut self.pen.attrs;
            match p.get(i) {
                0 => self.pen = Pen::default(),
                1 => attrs.insert(Attrs::BOLD),
                2 => attrs.insert(Attrs::DIM),
                3 => attrs.insert(Attrs::ITALIC),
                4 if end > i + 1 && p.get(i + 1) == 0 => attrs.remove(Attrs::UNDERLINE),
                4 | 21 => attrs.insert(Attrs::UNDERLINE),
                5 | 6 => attrs.insert(Attrs::BLINK),
                7 => attrs.insert(Attrs::REVERSE),
                8 => attrs.insert(Attrs::HIDDEN),
                9 => attrs.insert(Attrs::STRIKE),
                22 => {
                    attrs.remove(Attrs::BOLD);
                    attrs.remove(Attrs::DIM);
                }
                23 => attrs.remove(Attrs::ITALIC),
                24 => attrs.remove(Attrs::UNDERLINE),
                25 => attrs.remove(Attrs::BLINK),
                27 => attrs.remove(Attrs::REVERSE),
                28 => attrs.remove(Attrs::HIDDEN),
                29 => attrs.remove(Attrs::STRIKE),
                n @ 30..=37 => self.pen.fg = Color::Index(n as u8 - 30),
                39 => self.pen.fg = Color::Default,
                n @ 40..=47 => self.pen.bg = Color::Index(n as u8 - 40),
                49 => self.pen.bg = Color::Default,
                n @ 90..=97 => self.pen.fg = Color::Index(n as u8 - 90 + 8),
                n @ 100..=107 => self.pen.bg = Color::Index(n as u8 - 100 + 8),
                n @ (38 | 48) => {
                    let (color, next) =
                        if end > i + 1 { (sub_color(p, i + 1, end), end) } else { semi_color(p, i + 1) };
                    if let Some(c) = color {
                        if n == 38 {
                            self.pen.fg = c;
                        } else {
                            self.pen.bg = c;
                        }
                    }
                    i = next;
                    continue;
                }
                // 10, 11 and 12 are Linux's fonts, which terminfo's `sgr` sends; the rest
                // unknown. Each is passed over alone.
                _ => {}
            }
            i = end;
        }
    }

    fn reply(&mut self, s: &str) {
        self.replies.extend_from_slice(s.as_bytes());
    }

    /// RIS: as new, but for the size, the scrollback, and what's waiting to be taken.
    fn reset(&mut self) {
        let mut fresh = Screen::new(self.cols, self.rows, self.max_scrollback);
        fresh.scrollback = std::mem::take(&mut self.scrollback);
        fresh.replies = std::mem::take(&mut self.replies);
        fresh.bells = self.bells;
        *self = fresh;
    }

    fn resize(&mut self, cols: usize, rows: usize) {
        if (cols, rows) == (self.cols, self.rows) {
            return;
        }
        for line in self.lines.iter_mut().chain(self.other.iter_mut()) {
            line.cells.resize(cols, Cell::default());
            line.mend(cols - 1, cols);
        }
        if rows < self.rows {
            // From the top of the screen shown, as far as keeps the cursor on it.
            let from_top = (self.row + 1).saturating_sub(rows);
            for _ in 0..from_top {
                let gone = self.lines.remove(0);
                if !self.modes.alt_screen && self.max_scrollback > 0 {
                    if self.scrollback.len() == self.max_scrollback {
                        self.scrollback.pop_front();
                    }
                    self.scrollback.push_back(gone);
                }
            }
            self.row -= from_top;
            self.lines.truncate(rows);
            self.other.truncate(rows);
        } else {
            self.lines.resize(rows, Row::new(cols, Color::Default));
            self.other.resize(rows, Row::new(cols, Color::Default));
        }
        let old_cols = self.cols;
        self.tabs.resize(cols, false);
        for (c, stop) in self.tabs.iter_mut().enumerate().skip(old_cols) {
            *stop = c % TAB == 0;
        }
        self.cols = cols;
        self.rows = rows;
        self.top = 0;
        self.bottom = rows - 1;
        self.dirty = vec![true; rows];
        if self.col >= cols {
            self.pending = false;
        }
        self.goto_keep(self.row, self.col);
        for s in &mut self.saved {
            s.row = s.row.min(rows - 1);
            s.col = s.col.min(cols - 1);
        }
    }

    /// `goto` without clearing a pending wrap.
    fn goto_keep(&mut self, row: usize, col: usize) {
        let pending = self.pending;
        self.goto(row, col);
        self.pending = pending;
    }
}

/// `38;5;n` or `38;2;r;g;b`: the colour and where the next parameter is.
fn semi_color(p: &Params, i: usize) -> (Option<Color>, usize) {
    let byte = |j: usize| u8::try_from(p.get(j)).unwrap_or(u8::MAX);
    match p.get(i) {
        5 if i + 1 < p.len() => (Some(Color::Index(byte(i + 1))), i + 2),
        2 if i + 3 < p.len() => (Some(Color::Rgb(byte(i + 1), byte(i + 2), byte(i + 3))), i + 4),
        5 | 2 => (None, p.len()),
        _ => (None, i),
    }
}

/// `38:5:n`, `38:2::r:g:b` (the colour space, then the three), or `38:2:r:g:b`.
fn sub_color(p: &Params, i: usize, end: usize) -> Option<Color> {
    let byte = |j: usize| u8::try_from(p.get(j)).unwrap_or(u8::MAX);
    match (p.get(i), end - i) {
        (5, 2..) => Some(Color::Index(byte(i + 1))),
        (2, 5..) => Some(Color::Rgb(byte(i + 2), byte(i + 3), byte(i + 4))),
        (2, 4) => Some(Color::Rgb(byte(i + 1), byte(i + 2), byte(i + 3))),
        _ => None,
    }
}

/// DEC's special graphics, for 0x5F to 0x7E, and the arrows and block of terminfo's `acsc`.
fn graphics(c: char) -> char {
    match c {
        '+' => '→',
        ',' => '←',
        '-' => '↑',
        '.' => '↓',
        '0' => '█',
        '_' => '\u{A0}',
        '`' => '◆',
        'a' => '▒',
        'b' => '␉',
        'c' => '␌',
        'd' => '␍',
        'e' => '␊',
        'f' => '°',
        'g' => '±',
        'h' => '␤',
        'i' => '␋',
        'j' => '┘',
        'k' => '┐',
        'l' => '┌',
        'm' => '└',
        'n' => '┼',
        'o' => '⎺',
        'p' => '⎻',
        'q' => '─',
        'r' => '⎼',
        's' => '⎽',
        't' => '├',
        'u' => '┤',
        'v' => '┴',
        'w' => '┬',
        'x' => '│',
        'y' => '≤',
        'z' => '≥',
        '{' => 'π',
        '|' => '≠',
        '}' => '£',
        '~' => '·',
        c => c,
    }
}

impl Perform for Screen {
    fn print(&mut self, c: char) {
        let c = if self.charsets[self.gl] == Charset::Graphics { graphics(c) } else { c };
        let w = width(c);
        if w == 0 {
            self.join_mark(c);
            return;
        }
        if self.pending {
            self.wrap_line();
        }
        if w == 2 {
            if self.cols < 2 {
                return;
            }
            if self.col == self.cols - 1 {
                if self.modes.wrap {
                    self.wrap_line();
                } else {
                    self.col -= 1;
                }
            }
        }
        if self.modes.insert {
            self.insert_chars(w);
        }
        let (row, col, pen) = (self.row, self.col, self.pen);
        let cell =
            Cell { ch: c, marks: [None; 2], fg: pen.fg, bg: pen.bg, attrs: pen.attrs, width: Width::One };
        let line = &mut self.lines[row];
        if w == 2 {
            line.cells[col] = Cell { width: Width::Lead, ..cell };
            line.cells[col + 1] = Cell { ch: ' ', width: Width::Tail, ..cell };
        } else {
            line.cells[col] = cell;
        }
        line.mend(col, col + w);
        self.touch(row);
        if col + w < self.cols {
            self.col = col + w;
        } else {
            self.col = self.cols - 1;
            self.pending = self.modes.wrap;
        }
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x07 => self.bells += 1,
            0x08 => {
                self.col = self.col.saturating_sub(1);
                self.pending = false;
            }
            0x09 => self.tab(1),
            0x0A..=0x0C => {
                self.index();
                if self.modes.newline {
                    self.col = 0;
                }
            }
            0x0D => {
                self.col = 0;
                self.pending = false;
            }
            0x0E => self.gl = 1,
            0x0F => self.gl = 0,
            _ => {}
        }
    }

    fn csi(&mut self, private: u8, p: &Params, intermediates: &[u8], fin: u8) {
        if !intermediates.is_empty() {
            return;
        }
        let n = usize::from(p.or(0, 1));
        match (private, fin) {
            (b'?', b'h') => self.set_mode(b'?', p, true),
            (b'?', b'l') => self.set_mode(b'?', p, false),
            (0, b'h') => self.set_mode(0, p, true),
            (0, b'l') => self.set_mode(0, p, false),
            (0, b'@') => self.insert_chars(n),
            (0, b'A') => self.up(n),
            (0, b'B' | b'e') => self.down(n),
            (0, b'C' | b'a') => self.goto(self.row, self.col.saturating_add(n)),
            (0, b'D') => self.goto(self.row, self.col.saturating_sub(n)),
            (0, b'E') => {
                self.down(n);
                self.col = 0;
            }
            (0, b'F') => {
                self.up(n);
                self.col = 0;
            }
            (0, b'G' | b'`') => self.goto(self.row, n - 1),
            (0, b'H' | b'f') => self.goto_origin(usize::from(p.or(0, 1)) - 1, usize::from(p.or(1, 1)) - 1),
            (0, b'I') => self.tab(n),
            (0, b'J') => self.erase_display(p.get(0)),
            (0, b'K') => self.erase_line(p.get(0)),
            (0, b'L') => self.insert_lines(n),
            (0, b'M') => self.delete_lines(n),
            (0, b'P') => self.delete_chars(n),
            (0, b'S') => self.scroll_up(n),
            (0, b'T') if p.len() == 1 => self.scroll_down(n),
            (0, b'X') => {
                self.erase(self.row, self.col, self.col.saturating_add(n));
                self.pending = false;
            }
            (0, b'Z') => self.back_tab(n),
            (0, b'c') if p.get(0) == 0 => self.reply("\x1b[?6c"),
            (0, b'd') => self.goto_origin(n - 1, self.col),
            (0, b'g') => match p.get(0) {
                0 => self.tabs[self.col] = false,
                3 => self.tabs.iter_mut().for_each(|t| *t = false),
                _ => {}
            },
            (0, b'm') => self.sgr(p),
            (0, b'n') => match p.get(0) {
                5 => self.reply("\x1b[0n"),
                6 => {
                    let row = if self.modes.origin { self.row - self.top } else { self.row };
                    let s = format!("\x1b[{};{}R", row + 1, self.col + 1);
                    self.reply(&s);
                }
                _ => {}
            },
            (0, b'r') => self.set_region(p),
            (0, b's') => self.save_cursor(),
            (0, b'u') => self.restore_cursor(),
            // `CSI ? n c` is Linux's cursor shape (`civis`, `cnorm`, `cvvis`), never a question.
            _ => {}
        }
    }

    fn esc(&mut self, intermediates: &[u8], fin: u8) {
        match (intermediates, fin) {
            ([], b'7') => self.save_cursor(),
            ([], b'8') => self.restore_cursor(),
            ([], b'D') => self.index(),
            ([], b'E') => {
                self.index();
                self.col = 0;
            }
            ([], b'H') => self.tabs[self.col] = true,
            ([], b'M') => self.reverse_index(),
            ([], b'Z') => self.reply("\x1b[?6c"),
            ([], b'c') => self.reset(),
            ([], b'=') => self.modes.app_keypad = true,
            ([], b'>') => self.modes.app_keypad = false,
            ([g @ (b'(' | b')')], set) => {
                let set = if set == b'0' { Charset::Graphics } else { Charset::Ascii };
                self.charsets[usize::from(*g == b')')] = set;
            }
            ([b'#'], b'8') => {
                let e = Cell { ch: 'E', ..Cell::default() };
                for line in &mut self.lines {
                    line.cells.fill(e);
                    line.wrapped = false;
                }
                self.top = 0;
                self.bottom = self.rows - 1;
                self.goto(0, 0);
                self.touch_all();
            }
            _ => {}
        }
    }

    fn osc(&mut self, data: &[u8]) {
        let mut parts = data.splitn(2, |&b| b == b';');
        let (Some(kind), Some(text)) = (parts.next(), parts.next()) else { return };
        if kind == b"0" || kind == b"2" {
            self.title = String::from_utf8_lossy(text).into_owned();
        }
    }

    fn palette(&mut self, set: Option<(u8, [u8; 3])>) {
        match set {
            Some((n, rgb)) => self.palette[usize::from(n)] = Some(rgb),
            None => self.palette = [None; 16],
        }
        self.touch_all();
    }
}
