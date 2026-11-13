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
