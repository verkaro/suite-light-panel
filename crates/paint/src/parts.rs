//! The family's parts that a panel is made of (`../STYLE.md`, "The parts"), drawn with the
//! [`Pen`] in the dark skin: a key, and a form's field.

use crate::hits::bar_name;
use crate::{Align, Hits, Pen, Rect, Rgb, dark};

/// How a key looks. The default is a plain key that can be pressed, listed in the bar by its
/// label.
#[derive(Clone, Copy, Debug, Default)]
pub struct KeyLook<'a> {
    /// Lit: filled with this colour, its text the skin's `onRed`.
    pub lit: Option<Rgb>,
    /// Not to be pressed now: its text faint, a click passing it by, faint in the bar.
    pub faint: bool,
    /// Its text's colour, when it isn't the skin's `text`.
    pub color: Option<Rgb>,
    /// What the bar calls it, for a key whose label isn't a word.
    pub name: Option<&'a str>,
    /// Left out of the bar.
    pub off_bar: bool,
}

/// A form's field, and where the hand is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FieldState {
    #[default]
    Plain,
    /// The cursor is on it.
    Cursor,
    /// Being typed in, the caret before this character.
    Typing { caret: usize },
}

/// A form's field: a label, and its value in a box.
#[derive(Clone, Copy, Debug, Default)]
pub struct Field<'a> {
    pub label: &'a str,
    pub value: &'a str,
    /// Said faint in an empty box: what goes here.
    pub hint: &'a str,
    /// Changed and not yet saved: amber, as every waiting change is.
    pub waiting: bool,
    pub state: FieldState,
    /// How much of the width is the label's, in logical pixels.
    pub label_w: i32,
    /// The text's height, in logical pixels.
    pub size: f32,
}

impl Pen<'_, '_> {
    /// A key (`page-kit.js`'s `key`): the skin's `btn` with a `btnEdge`, corners 3, its label
    /// centred. With an id it's somewhere to click, and the bar lists it.
    pub fn key(&mut self, hits: &mut Hits, r: Rect, label: &str, id: &str, look: KeyLook) {
        self.rrect(r, 3.0, look.lit.unwrap_or(dark::BTN), look.lit.unwrap_or(dark::BTN_EDGE));
        let color = match (look.lit, look.faint) {
            (Some(_), _) => dark::ON_RED,
            (None, true) => dark::FAINT,
            (None, false) => look.color.unwrap_or(dark::TEXT),
        };
        self.text(r.inset(2), label, 10.0, true, color, Align::Centre);
        if id.is_empty() {
            return;
        }
        if look.off_bar {
            if !look.faint {
                hits.add(r, id);
            }
        } else {
            let name = look.name.map_or_else(|| bar_name(label), str::to_string);
            hits.key(r, id, &name, look.faint);
        }
    }

    /// A field in `r`: its label at the left, its value in a box across the rest.
    ///
    /// - plain: the value in `text`, in a `well`;
    /// - under the cursor: the box is `btnHi`, as a chosen row is;
    /// - being typed in: an amber edge, the value in `readout`, a caret, and as much of a
    ///   long value as fits around the caret;
    /// - waiting: the value amber, with a mark by the label, until it's saved or dropped.
    pub fn field(&mut self, r: Rect, f: &Field) {
        let label = Rect::new(r.x, r.y, f.label_w.min(r.w), r.h);
        let boxr = Rect::new(r.x + label.w, r.y, r.w - label.w, r.h);
        let typing = matches!(f.state, FieldState::Typing { .. });
        let label_color = if f.state == FieldState::Plain { dark::DIM } else { dark::TEXT };
        self.text(
            Rect::new(label.x, label.y, label.w - 6, label.h),
            f.label,
            9.0,
            true,
            label_color,
            Align::Left,
        );
        if f.waiting {
            // The mark: a small amber bar where the label ends.
            self.fill(Rect::new(boxr.x - 5, r.y + 3, 2, r.h - 6), dark::AMBER);
        }
        let (fill, edge) = match f.state {
            FieldState::Plain => (dark::WELL, dark::RULE),
            FieldState::Cursor => (dark::BTN_HI, dark::BTN_EDGE),
            FieldState::Typing { .. } => (dark::WELL, dark::AMBER),
        };
        self.rrect(boxr, 3.0, fill, edge);
        let inner = Rect::new(boxr.x + 6, boxr.y, boxr.w - 12, boxr.h);
        if f.value.is_empty() && !typing {
            self.text(inner, f.hint, f.size, false, dark::FAINT, Align::Left);
            return;
        }
        let color = if typing {
            dark::READOUT
        } else if f.waiting {
            dark::AMBER
        } else {
            dark::TEXT
        };
        let FieldState::Typing { caret } = f.state else {
            self.text(inner, f.value, f.size, false, color, Align::Left);
            return;
        };
        // Typed: never shrunk or cut with …, but slid so the caret shows.
        let chars: Vec<char> = f.value.chars().collect();
        let caret = caret.min(chars.len());
        let each = self.width("M", f.size, false).max(1.0);
        let fit = ((inner.w as f32 / each) as usize).max(2);
        let from = if caret >= fit { caret + 1 - fit } else { 0 };
        let shown: String = chars[from..].iter().take(fit).collect();
        self.run(inner.x as f32, inner, &shown, f.size, false, color);
        let at = inner.x as f32 + (caret - from) as f32 * each;
        let tall = (f.size + 2.0).min(r.h as f32 - 4.0);
        let top = r.y as f32 + (r.h as f32 - tall) / 2.0;
        let d = Rect::new(self.v.x(at), self.v.y(top), self.v.len(1.5), self.v.len(tall));
        self.sf.fill(d, dark::AMBER);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pen::tests::{fonts, has};
    use crate::{Image, View};

    fn drawn(w: i32, h: i32, f: impl FnOnce(&mut Pen)) -> Option<Image> {
        let fs = fonts()?;
        let mut img = Image::new(w as usize, h as usize);
        let mut sf = img.surface();
        let mut p = Pen { sf: &mut sf, fonts: &fs, v: View::fit(w, h, w as usize, h as usize) };
        f(&mut p);
        Some(img)
    }

    #[test]
    fn a_key_is_drawn_and_can_be_clicked_and_named() {
        let mut hits = Hits::new();
        hits.section("ROOM");
        let Some(img) = drawn(300, 30, |p| {
            p.key(&mut hits, Rect::new(0, 0, 90, 30), "KEEP ▾", "keep", KeyLook::default());
            let lit = KeyLook { lit: Some(dark::GREEN), name: Some("play"), ..KeyLook::default() };
            p.key(&mut hits, Rect::new(100, 0, 90, 30), "▶", "play", lit);
            p.key(
                &mut hits,
                Rect::new(200, 0, 90, 30),
                "EXPORT",
                "export",
                KeyLook { faint: true, ..KeyLook::default() },
            );
        }) else {
            return;
        };
        assert!(
            has(&img, Rect::new(0, 0, 90, 30), dark::BTN) && has(&img, Rect::new(0, 0, 90, 30), dark::TEXT)
        );
        assert!(
            has(&img, Rect::new(100, 0, 90, 30), dark::GREEN)
                && has(&img, Rect::new(100, 0, 90, 30), dark::ON_RED)
        );
        assert!(
            has(&img, Rect::new(200, 0, 90, 30), dark::FAINT)
                && !has(&img, Rect::new(200, 0, 90, 30), dark::TEXT)
        );
        assert_eq!(hits.at(10.0, 10.0), Some("keep"));
        assert_eq!(hits.at(210.0, 10.0), None, "a faint key can't be clicked");
        let names: Vec<(&str, &str, bool)> =
            hits.iter().map(|h| (h.name.as_str(), h.section.as_str(), h.faint)).collect();
        assert_eq!(names, [("keep", "ROOM", false), ("play", "ROOM", false), ("export", "ROOM", true)]);
    }

    #[test]
    fn a_field_says_where_the_hand_is_and_what_waits() {
        let field = |value: &'static str, waiting, state| Field {
            label: "KIND",
            value,
            hint: "what the place is",
            waiting,
            state,
            label_w: 60,
            size: 12.0,
        };
        let all = Rect::new(0, 0, 240, 24);
        let draw = |f: Field<'static>| drawn(240, 24, |p| p.field(all, &f));
        let Some(plain) = draw(field("stairwell", false, FieldState::Plain)) else { return };
        assert!(has(&plain, all, dark::WELL) && has(&plain, all, dark::TEXT) && has(&plain, all, dark::DIM));
        assert!(!has(&plain, all, dark::AMBER));
        let empty = draw(field("", false, FieldState::Plain)).unwrap();
        assert!(has(&empty, all, dark::FAINT) && !has(&empty, Rect::new(60, 0, 180, 24), dark::TEXT));
        let cursor = draw(field("stairwell", false, FieldState::Cursor)).unwrap();
        assert!(has(&cursor, all, dark::BTN_HI));
        let waiting = draw(field("stairwell", true, FieldState::Cursor)).unwrap();
        assert!(has(&waiting, Rect::new(60, 0, 180, 24), dark::AMBER), "a waiting value is amber");
        assert!(has(&waiting, Rect::new(50, 0, 10, 24), dark::AMBER), "and marked by its label");
        let typing = draw(field("stair", false, FieldState::Typing { caret: 5 })).unwrap();
        assert!(has(&typing, all, dark::READOUT) && has(&typing, all, dark::AMBER));
        // A value longer than the box: the caret, at its end, still shows inside the box.
        const LONG: &str = "a very long note about the stairwell behind the old hall, top landing";
        let long = draw(field(LONG, false, FieldState::Typing { caret: LONG.len() })).unwrap();
        let caret_x = (66..234).rev().find(|&x| (7..17).all(|y| long.px[y * 240 + x] == dark::AMBER));
        assert!(caret_x.is_some_and(|x| x > 150), "{caret_x:?}");
    }
}
