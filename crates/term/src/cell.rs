//! A cell of the grid, and a row of them.

/// A colour as the shell asked for it. What it looks like is the drawer's: the sixteen are named
/// by role in one place there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Color {
    /// The terminal's own foreground or background.
    #[default]
    Default,
    /// One of the 256: 0 to 7 the ANSI colours, 8 to 15 their bright forms, then the cube and
    /// the greys.
    Index(u8),
    Rgb(u8, u8, u8),
}

/// A cell's attributes, a set of the constants below.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Attrs(u16);

impl Attrs {
    pub const BOLD: Attrs = Attrs(1);
    pub const DIM: Attrs = Attrs(1 << 1);
    pub const ITALIC: Attrs = Attrs(1 << 2);
    pub const UNDERLINE: Attrs = Attrs(1 << 3);
    pub const BLINK: Attrs = Attrs(1 << 4);
    pub const REVERSE: Attrs = Attrs(1 << 5);
    pub const HIDDEN: Attrs = Attrs(1 << 6);
    pub const STRIKE: Attrs = Attrs(1 << 7);

    pub const fn empty() -> Attrs {
        Attrs(0)
    }

    pub const fn contains(self, other: Attrs) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn insert(&mut self, other: Attrs) {
        self.0 |= other.0;
    }

    pub fn remove(&mut self, other: Attrs) {
        self.0 &= !other.0;
    }

    /// The bits, for a protocol to carry.
    pub const fn bits(self) -> u16 {
        self.0
    }

    pub const fn from_bits(bits: u16) -> Attrs {
        Attrs(bits)
    }
}

/// Where a cell sits in a character: alone, or the left or right half of a wide one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Width {
    #[default]
    One,
    /// The left half of a wide character, which it holds; the cell after is its `Tail`.
    Lead,
    /// The right half, holding nothing of its own.
    Tail,
}

/// One cell: a character, the marks joined to it (up to two; more are dropped), its colours and
/// attributes, and which half of a wide character it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub marks: [Option<char>; 2],
    pub fg: Color,
    pub bg: Color,
    pub attrs: Attrs,
    pub width: Width,
}

impl Default for Cell {
    fn default() -> Cell {
        Cell::blank(Color::Default)
    }
}

impl Cell {
    /// An empty cell. Erasing gives the current background (terminfo's `bce`).
    pub const fn blank(bg: Color) -> Cell {
        Cell { ch: ' ', marks: [None; 2], fg: Color::Default, bg, attrs: Attrs::empty(), width: Width::One }
    }
}

/// A row of cells, as wide as the grid was when it was last touched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub(crate) cells: Vec<Cell>,
    pub(crate) wrapped: bool,
}

impl Row {
    pub(crate) fn new(cols: usize, bg: Color) -> Row {
        Row { cells: vec![Cell::blank(bg); cols], wrapped: false }
    }

    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    /// Whether the text ran on into the next row at the edge, rather than ending in a newline.
    pub fn wrapped(&self) -> bool {
        self.wrapped
    }

    /// The row as text, a wide character once, its marks after it.
    pub fn text(&self) -> String {
        let mut s = String::with_capacity(self.cells.len());
        for c in &self.cells {
            if c.width != Width::Tail {
                s.push(c.ch);
                s.extend(c.marks.iter().flatten());
            }
        }
        s
    }

    /// Mends wide characters cut in two by an edit at `lo..hi`: a lead without its tail, or a
    /// tail without its lead, becomes blank.
    pub(crate) fn mend(&mut self, lo: usize, hi: usize) {
        let n = self.cells.len();
        let from = lo.saturating_sub(1);
        let to = (hi + 1).min(n);
        for i in from..to {
            let broken = match self.cells[i].width {
                Width::One => false,
                Width::Lead => i + 1 >= n || self.cells[i + 1].width != Width::Tail,
                Width::Tail => i == 0 || self.cells[i - 1].width != Width::Lead,
            };
            if broken {
                let bg = self.cells[i].bg;
                self.cells[i] = Cell::blank(bg);
            }
        }
    }
}
