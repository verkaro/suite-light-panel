//! The family's skins on the console (`../STYLE.md`, "Skins"; P9, 2026-11-13): dark and 70s,
//! the two that matter most (Warren, 2026-11-08), as `as-page-kit`'s `page-kit.js` has them
//! (`SKINS.dark`, `SKINS.seventies`), by the same names, and a test holds them to it.
//!
//! The suite's parts draw in the skin this thread has ([`skin`]), dark until a panel says
//! otherwise ([`set_skin`]). Nothing here reads the environment by itself: a panel that hasn't
//! taken skins keeps drawing in dark, its own colours and the suite's alike. A panel run by
//! muntin asks [`Skin::from_frame`] which skin the console is in.
//!
//! 70s has light keys on darker panels, so anything drawn on a key goes through
//! [`Skin::on_key`], as the page's `onKey` does. Dark has no key colours: there it changes
//! nothing.

use crate::{LedColors, Rgb, dark, hex};
use std::cell::Cell;

/// What's drawn on a key in place of `text`, `dim`, `faint`, `amber`, `red` and `green`, on a
/// skin whose keys are light (`page-kit.js`'s `key`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyColors {
    pub text: Rgb,
    pub dim: Rgb,
    pub faint: Rgb,
    pub amber: Rgb,
    pub red: Rgb,
    pub green: Rgb,
}

/// A skin: every solid colour the page names, by its name (`btnEdge` is `btn_edge`), the
/// meters' three bands lit and unlit, and the key colours where the keys are light.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Skin {
    /// The page's name for it: `dark`, `seventies`. What muntin passes a panel.
    pub name: &'static str,
    /// What a button says: `DARK`, `70s` (`page-kit.js`'s `SKIN_NAMES`).
    pub title: &'static str,
    pub page: Rgb,
    pub bg: Rgb,
    pub panel: Rgb,
    pub column: Rgb,
    pub rule: Rgb,
    pub btn: Rgb,
    pub btn_hi: Rgb,
    pub btn_edge: Rgb,
    pub text: Rgb,
    pub dim: Rgb,
    pub faint: Rgb,
    pub green: Rgb,
    pub amber: Rgb,
    pub red: Rgb,
    pub lamp_off: Rgb,
    pub well: Rgb,
    pub readout: Rgb,
    pub ghost: Rgb,
    pub on_red: Rgb,
    pub arm_text: Rgb,
    pub arm_idle: Rgb,
    pub tape: Rgb,
    pub overlay: Rgb,
    pub menu: Rgb,
    /// The meters' green, amber and red, lit and unlit.
    pub lit: [Rgb; 3],
    pub unlit: [Rgb; 3],
    pub key: Option<KeyColors>,
}

impl Skin {
    /// The default everywhere: warm near-black, cream text, an amber LED counter. Its colours
    /// are [`dark`]'s, unchanged.
    pub const DARK: Skin = Skin {
        name: "dark",
        title: "DARK",
        page: dark::PAGE,
        bg: dark::BG,
        panel: dark::PANEL,
        column: dark::COLUMN,
        rule: dark::RULE,
        btn: dark::BTN,
        btn_hi: dark::BTN_HI,
        btn_edge: dark::BTN_EDGE,
        text: dark::TEXT,
        dim: dark::DIM,
        faint: dark::FAINT,
        green: dark::GREEN,
        amber: dark::AMBER,
        red: dark::RED,
        lamp_off: dark::LAMP_OFF,
        well: dark::WELL,
        readout: dark::READOUT,
        ghost: dark::GHOST,
        on_red: dark::ON_RED,
        arm_text: dark::ARM_TEXT,
        arm_idle: dark::ARM_IDLE,
        tape: dark::TAPE,
        overlay: dark::OVERLAY,
        menu: dark::MENU,
        lit: [hex("#56d364"), hex("#ffb020"), hex("#ff4b3e")],
        unlit: [hex("#1d2a1f"), hex("#352a15"), hex("#361b17")],
        key: None,
    };

    /// A broadcast console: olive panels, a walnut column, cream keys, a blue-green
    /// fluorescent counter.
    pub const SEVENTIES: Skin = Skin {
        name: "seventies",
        title: "70s",
        page: hex("#2e2b1f"),
        bg: hex("#38352a"),
        panel: hex("#6b6843"),
        column: hex("#5b3f2c"),
        rule: hex("#8f8b62"),
        btn: hex("#e2d1a4"),
        btn_hi: hex("#d2be8b"),
        btn_edge: hex("#2b2618"),
        text: hex("#eedcae"),
        dim: hex("#cbba8f"),
        faint: hex("#a39873"),
        green: hex("#b6f07a"),
        amber: hex("#ffb347"),
        red: hex("#ff6a4d"),
        lamp_off: hex("#b9a57a"),
        well: hex("#0c1311"),
        readout: hex("#5ff2d0"),
        ghost: hex("#15302a"),
        on_red: hex("#1a0806"),
        arm_text: hex("#9c2615"),
        arm_idle: hex("#4e4530"),
        tape: hex("#5ff2d0"),
        overlay: hex("#3d3b27"),
        menu: hex("#4d4a30"),
        lit: [hex("#a6f06a"), hex("#ffb347"), hex("#ff5a3d")],
        unlit: [hex("#545937"), hex("#6b5b39"), hex("#6b4635")],
        key: Some(KeyColors {
            text: hex("#231f14"),
            dim: hex("#5e5540"),
            faint: hex("#aa9e7b"),
            amber: hex("#b0520a"),
            red: hex("#b8291a"),
            green: hex("#3f7d1f"),
        }),
    };

    /// The skins on the console, in the page's order.
    pub const ALL: [&'static Skin; 2] = [&Skin::DARK, &Skin::SEVENTIES];

    /// A skin by the page's name for it.
    pub fn named(name: &str) -> Option<&'static Skin> {
        Skin::ALL.into_iter().find(|s| s.name == name)
    }

    /// The skin after this one, for a button that goes round them.
    pub fn next(&self) -> &'static Skin {
        let at = Skin::ALL.iter().position(|s| s.name == self.name).unwrap_or(0);
        Skin::ALL[(at + 1) % Skin::ALL.len()]
    }

    /// The skin muntin's console is in, as it tells a panel: `MUNTIN_SKIN`, by the page's
    /// name. Dark when it isn't set or isn't a skin here. Asked only when a panel asks.
    pub fn from_frame() -> &'static Skin {
        std::env::var("MUNTIN_SKIN").ok().and_then(|n| Skin::named(&n)).unwrap_or(&Skin::DARK)
    }

    /// A colour drawn on a key: the skin's key colours, where it has them (the page's `onKey`).
    pub fn on_key(&self, c: Rgb) -> Rgb {
        let Some(k) = self.key else { return c };
        match c {
            c if c == self.text => k.text,
            c if c == self.dim => k.dim,
            c if c == self.faint => k.faint,
            c if c == self.amber => k.amber,
            c if c == self.red => k.red,
            c if c == self.green => k.green,
            c => c,
        }
    }

    /// `n` LEDs in this skin's bands.
    pub fn leds(&self, n: usize) -> LedColors {
        LedColors::bands(n, self.lit, self.unlit)
    }

    /// Every solid colour by the page's name for it.
    pub fn named_colours(&self) -> [(&'static str, Rgb); 24] {
        [
            ("page", self.page),
            ("bg", self.bg),
            ("panel", self.panel),
            ("column", self.column),
            ("rule", self.rule),
            ("btn", self.btn),
            ("btnHi", self.btn_hi),
            ("btnEdge", self.btn_edge),
            ("text", self.text),
            ("dim", self.dim),
            ("faint", self.faint),
            ("green", self.green),
            ("amber", self.amber),
            ("red", self.red),
            ("lampOff", self.lamp_off),
            ("well", self.well),
            ("readout", self.readout),
            ("ghost", self.ghost),
            ("onRed", self.on_red),
            ("armText", self.arm_text),
            ("armIdle", self.arm_idle),
            ("tape", self.tape),
            ("overlay", self.overlay),
            ("menu", self.menu),
        ]
    }
}

thread_local! {
    static NOW: Cell<&'static Skin> = const { Cell::new(&Skin::DARK) };
}

/// The skin this thread draws in: dark until [`set_skin`]. A panel draws on one thread, so it's
/// the panel's; each test, on a thread of its own, starts in dark.
pub fn skin() -> &'static Skin {
    NOW.with(Cell::get)
}

/// Draw in `s` from now on, on this thread.
pub fn set_skin(s: &'static Skin) {
    NOW.with(|n| n.set(s));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A skin's block in page-kit.js, by its name there.
    fn block<'a>(js: &'a str, name: &str) -> &'a str {
        let open = format!("  {name}: {{");
        let at = js.find(&open).unwrap_or_else(|| panic!("SKINS.{name}")) + open.len();
        &js[at..at + js[at..].find("\n  },").expect("its end")]
    }

    /// `key: "#rrggbb"` pairs, in order.
    fn pairs(body: &str) -> Vec<(String, Rgb)> {
        let mut out = Vec::new();
        for part in body.split(',') {
            let Some((k, v)) = part.split_once(':') else { continue };
            let (k, v) = (k.trim(), v.trim().trim_matches('"'));
            if v.len() == 7 && v.starts_with('#') {
                out.push((k.to_string(), hex(v)));
            }
        }
        out
    }

    /// The three colours in `name: ladderColors("#..", "#..", "#..")`.
    fn ladder(body: &str, name: &str) -> [Rgb; 3] {
        let open = format!("{name}: ladderColors(");
        let at = body.find(&open).unwrap_or_else(|| panic!("{name}")) + open.len();
        let args = &body[at..at + body[at..].find(')').unwrap()];
        let c: Vec<Rgb> = args.split(',').map(|a| hex(a.trim().trim_matches('"'))).collect();
        [c[0], c[1], c[2]]
    }

    /// Each skin here against page-kit.js: every solid colour of the page's, the same; the key
    /// colours; the meters' bands. `tile` and `menuHi` aren't on the console yet.
    #[test]
    fn the_same_as_the_pages_skins() {
        let js =
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../../suite-audio-libs/crates/page-kit/src/page-kit.js");
        let Ok(js) = std::fs::read_to_string(js) else {
            eprintln!("no suite-audio-libs beside this suite: page-kit.js not compared");
            return;
        };
        for s in Skin::ALL {
            let body = block(&js, s.name);
            // The key colours are a block of their own, named as the outer ones are.
            let (outer, key) = match body.find("key: {") {
                Some(at) => {
                    let end = at + body[at..].find('}').unwrap() + 1;
                    (format!("{}{}", &body[..at], &body[end..]), Some(&body[at + 6..end - 1]))
                }
                None => (body.to_string(), None),
            };
            let page: Vec<_> =
                pairs(&outer).into_iter().filter(|(k, _)| k != "tile" && k != "menuHi").collect();
            let ours = s.named_colours();
            for (k, v) in &page {
                let o =
                    ours.iter().find(|(n, _)| n == k).unwrap_or_else(|| panic!("{}: {k} isn't here", s.name));
                assert_eq!(o.1, *v, "{}: {k}", s.name);
            }
            assert_eq!(page.len(), ours.len(), "{}", s.name);
            let keys = key.map(pairs).map(|p| {
                let c = |n: &str| p.iter().find(|(k, _)| k == n).unwrap_or_else(|| panic!("key.{n}")).1;
                KeyColors {
                    text: c("text"),
                    dim: c("dim"),
                    faint: c("faint"),
                    amber: c("amber"),
                    red: c("red"),
                    green: c("green"),
                }
            });
            assert_eq!(s.key, keys, "{}: key", s.name);
            assert_eq!((s.lit, s.unlit), (ladder(body, "lit"), ladder(body, "unlit")), "{}", s.name);
        }
    }

    #[test]
    fn dark_is_darks() {
        let d = Skin::DARK.named_colours();
        assert_eq!(d.len(), dark::ALL.len());
        for ((n, c), (dn, dc)) in d.iter().zip(dark::ALL) {
            assert_eq!((n, c), (&dn, &dc));
        }
        assert_eq!(Skin::DARK.leds(16), LedColors::dexter_dark());
    }

    #[test]
    fn by_name_round_and_on_keys() {
        assert_eq!(Skin::named("seventies"), Some(&Skin::SEVENTIES));
        assert_eq!(Skin::named("studio"), None);
        assert_eq!(Skin::DARK.next(), &Skin::SEVENTIES);
        assert_eq!(Skin::SEVENTIES.next(), &Skin::DARK);
        let s = &Skin::SEVENTIES;
        assert_eq!(s.on_key(s.text), hex("#231f14"));
        assert_eq!(s.on_key(s.green), hex("#3f7d1f"));
        assert_eq!(s.on_key(s.tape), s.tape);
        assert_eq!(Skin::DARK.on_key(dark::TEXT), dark::TEXT);
    }

    #[test]
    fn each_thread_starts_dark() {
        assert_eq!(skin(), &Skin::DARK);
        set_skin(&Skin::SEVENTIES);
        assert_eq!(skin(), &Skin::SEVENTIES);
        std::thread::spawn(|| assert_eq!(skin(), &Skin::DARK)).join().unwrap();
        set_skin(&Skin::DARK);
    }
}
