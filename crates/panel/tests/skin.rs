//! The parts in 70s (P9, 2026-11-13): the parts example drawn in the 70s skin, with no pixel
//! a colour only dark has, so no part still names dark's colours. `PARTS_PNG=DIR` keeps the
//! pictures, to look at.

#[allow(dead_code)]
#[path = "../examples/parts.rs"]
mod parts;

use lp_paint::{Fonts, Image, Skin, set_skin};
use std::collections::HashSet;

/// Every pixel's colour, the parts drawn in `s`, with the keys over them or not.
fn drawn(s: &'static Skin, keys: bool, name: &str) -> HashSet<u32> {
    set_skin(s);
    let fonts = Fonts::terminus(None).expect("the Terminus fonts (console-setup-linux)");
    let mut img = Image::new(1400, 1340);
    let mut sf = img.surface();
    parts::draw(&mut sf, &fonts, keys).unwrap();
    if let Some(d) = std::env::var_os("PARTS_PNG") {
        lp_paint::write_png(&std::path::Path::new(&d).join(format!("parts-{name}.png")), &sf).unwrap();
    }
    img.px.iter().copied().collect()
}

#[test]
fn no_part_draws_dark_in_seventies() {
    let (d, s) = (&Skin::DARK, &Skin::SEVENTIES);
    let theirs: HashSet<u32> =
        s.named_colours().iter().map(|(_, c)| *c).chain(s.lit).chain(s.unlit).collect();
    let only_dark: Vec<(&str, u32)> = d
        .named_colours()
        .into_iter()
        .chain(d.lit.map(|c| ("a lit LED", c)))
        .chain(d.unlit.map(|c| ("an unlit LED", c)))
        .filter(|(_, c)| !theirs.contains(c))
        // A disc LED's hot red is its own light, the remote's, in every skin; it's dark's red too.
        .filter(|(_, c)| *c != lp_paint::hex("#ff4b3e"))
        .collect();
    let dark = drawn(d, false, "dark");
    let seventies = drawn(s, false, "seventies");
    let over = drawn(s, true, "seventies-keys");
    // Dark's own colours are drawn in dark, so the check below has something to find.
    let found: Vec<_> = only_dark.iter().filter(|(_, c)| dark.contains(c)).collect();
    assert!(found.len() >= 6, "{found:?}");
    let left: Vec<_> = only_dark.iter().filter(|(_, c)| seventies.contains(c) || over.contains(c)).collect();
    assert!(left.is_empty(), "dark's colours in 70s: {left:?}");
    // And 70s's key colours are used: what's on a key is drawn as on a key.
    assert!(seventies.contains(&s.key.unwrap().text), "key text");
}

/// The sheet in `s`, as pixels.
fn pixels(s: &'static Skin) -> Vec<u32> {
    set_skin(s);
    let fonts = Fonts::terminus(None).expect("the Terminus fonts (console-setup-linux)");
    let mut img = Image::new(1400, 1340);
    let mut sf = img.surface();
    parts::draw(&mut sf, &fonts, false).unwrap();
    img.px
}

/// The studio's two desks differ in their desk and in nothing a track is drawn on: a shell
/// meter's card, its LEDs and its numbers are the same pixels on a light desk as on a dark one.
#[test]
fn a_meter_is_the_same_on_both_studio_desks() {
    let (dark, light) = (pixels(&Skin::STUDIO_DARK), pixels(&Skin::STUDIO_LIGHT));
    set_skin(&Skin::DARK);
    // The cards the sheet draws (`parts.rs`): inside each, the hairline and the corners left.
    let cards = [(14, 876, 212, 212), (14, 1096, 212, 212), (1174, 876, 212, 212)];
    for (x, y, w, h) in cards {
        for py in y + 12..y + h - 12 {
            for px in x + 12..x + w - 12 {
                assert_eq!(dark[py * 1400 + px], light[py * 1400 + px], "({px}, {py}) inside a card");
            }
        }
    }
    // And the desks themselves are not the same picture: the page, the groups, the text.
    let differing = dark.iter().zip(&light).filter(|(a, b)| a != b).count();
    assert!(differing > 400_000, "only {differing} pixels differ between the desks");
    // The light desk is light: its ground, where the sheet draws on it.
    assert_eq!(light[5 * 1400 + 700], Skin::STUDIO_LIGHT.bg);
}
