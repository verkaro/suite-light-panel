//! `?`: a tool's keys, over the screen (`../STYLE.md`, "Keys"). The panel's half of
//! `kitKeysOver` in `page-kit.js`, drawn from the same list the page draws from, the tool's
//! `keys.json`, which is what keeps that list honest: a key left out shows as help that's
//! missing.

use lp_keys::common::{Declared, shown};
use lp_paint::{Align, Pen, Rect, skin};

/// The keys in `list` for the screens named, in that order, in the box `r`, the rest of the
/// canvas shaded. Three columns, a screen's name over its keys. The panel closes it: Esc, `?`
/// or a click is the usual way.
pub fn keys_over(pen: &mut Pen, list: &[Declared], screens: &[&str], r: Rect) {
    let area = pen.v.area();
    pen.sf.shade(area, skin().page, 0.6);
    pen.rrect(r, 6.0, skin().overlay, skin().rule);
    let head = Rect::new(r.x + 16, r.y + 10, r.w - 32, 20);
    pen.text(head, "THE KEYS", 13.0, true, skin().text, Align::Left);
    pen.text(head, "Esc or ? closes", 10.0, false, skin().faint, Align::Right);
    let col = (r.w - 32) / 3;
    const LINE: i32 = 18;
    let top = r.y + 42;
    let (mut x, mut y) = (r.x + 16, top);
    // The keys' column: as wide as the widest key shown ("Alt+Tab"), never under 44 or over half.
    let widest = list
        .iter()
        .filter(|d| screens.contains(&d.screen.as_str()))
        .map(|d| pen.width(shown(&d.key), 11.0, true).ceil() as i32)
        .max()
        .unwrap_or(0);
    let kw = widest.clamp(44, col / 2);
    let next = |x: &mut i32, y: &mut i32| {
        *y += LINE;
        if *y + LINE > r.bottom() - 10 {
            *y = top;
            *x += col;
        }
    };
    for screen in screens {
        let mut here = list.iter().filter(|d| d.screen == *screen).peekable();
        if here.peek().is_none() {
            continue;
        }
        pen.text(
            Rect::new(x, y, col - 8, LINE),
            &screen.to_uppercase(),
            10.0,
            true,
            skin().amber,
            Align::Left,
        );
        next(&mut x, &mut y);
        for d in here {
            pen.text(Rect::new(x, y, kw, LINE), shown(&d.key), 11.0, true, skin().text, Align::Left);
            let does = if d.note.is_empty() { d.does.clone() } else { format!("{} · {}", d.does, d.note) };
            pen.text(
                Rect::new(x + kw + 4, y, col - kw - 12, LINE),
                &does,
                11.0,
                false,
                skin().dim,
                Align::Left,
            );
            next(&mut x, &mut y);
        }
        next(&mut x, &mut y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_paint::dark;
    use lp_paint::{Fonts, Image, View};

    fn entry(key: &str, does: &str, note: &str, screen: &str) -> Declared {
        Declared {
            key: key.into(),
            does: does.into(),
            note: note.into(),
            screen: screen.into(),
            differs: String::new(),
        }
    }

    #[test]
    fn the_keys_by_screen_over_a_shaded_canvas() {
        let Ok(fs) = Fonts::terminus(None) else { return };
        let mut img = Image::new(600, 300);
        img.px.fill(dark::PANEL);
        let mut sf = img.surface();
        let mut p = Pen { sf: &mut sf, fonts: &fs, v: View::fit(600, 300, 600, 300) };
        let mut list = vec![
            entry("j", "the next", "a room", "library"),
            entry(" ", "play and stop", "", "library"),
            entry("i", "start changing this in place", "a field", "tags"),
            entry("z", "never shown", "", "nowhere"),
        ];
        // Enough to run into a second column.
        list.extend((0..14).map(|i| entry("x", &format!("thing {i}"), "", "tags")));
        let r = Rect::new(40, 20, 520, 260);
        keys_over(&mut p, &list, &["library", "tags", "empty"], r);
        let count = |r: Rect, c: u32| {
            (r.y..r.bottom())
                .flat_map(|y| (r.x..r.right()).map(move |x| (x, y)))
                .filter(|(x, y)| img.px[*y as usize * 600 + *x as usize] == c)
                .count()
        };
        assert_ne!(img.px[5 * 600 + 5], dark::PANEL, "the rest is shaded");
        assert_eq!(count(Rect::new(0, 0, 40, 300), dark::TEXT), 0);
        let col = (520 - 32) / 3;
        let first = Rect::new(56, 62, col, 210);
        let second = Rect::new(56 + col, 62, col, 210);
        let third = Rect::new(56 + 2 * col, 62, col, 210);
        assert!(count(first, dark::AMBER) > 0 && count(first, dark::TEXT) > 0 && count(first, dark::DIM) > 0);
        assert!(count(second, dark::DIM) > 0, "what doesn't fit goes on in the next column");
        assert_eq!(count(third, dark::DIM), 0);
        assert!(count(Rect::new(56, 30, 200, 20), dark::TEXT) > 0, "its title");
    }
}
