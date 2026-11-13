//! A list on a panel: rows in columns under their heads, the chosen row lit, scrolled so it
//! stays in view. The rows are the tool's and so is which is chosen; a list keeps only where
//! it's scrolled to, which is the view's own to keep.

use crate::{Align, Hits, Pen, Rect, Rgb, skin};

/// A column: its head, and how wide it is.
#[derive(Clone, Copy, Debug)]
pub struct Column<'a> {
    /// In capitals, as a section's label is. Empty for none.
    pub head: &'a str,
    /// In logical pixels. 0: a share of what the others leave.
    pub w: i32,
    pub align: Align,
}

/// One cell of a row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    pub text: String,
    /// When it isn't the skin's `text`: `dim` for what's secondary, amber for what waits.
    pub color: Option<Rgb>,
}

impl Cell {
    pub fn new(text: impl Into<String>) -> Cell {
        Cell { text: text.into(), color: None }
    }

    pub fn colored(text: impl Into<String>, color: Rgb) -> Cell {
        Cell { text: text.into(), color: Some(color) }
    }
}

/// How a list is set.
#[derive(Clone, Copy, Debug)]
pub struct ListLook<'a> {
    /// A row's height, in logical pixels.
    pub row_h: i32,
    /// The text's height.
    pub size: f32,
    /// Said in the middle when there are no rows.
    pub empty: &'a str,
}

/// The gap between columns, and at each side, in logical pixels.
const GAP: i32 = 8;
/// The scroll mark's width.
const MARK: i32 = 3;

/// Where a list is scrolled to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct List {
    /// The first row shown.
    pub top: usize,
}

impl List {
    /// How many rows fit in `r` under the heads.
    pub fn rows(r: Rect, look: &ListLook, cols: &[Column]) -> usize {
        let heads = if cols.iter().any(|c| !c.head.is_empty()) { look.row_h } else { 0 };
        ((r.h - heads) / look.row_h.max(1)).max(0) as usize
    }

    /// Scrolls as little as keeps row `cursor` in view, of `total` rows with `rows` showing.
    pub fn keep(&mut self, cursor: usize, total: usize, rows: usize) {
        if rows == 0 {
            return;
        }
        if cursor < self.top {
            self.top = cursor;
        } else if cursor >= self.top + rows {
            self.top = cursor + 1 - rows;
        }
        self.top = self.top.min(total.saturating_sub(rows));
    }

    /// Each column's place across `r`: its x and its width.
    fn places(r: Rect, cols: &[Column]) -> Vec<(i32, i32)> {
        let fixed: i32 = cols.iter().map(|c| c.w).sum();
        let shared = cols.iter().filter(|c| c.w == 0).count() as i32;
        let gaps = GAP * (cols.len() as i32 + 1) + MARK;
        let each = if shared > 0 { ((r.w - fixed - gaps) / shared).max(0) } else { 0 };
        let mut x = r.x + GAP;
        cols.iter()
            .map(|c| {
                let w = if c.w == 0 { each } else { c.w };
                let at = (x, w);
                x += w + GAP;
                at
            })
            .collect()
    }

    /// Draws the list in `r`: the heads, then the rows from where it's scrolled to, `row(i)`
    /// giving row `i`'s cells. The row at `cursor` is lit: `btnHi` when the hand is on the
    /// list (`focus`), `btn` when it's elsewhere. With `hits`, each row shown can be clicked,
    /// as `PREFIX:INDEX`. A mark at the right says where in a longer list this is.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        pen: &mut Pen,
        r: Rect,
        look: &ListLook,
        cols: &[Column],
        total: usize,
        cursor: Option<usize>,
        focus: bool,
        mut row: impl FnMut(usize) -> Vec<Cell>,
        mut hits: Option<(&mut Hits, &str)>,
    ) {
        let places = Self::places(r, cols);
        let mut y = r.y;
        if cols.iter().any(|c| !c.head.is_empty()) {
            for (c, (x, w)) in cols.iter().zip(&places) {
                pen.text(Rect::new(*x, y, *w, look.row_h), c.head, 9.0, true, skin().dim, c.align);
            }
            pen.fill(Rect::new(r.x, y + look.row_h - 1, r.w, 1), skin().rule);
            y += look.row_h;
        }
        let rows = Self::rows(r, look, cols);
        if total == 0 {
            let mid = Rect::new(r.x, y, r.w, (r.bottom() - y).min(look.row_h * 3));
            pen.text(mid, look.empty, look.size, false, skin().faint, Align::Centre);
            return;
        }
        let top = self.top.min(total.saturating_sub(rows));
        for i in top..(top + rows).min(total) {
            let line = Rect::new(r.x, y, r.w - MARK, look.row_h);
            if cursor == Some(i) {
                pen.fill(line, if focus { skin().btn_hi } else { skin().btn });
            }
            // The cursor's row is a key's colour: what's on it is drawn as on a key.
            let on = |c| if cursor == Some(i) { skin().on_key(c) } else { c };
            for ((c, (x, w)), cell) in cols.iter().zip(&places).zip(row(i)) {
                let color = on(cell.color.unwrap_or(skin().text));
                pen.text(Rect::new(*x, y, *w, look.row_h), &cell.text, look.size, false, color, c.align);
            }
            if let Some((hits, prefix)) = hits.as_mut() {
                hits.add(line, &format!("{prefix}:{i}"));
            }
            y += look.row_h;
        }
        if total > rows && rows > 0 {
            // Where this is in the whole list.
            let track = rows as i32 * look.row_h;
            let start = y - track;
            let h = ((track as f32 * rows as f32 / total as f32) as i32).max(look.row_h / 2);
            let at = start + ((track - h) as f32 * top as f32 / (total - rows) as f32).round() as i32;
            pen.fill(Rect::new(r.right() - MARK, at, MARK, h), skin().dim);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dark;
    use crate::pen::tests::{fonts, has};
    use crate::{Image, View};

    const LOOK: ListLook = ListLook { row_h: 20, size: 12.0, empty: "nothing found" };
    const COLS: [Column; 3] = [
        Column { head: "NAME", w: 0, align: Align::Left },
        Column { head: "RT", w: 50, align: Align::Right },
        Column { head: "", w: 30, align: Align::Left },
    ];

    #[test]
    fn it_scrolls_as_little_as_keeps_the_cursor_in_view() {
        let mut l = List::default();
        l.keep(3, 100, 10);
        assert_eq!(l.top, 0);
        l.keep(10, 100, 10);
        assert_eq!(l.top, 1);
        l.keep(50, 100, 10);
        assert_eq!(l.top, 41);
        l.keep(45, 100, 10);
        assert_eq!(l.top, 41, "still in view: it stays");
        l.keep(2, 100, 10);
        assert_eq!(l.top, 2);
        l.keep(99, 100, 10);
        assert_eq!(l.top, 90);
        // The list gets shorter under it.
        l.keep(3, 5, 10);
        assert_eq!(l.top, 0);
        l.keep(0, 0, 10);
        assert_eq!(l.top, 0);
    }

    fn drawn(l: &List, total: usize, cursor: Option<usize>, focus: bool, hits: &mut Hits) -> Option<Image> {
        let fs = fonts()?;
        let mut img = Image::new(300, 120);
        let mut sf = img.surface();
        let mut p = Pen { sf: &mut sf, fonts: &fs, v: View::fit(300, 120, 300, 120) };
        let row = |i: usize| {
            vec![
                Cell::new(format!("room {i}")),
                Cell::colored("0.42", dark::DIM),
                Cell::colored("•", dark::AMBER),
            ]
        };
        l.draw(
            &mut p,
            Rect::new(0, 0, 300, 120),
            &LOOK,
            &COLS,
            total,
            cursor,
            focus,
            row,
            Some((hits, "row")),
        );
        Some(img)
    }

    #[test]
    fn rows_under_their_heads_with_the_chosen_one_lit() {
        let all = Rect::new(0, 0, 300, 120);
        assert_eq!(List::rows(all, &LOOK, &COLS), 5);
        let mut hits = Hits::new();
        let Some(img) = drawn(&List::default(), 3, Some(1), true, &mut hits) else { return };
        let row = |i: i32| Rect::new(0, 20 + 20 * i, 297, 20);
        assert!(has(&img, Rect::new(0, 0, 300, 19), dark::DIM), "the heads");
        assert!(has(&img, Rect::new(0, 19, 300, 1), dark::RULE));
        assert!(has(&img, row(1), dark::BTN_HI) && !has(&img, row(0), dark::BTN_HI));
        assert!(has(&img, row(0), dark::TEXT) && has(&img, row(0), dark::AMBER));
        assert!(!has(&img, row(3), dark::TEXT), "three rows only");
        assert_eq!(hits.at(100.0, 45.0), Some("row:1"));
        assert_eq!(hits.at(100.0, 10.0), None, "a head isn't a row");
        assert_eq!(hits.at(100.0, 85.0), None);
        assert!(!has(&img, Rect::new(297, 0, 3, 120), dark::DIM), "all in view: no mark");
        // The hand elsewhere: the choice still shows, quieter.
        let img = drawn(&List::default(), 3, Some(1), false, &mut Hits::new()).unwrap();
        assert!(has(&img, row(1), dark::BTN) && !has(&img, row(1), dark::BTN_HI));
    }

    #[test]
    fn a_long_list_shows_where_it_is_and_an_empty_one_says_so() {
        let mut hits = Hits::new();
        let top = List::default();
        let Some(img) = drawn(&top, 50, Some(0), true, &mut hits) else { return };
        let mark = |img: &Image| (0..120).filter(|&y| img.px[y * 300 + 298] == dark::DIM).collect::<Vec<_>>();
        let at_top = mark(&img);
        assert_eq!(at_top.first(), Some(&20), "{at_top:?}");
        let end = List { top: 45 };
        let mut hits = Hits::new();
        let img = drawn(&end, 50, Some(49), true, &mut hits).unwrap();
        let at_end = mark(&img);
        assert_eq!(at_end.last(), Some(&119), "{at_end:?}");
        assert_eq!(at_end.len(), at_top.len());
        assert_eq!(hits.at(100.0, 110.0), Some("row:49"));
        assert_eq!(hits.at(100.0, 30.0), Some("row:45"));
        // Scrolled past a list that's since got shorter: it still shows its end.
        let mut hits = Hits::new();
        drawn(&List { top: 45 }, 7, None, true, &mut hits).unwrap();
        assert_eq!(hits.at(100.0, 110.0), Some("row:6"));
        let mut hits = Hits::new();
        let img = drawn(&top, 0, None, true, &mut hits).unwrap();
        assert!(has(&img, Rect::new(0, 20, 300, 60), dark::FAINT));
        assert_eq!(hits.iter().count(), 0);
    }
}
