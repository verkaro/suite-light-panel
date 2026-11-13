//! A terminal's grid (`lp_term`), drawn at the screen's own pixels in Terminus: no `View`, which
//! would blur a bitmap font. Each cell its background, its character, the marks joined to it
//! where the font has them, underline and strike as lines. A character the font hasn't got is a
//! box, never nothing. Bold is Terminus Bold, and in one of the first eight colours, its bright
//! form. The cursor is a block in amber, the family's colour for it.
//!
//! The colours a shell asks for are named here and nowhere else ([`Colours::skin`]), from a
//! skin's own: dark's by [`Colours::dark`], 70s's by `Colours::skin(&Skin::SEVENTIES)`.

use crate::{Font, Rect, Rgb, Skin, Surface, mix, rgb};
use lp_term::{Attrs, Cell, Color, Width};

/// What the grid is drawn in: the default foreground and background, the cursor, and the
/// sixteen colours a program names by number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Colours {
    pub fg: Rgb,
    pub bg: Rgb,
    pub cursor: Rgb,
    pub ansi: [Rgb; 16],
}

impl Colours {
    /// The dark skin's.
    pub fn dark() -> Colours {
        Colours::skin(&Skin::DARK)
    }

    /// A skin's. The bright eight are the skin's lit colours; the first eight the same, sunk a
    /// third of the way into the ground, so text on them still reads. Magenta and cyan aren't in
    /// a skin: they're its red and green each met halfway with `tape`.
    pub fn skin(k: &Skin) -> Colours {
        let sunk = |c: Rgb| mix(c, k.bg, 0.3);
        let magenta = mix(k.red, k.tape, 0.5);
        let cyan = mix(k.green, k.tape, 0.5);
        Colours {
            fg: k.text,
            bg: k.bg,
            cursor: k.amber,
            ansi: [
                k.well,
                sunk(k.red),
                sunk(k.green),
                sunk(k.amber),
                sunk(k.tape),
                sunk(magenta),
                sunk(cyan),
                k.text,
                k.faint,
                k.red,
                k.green,
                k.amber,
                k.tape,
                magenta,
                cyan,
                mix(k.text, rgb(255, 255, 255), 0.5),
            ],
        }
    }

    /// These, with the sixteen a program set (Linux's `ESC ] P`) in place of the skin's.
    pub fn with_palette(&self, palette: &[Option<[u8; 3]>; 16]) -> Colours {
        let mut c = self.clone();
        for (i, p) in palette.iter().enumerate() {
            if let Some([r, g, b]) = *p {
                c.ansi[i] = rgb(r, g, b);
            }
        }
        c
    }

    /// The colour a cell names, `default` where it names none.
    pub fn of(&self, c: Color, default: Rgb) -> Rgb {
        match c {
            Color::Default => default,
            Color::Index(i @ 0..16) => self.ansi[usize::from(i)],
            // The 6 × 6 × 6 cube, then 24 greys, as every 256-colour terminal has them.
            Color::Index(i @ 16..232) => {
                let level = |n: u8| if n == 0 { 0 } else { 55 + 40 * n };
                let i = i - 16;
                rgb(level(i / 36), level(i / 6 % 6), level(i % 6))
            }
            Color::Index(i) => {
                let g = 8 + 10 * (i - 232);
                rgb(g, g, g)
            }
            Color::Rgb(r, g, b) => rgb(r, g, b),
        }
    }
}

/// The font a grid is drawn in, and its bold where the console has one of the same size.
#[derive(Clone, Copy)]
pub struct GridFonts<'a> {
    pub regular: &'a Font,
    pub bold: Option<&'a Font>,
}

impl GridFonts<'_> {
    /// A cell's size in pixels.
    pub fn cell(&self) -> (usize, usize) {
        (self.regular.width, self.regular.height)
    }

    /// How many cells fit in `w` × `h` pixels.
    pub fn fit(&self, w: usize, h: usize) -> (usize, usize) {
        let (cw, ch) = self.cell();
        ((w / cw).max(1), (h / ch).max(1))
    }
}

/// How a row is drawn beyond its cells.
#[derive(Clone, Copy, Debug, Default)]
pub struct RowLook {
    /// How many cells across. A row of other width (from the scrollback, kept at the width it
    /// had) is cut, or filled out with the background.
    pub cols: usize,
    /// The cursor's column on this row, drawn as a block.
    pub cursor: Option<usize>,
    /// The whole screen reversed (`?5`, terminfo's `flash`).
    pub reverse: bool,
}

/// One row of cells with its top left at (x, y).
pub fn draw_row(
    sf: &mut Surface,
    x: i32,
    y: i32,
    fonts: GridFonts,
    colours: &Colours,
    cells: &[Cell],
    look: RowLook,
) {
    let cols = look.cols;
    let (cw, ch) = fonts.cell();
    let (dfg, dbg) = if look.reverse { (colours.bg, colours.fg) } else { (colours.fg, colours.bg) };
    let mut buf = [0u8; 4];
    for col in 0..cols {
        let cx = x + (col * cw) as i32;
        let Some(cell) = cells.get(col) else {
            sf.fill(Rect::new(cx, y, cw as i32, ch as i32), dbg);
            continue;
        };
        if cell.width == Width::Tail && col > 0 {
            continue; // drawn with its left half
        }
        let span = if cell.width == Width::Lead && col + 1 < cols { 2 } else { 1 };
        let r = Rect::new(cx, y, (cw * span) as i32, ch as i32);
        let a = cell.attrs;
        // Bold in one of the first eight is its bright form, as the Linux console draws it: with
        // `TERM=linux`'s eight colours, bold black is how a program asks for grey.
        let fg_named = match cell.fg {
            Color::Index(i @ 0..8) if a.contains(Attrs::BOLD) => Color::Index(i + 8),
            c => c,
        };
        let mut fg = colours.of(fg_named, dfg);
        let mut bg = colours.of(cell.bg, dbg);
        if a.contains(Attrs::REVERSE) {
            std::mem::swap(&mut fg, &mut bg);
        }
        if a.contains(Attrs::DIM) {
            fg = mix(bg, fg, 0.6);
        }
        if a.contains(Attrs::HIDDEN) {
            fg = bg;
        }
        if look.cursor == Some(col) {
            (fg, bg) = (dbg, colours.cursor);
        }
        sf.fill(r, bg);
        let font = match fonts.bold {
            Some(b) if a.contains(Attrs::BOLD) => b,
            _ => fonts.regular,
        };
        if cell.ch != ' ' && cell.ch != '\u{A0}' {
            if font.has(cell.ch) {
                font.draw(sf, cx, y, cell.ch.encode_utf8(&mut buf), fg, 1);
            } else {
                sf.frame(r.inset(1), 1, fg);
            }
        }
        for m in cell.marks.iter().flatten() {
            if font.has(*m) {
                font.draw(sf, cx, y, m.encode_utf8(&mut buf), fg, 1);
            }
        }
        let t = (ch / 16).max(1) as i32;
        if a.contains(Attrs::UNDERLINE) {
            sf.fill(Rect::new(r.x, r.bottom() - t, r.w, t), fg);
        }
        if a.contains(Attrs::STRIKE) {
            sf.fill(Rect::new(r.x, y + ch as i32 / 2, r.w, t), fg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Image;
    use lp_term::Term;

    /// A font of 4 × 4: 'A' every pixel set, 'B' only the top row; nothing else.
    fn font() -> Font {
        let mut d = vec![0x72, 0xb5, 0x4a, 0x86];
        for v in [0u32, 32, 1, 2, 4, 4, 4] {
            d.extend(v.to_le_bytes());
        }
        d.extend([0xf0; 4]);
        d.extend([0xf0, 0, 0, 0]);
        d.extend(b"A\xffB\xff");
        Font::parse(&d).unwrap()
    }

    /// The term drawn whole, and a pixel reader.
    fn draw(term: &Term, f: &Font, colours: &Colours) -> Image {
        let fonts = GridFonts { regular: f, bold: None };
        let mut img = Image::new(4 * term.cols(), 4 * term.rows());
        let c = term.cursor();
        for r in 0..term.rows() {
            let look = RowLook {
                cols: term.cols(),
                cursor: (r == c.row && term.modes().cursor_visible).then_some(c.col),
                reverse: term.modes().reverse,
            };
            let mut sf = img.surface();
            draw_row(&mut sf, 0, (r * 4) as i32, fonts, colours, term.row(r).cells(), look);
        }
        img
    }

    fn px(img: &Image, x: usize, y: usize) -> Rgb {
        img.px[y * img.w + x]
    }

    fn term(cols: usize, rows: usize, s: &str) -> Term {
        let mut t = Term::new(cols, rows, 0);
        t.feed(s.as_bytes());
        t
    }

    #[test]
    fn cells_in_their_colours() {
        let c = Colours::dark();
        let f = font();
        let img = draw(&term(3, 1, "A\x1b[31;44mB\x1b[?25l"), &f, &c);
        assert_eq!(px(&img, 1, 1), c.fg, "the glyph, in the default foreground");
        assert_eq!(px(&img, 5, 0), c.ansi[1], "B's top row, red");
        assert_eq!(px(&img, 5, 2), c.ansi[4], "on blue");
        assert_eq!(px(&img, 9, 2), c.bg, "an empty cell, the ground");
    }

    #[test]
    fn bold_is_bright() {
        let c = Colours::dark();
        let img = draw(&term(3, 1, "\x1b[1;30mB\x1b[0;1mB\x1b[0;1;91mB\x1b[?25l"), &font(), &c);
        assert_eq!(px(&img, 1, 0), c.ansi[8], "bold black is grey");
        assert_eq!(px(&img, 5, 0), c.fg, "bold in the default colour stays it");
        assert_eq!(px(&img, 9, 0), c.ansi[9], "and a bright one");
    }

    #[test]
    fn the_cursor_is_an_amber_block() {
        let c = Colours::dark();
        let img = draw(&term(3, 1, "B"), &font(), &c);
        assert_eq!(px(&img, 5, 2), c.cursor);
        let img = draw(&term(3, 1, "B\x1b[?25l"), &font(), &c);
        assert_eq!(px(&img, 5, 2), c.bg, "hidden");
    }

    #[test]
    fn a_missing_glyph_is_a_box() {
        let c = Colours::dark();
        let img = draw(&term(4, 1, "Z中\x1b[?25l"), &font(), &c);
        assert_eq!(px(&img, 1, 1), c.fg, "Z's box");
        assert_eq!(px(&img, 0, 0), c.bg, "inset");
        assert_eq!(px(&img, 5, 1), c.fg, "a wide one's box, across two cells");
        assert_eq!(px(&img, 10, 1), c.fg, "its far edge");
        assert_eq!(px(&img, 11, 1), c.bg, "and no further");
    }

    #[test]
    fn attributes() {
        let c = Colours::dark();
        let f = font();
        let img = draw(&term(4, 1, "\x1b[7mB\x1b[0;4mB\x1b[0;8mA\x1b[0;9mB\x1b[?25l"), &f, &c);
        assert_eq!((px(&img, 0, 0), px(&img, 0, 2)), (c.bg, c.fg), "reversed");
        assert_eq!(px(&img, 5, 3), c.fg, "underlined");
        assert_eq!(px(&img, 9, 1), c.bg, "hidden");
        assert_eq!(px(&img, 13, 2), c.fg, "struck");
        let img = draw(&term(1, 1, "\x1b[2mA\x1b[?25l"), &f, &c);
        assert_eq!(px(&img, 0, 0), mix(c.bg, c.fg, 0.6), "dim");
        let img = draw(&term(1, 1, "\x1b[?5h\x1b[?25l"), &f, &c);
        assert_eq!(px(&img, 0, 0), c.fg, "the screen reversed");
    }

    #[test]
    fn colours_by_number() {
        let c = Colours::dark();
        assert_eq!(c.of(Color::Index(16), 0), rgb(0, 0, 0));
        assert_eq!(c.of(Color::Index(196), 0), rgb(255, 0, 0));
        assert_eq!(c.of(Color::Index(231), 0), rgb(255, 255, 255));
        assert_eq!(c.of(Color::Index(232), 0), rgb(8, 8, 8));
        assert_eq!(c.of(Color::Index(255), 0), rgb(238, 238, 238));
        assert_eq!(c.of(Color::Rgb(1, 2, 3), 0), rgb(1, 2, 3));
        assert_eq!(c.of(Color::Default, 7), 7);
        let mut pal = [None; 16];
        pal[1] = Some([1, 2, 3]);
        assert_eq!(c.with_palette(&pal).ansi[1], rgb(1, 2, 3));
        assert_eq!(c.with_palette(&pal).ansi[2], c.ansi[2]);
    }

    #[test]
    fn a_short_row_is_filled_out() {
        let c = Colours::dark();
        let fonts = GridFonts { regular: &font(), bold: None };
        let mut img = Image::new(12, 4);
        img.px.fill(1);
        let cells = [Cell { ch: 'A', ..Cell::default() }];
        draw_row(&mut img.surface(), 0, 0, fonts, &c, &cells, RowLook { cols: 3, ..RowLook::default() });
        assert_eq!((px(&img, 1, 1), px(&img, 9, 1)), (c.fg, c.bg));
    }
}
