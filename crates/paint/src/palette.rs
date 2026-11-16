//! Palettes: sixteen colours, by role, made once and held to contrast targets (Warren,
//! 2026-11-16, for capstan's client; family continuity).
//!
//! A palette is **six neutrals that change with the desk** (dark or light) and **ten colours
//! that never do**:
//!
//! | | slots |
//! |---|---|
//! | the desk | `ground`, `raised`, `edge`, `faint`, `dim`, `text` |
//! | the meter window | `well` |
//! | signals | `green`, `amber`, `red` |
//! | families | `orange`, `lilac`, `sky`, `peri`, `sand`, and `focus` for a selection |
//!
//! So a meter looks the same on a dark desk and a light one, and a family means the same on
//! both: the ten are pastels, fills with dark ink (a lit key, a tab). Where a family or a signal
//! is drawn thin on the desk itself, as an outline or a number, it is [`OnDesk`]: its hue and
//! chroma kept and its lightness solved to 7:1 against that desk's ground, so it stays legible
//! on a light desk where the pastel would not.
//!
//! The colours are made in OKLCH: a hue and a chroma, and a lightness solved to hit a contrast.
//! [`Palette::generate`] is the recipe, and [`Palette::STUDIO_DARK`] and [`Palette::STUDIO_LIGHT`]
//! are what it makes, written out so they are stable and can be read; a test holds each to
//! the recipe. `cargo run -p lp-paint --example palette` prints them again.
//!
//! The full inversion, light meter windows on a light desk, is left undecided: the window
//! is a skin's `window` ([`crate::Skin`]), so it can still be done by a palette that wants it.

use crate::{Rgb, contrast};

/// A palette's colours drawn thin on the desk: the signals and the families, their lightness
/// solved to 7:1 against the desk's ground.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OnDesk {
    pub green: Rgb,
    pub amber: Rgb,
    pub red: Rgb,
    pub orange: Rgb,
    pub lilac: Rgb,
    pub sky: Rgb,
    pub peri: Rgb,
    pub sand: Rgb,
    pub focus: Rgb,
}

/// Sixteen colours, by role; see the module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub name: &'static str,
    /// A dark desk: its text is lighter than its ground.
    pub dark: bool,
    pub ground: Rgb,
    pub raised: Rgb,
    pub edge: Rgb,
    pub faint: Rgb,
    pub dim: Rgb,
    pub text: Rgb,
    pub well: Rgb,
    pub green: Rgb,
    pub amber: Rgb,
    pub red: Rgb,
    pub orange: Rgb,
    pub lilac: Rgb,
    pub sky: Rgb,
    pub peri: Rgb,
    pub sand: Rgb,
    pub focus: Rgb,
    pub on: OnDesk,
}

/// `a` towards `b` by `permille` thousandths, in whole numbers, so a palette's skin can be a
/// constant ([`crate::mix`] is the same in floats).
pub const fn lean(a: Rgb, b: Rgb, permille: u32) -> Rgb {
    const fn channel(a: Rgb, b: Rgb, p: u32, s: u32) -> u32 {
        let (x, y) = ((a >> s) & 0xff, (b >> s) & 0xff);
        ((x * (1000 - p) + y * p + 500) / 1000) << s
    }
    let p = if permille > 1000 { 1000 } else { permille };
    channel(a, b, p, 16) | channel(a, b, p, 8) | channel(a, b, p, 0)
}

fn decode(c: u32) -> f64 {
    let c = c as f64 / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn encode(x: f64) -> u32 {
    let x = x.clamp(0.0, 1.0);
    let v = if x <= 0.003_130_8 { 12.92 * x } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 };
    (v * 255.0).round() as u32
}

/// OKLCH (lightness 0 to 1, chroma, hue in degrees) as linear sRGB, unclipped.
fn linear(l: f64, c: f64, h: f64) -> (f64, f64, f64) {
    let (a, b) = (c * h.to_radians().cos(), c * h.to_radians().sin());
    let l_ = (l + 0.396_337_777_4 * a + 0.215_803_757_3 * b).powi(3);
    let m_ = (l - 0.105_561_345_8 * a - 0.063_854_172_8 * b).powi(3);
    let s_ = (l - 0.089_484_177_5 * a - 1.291_485_548_0 * b).powi(3);
    (
        4.076_741_662_1 * l_ - 3.307_711_591_3 * m_ + 0.230_969_929_2 * s_,
        -1.268_438_004_6 * l_ + 2.609_757_401_1 * m_ - 0.341_319_396_5 * s_,
        -0.004_196_086_3 * l_ - 0.703_418_614_7 * m_ + 1.707_614_701_0 * s_,
    )
}

/// A colour in OKLCH, in sRGB. One out of the gamut has its chroma brought down, its lightness
/// and hue kept.
pub fn oklch(l: f64, c: f64, h: f64) -> Rgb {
    let mut c = c;
    for _ in 0..60 {
        let (r, g, b) = linear(l, c, h);
        if [r, g, b].iter().all(|v| (-1e-4..=1.0 + 1e-4).contains(v)) {
            break;
        }
        c *= 0.93;
    }
    let (r, g, b) = linear(l, c, h);
    encode(r) << 16 | encode(g) << 8 | encode(b)
}

/// A colour's OKLCH: lightness, chroma, hue in degrees.
pub fn to_oklch(c: Rgb) -> (f64, f64, f64) {
    let (r, g, b) = (decode(c >> 16 & 0xff), decode(c >> 8 & 0xff), decode(c & 0xff));
    let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
    let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
    let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
    let big_l = 0.210_454_255_3 * l + 0.793_617_785_0 * m - 0.004_072_046_8 * s;
    let a = 1.977_998_495_1 * l - 2.428_592_205_0 * m + 0.450_593_709_9 * s;
    let b = 0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766_0 * s;
    (big_l, a.hypot(b), b.atan2(a).to_degrees().rem_euclid(360.0))
}

/// The colour of this hue and chroma whose contrast with `ground` is `target`: lighter than it
/// on a dark ground, darker on a light one.
pub fn solve(hue: f64, chroma: f64, ground: Rgb, target: f32, lighter: bool) -> Rgb {
    let (mut lo, mut hi) = (0.0f64, 1.0f64);
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        let enough = contrast(oklch(mid, chroma, hue), ground) >= target;
        // Contrast grows as the lightness moves away from the ground's.
        if lighter == enough { hi = mid } else { lo = mid }
    }
    oklch((lo + hi) / 2.0, chroma, hue)
}

/// The signals' and the families' hue and chroma: the recipe's own numbers.
const GREEN: (f64, f64) = (145.0, 0.17);
const AMBER: (f64, f64) = (80.0, 0.15);
const RED: (f64, f64) = (28.0, 0.19);
const ORANGE: (f64, f64) = (58.0, 0.115);
const LILAC: (f64, f64) = (325.0, 0.09);
const SKY: (f64, f64) = (235.0, 0.085);
const PERI: (f64, f64) = (280.0, 0.10);
const SAND: (f64, f64) = (95.0, 0.075);
const FOCUS: (f64, f64) = (192.0, 0.10);

/// What the desk's neutrals are held to against its ground: `raised`, `edge`, `faint`, `dim`,
/// `text`. The same on both desks, which is what makes them predictable.
pub const DESK_CONTRAST: [f32; 5] = [1.14, 1.8, 3.0, 6.5, 14.0];
/// What the pastels are held to against the meter window.
pub const SIGNAL_CONTRAST: [f32; 3] = [8.5, 12.0, 5.5]; // green, amber, red
pub const FAMILY_CONTRAST: f32 = 9.0;
/// What a colour drawn thin on the desk is held to against its ground.
pub const ON_DESK_CONTRAST: f32 = 7.0;

impl Palette {
    /// The recipe: the palette for a dark or a light desk.
    pub fn generate(dark: bool) -> Palette {
        let ground = if dark { oklch(0.16, 0.008, 75.0) } else { oklch(0.955, 0.013, 85.0) };
        let neutral = |t: f32| solve(80.0, 0.010, ground, t, dark);
        let well = oklch(0.12, 0.006, 75.0);
        let pastel = |(h, c): (f64, f64), t: f32| solve(h, c, well, t, true);
        let thin = |(h, c): (f64, f64)| solve(h, c, ground, ON_DESK_CONTRAST, dark);
        Palette {
            name: if dark { "studio" } else { "studio-light" },
            dark,
            ground,
            raised: neutral(DESK_CONTRAST[0]),
            edge: neutral(DESK_CONTRAST[1]),
            faint: neutral(DESK_CONTRAST[2]),
            dim: neutral(DESK_CONTRAST[3]),
            text: neutral(DESK_CONTRAST[4]),
            well,
            green: pastel(GREEN, SIGNAL_CONTRAST[0]),
            amber: pastel(AMBER, SIGNAL_CONTRAST[1]),
            red: pastel(RED, SIGNAL_CONTRAST[2]),
            orange: pastel(ORANGE, FAMILY_CONTRAST),
            lilac: pastel(LILAC, FAMILY_CONTRAST),
            sky: pastel(SKY, FAMILY_CONTRAST),
            peri: pastel(PERI, FAMILY_CONTRAST),
            sand: pastel(SAND, FAMILY_CONTRAST),
            focus: pastel(FOCUS, FAMILY_CONTRAST),
            on: OnDesk {
                green: thin(GREEN),
                amber: thin(AMBER),
                red: thin(RED),
                orange: thin(ORANGE),
                lilac: thin(LILAC),
                sky: thin(SKY),
                peri: thin(PERI),
                sand: thin(SAND),
                focus: thin(FOCUS),
            },
        }
    }

    /// The slots by name, in order: the sixteen, then the nine [`OnDesk`] colours as `on.NAME`.
    pub fn slots(&self) -> [(&'static str, Rgb); 25] {
        [
            ("ground", self.ground),
            ("raised", self.raised),
            ("edge", self.edge),
            ("faint", self.faint),
            ("dim", self.dim),
            ("text", self.text),
            ("well", self.well),
            ("green", self.green),
            ("amber", self.amber),
            ("red", self.red),
            ("orange", self.orange),
            ("lilac", self.lilac),
            ("sky", self.sky),
            ("peri", self.peri),
            ("sand", self.sand),
            ("focus", self.focus),
            ("on.green", self.on.green),
            ("on.amber", self.on.amber),
            ("on.red", self.on.red),
            ("on.orange", self.on.orange),
            ("on.lilac", self.on.lilac),
            ("on.sky", self.on.sky),
            ("on.peri", self.on.peri),
            ("on.sand", self.on.sand),
            ("on.focus", self.on.focus),
        ]
    }

    /// A dark desk: warm black, cream text.
    pub const STUDIO_DARK: Palette = Palette {
        name: "studio",
        dark: true,
        ground: 0x0f0d09,
        raised: 0x1e1b17,
        edge: 0x403d38,
        faint: 0x615d58,
        dim: 0x99958f,
        text: 0xdfdad4,
        well: 0x070604,
        green: 0x52bd5a,
        amber: 0xfbbc43,
        red: 0xe95045,
        orange: 0xe69d63,
        lilac: 0xce9dd0,
        sky: 0x74b5da,
        peri: 0xa2a7ed,
        sand: 0xbbad75,
        focus: 0x4fbeba,
        on: OnDesk {
            green: 0x44b14e,
            amber: 0xca9215,
            red: 0xfd6e60,
            orange: 0xd38a51,
            lilac: 0xbc8bbe,
            sky: 0x63a3c7,
            peri: 0x9196da,
            sand: 0xa99b65,
            focus: 0x3aaca8,
        },
    };

    /// A light desk: warm paper, dark text. The meter window, the signals and the families are
    /// the dark desk's, to the bit.
    pub const STUDIO_LIGHT: Palette = Palette {
        name: "studio-light",
        dark: false,
        ground: 0xf4f0e7,
        raised: 0xe6e2db,
        edge: 0xb8b4ae,
        faint: 0x8e8a84,
        dim: 0x595650,
        text: 0x24221d,
        well: 0x070604,
        green: 0x52bd5a,
        amber: 0xfbbc43,
        red: 0xe95045,
        orange: 0xe69d63,
        lilac: 0xce9dd0,
        sky: 0x74b5da,
        peri: 0xa2a7ed,
        sand: 0xbbad75,
        focus: 0x4fbeba,
        on: OnDesk {
            green: 0x0a5f19,
            amber: 0x6b4b01,
            red: 0xa31111,
            orange: 0x7b430c,
            lilac: 0x6d4170,
            sky: 0x0f5678,
            peri: 0x494b88,
            sand: 0x5e5119,
            focus: 0x0c5a58,
        },
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Skin;

    fn pair(dark: bool) -> (Palette, Palette) {
        (Palette::generate(dark), if dark { Palette::STUDIO_DARK } else { Palette::STUDIO_LIGHT })
    }

    /// The ten colours that never change with the desk, by name.
    fn invariants(p: &Palette) -> [(&'static str, Rgb); 10] {
        [
            ("well", p.well),
            ("green", p.green),
            ("amber", p.amber),
            ("red", p.red),
            ("orange", p.orange),
            ("lilac", p.lilac),
            ("sky", p.sky),
            ("peri", p.peri),
            ("sand", p.sand),
            ("focus", p.focus),
        ]
    }

    fn within(a: Rgb, b: Rgb, d: u32) -> bool {
        [16, 8, 0].iter().all(|&s| ((a >> s) & 0xff).abs_diff((b >> s) & 0xff) <= d)
    }

    #[test]
    fn the_written_out_tables_are_what_the_recipe_makes() {
        // To a bit of rounding: the recipe is floating point, and the table is what it made.
        for dark in [true, false] {
            let (made, held) = pair(dark);
            assert_eq!((made.name, made.dark), (held.name, held.dark));
            for ((name, a), (_, b)) in made.slots().iter().zip(held.slots().iter()) {
                assert!(
                    within(*a, *b, 1),
                    "{} {name}: the recipe makes {a:06x}, the table has {b:06x}",
                    held.name
                );
            }
        }
    }

    #[test]
    fn the_desks_differ_only_in_their_neutrals() {
        let (d, l) = (Palette::STUDIO_DARK, Palette::STUDIO_LIGHT);
        assert_eq!(invariants(&d), invariants(&l), "ten colours, to the bit");
        for ((n, a), (_, b)) in [
            ("ground", (d.ground, l.ground)),
            ("raised", (d.raised, l.raised)),
            ("edge", (d.edge, l.edge)),
            ("faint", (d.faint, l.faint)),
            ("dim", (d.dim, l.dim)),
            ("text", (d.text, l.text)),
        ]
        .map(|(n, (a, b))| ((n, a), (n, b)))
        {
            assert_ne!(a, b, "{n} is the desk's");
        }
    }

    #[test]
    fn each_desk_holds_its_contrast_ladder() {
        for p in [Palette::STUDIO_DARK, Palette::STUDIO_LIGHT] {
            let on_ground = [p.raised, p.edge, p.faint, p.dim, p.text];
            for (c, want) in on_ground.iter().zip(DESK_CONTRAST) {
                let got = contrast(*c, p.ground);
                assert!(
                    got >= want * 0.97 && got <= want * 1.06,
                    "{}: {c:06x} is {got:.2}:1, held to {want}",
                    p.name
                );
            }
            for (name, c) in [
                ("green", p.on.green),
                ("amber", p.on.amber),
                ("red", p.on.red),
                ("orange", p.on.orange),
                ("lilac", p.on.lilac),
                ("sky", p.on.sky),
                ("peri", p.on.peri),
                ("sand", p.on.sand),
                ("focus", p.on.focus),
            ] {
                assert!(contrast(c, p.ground) >= ON_DESK_CONTRAST - 0.15, "{} {name} on the desk", p.name);
            }
            // The pastels, against the window they sit in, equally heavy: one hue isn't louder.
            let sig =
                [(p.green, SIGNAL_CONTRAST[0]), (p.amber, SIGNAL_CONTRAST[1]), (p.red, SIGNAL_CONTRAST[2])];
            let fam = [p.orange, p.lilac, p.sky, p.peri, p.sand, p.focus].map(|c| (c, FAMILY_CONTRAST));
            for (c, want) in sig.into_iter().chain(fam) {
                let got = contrast(c, p.well);
                assert!(
                    (got - want).abs() <= want * 0.04,
                    "{}: {c:06x} is {got:.2}:1 on the window, held to {want}",
                    p.name
                );
            }
        }
    }

    #[test]
    fn the_signals_are_told_apart_by_lightness_as_well_as_hue() {
        for p in [Palette::STUDIO_DARK, Palette::STUDIO_LIGHT] {
            let l = |c| contrast(c, 0);
            assert!(
                l(p.amber) > l(p.green) && l(p.green) > l(p.red),
                "{}: amber, then green, then red",
                p.name
            );
            // And by a good margin: not a hair, which a colour-blind reader couldn't see.
            assert!(contrast(p.amber, p.green) >= 1.3 && contrast(p.green, p.red) >= 1.3, "{}", p.name);
        }
    }

    #[test]
    fn the_neutrals_are_warm_on_both_desks() {
        for p in [Palette::STUDIO_DARK, Palette::STUDIO_LIGHT] {
            for c in [p.ground, p.raised, p.edge, p.faint, p.dim, p.text, p.well] {
                let (r, g, b) = (c >> 16 & 0xff, c >> 8 & 0xff, c & 0xff);
                assert!(r >= g && g >= b, "{}: {c:06x} leans cold", p.name);
            }
        }
    }

    #[test]
    fn a_colour_drawn_thin_keeps_the_hue_of_its_pastel() {
        let turn = |a: f64, b: f64| ((a - b + 540.0) % 360.0 - 180.0).abs();
        for p in [Palette::STUDIO_DARK, Palette::STUDIO_LIGHT] {
            for (name, pastel, thin) in [
                ("green", p.green, p.on.green),
                ("amber", p.amber, p.on.amber),
                ("red", p.red, p.on.red),
                ("orange", p.orange, p.on.orange),
                ("lilac", p.lilac, p.on.lilac),
                ("sky", p.sky, p.on.sky),
                ("peri", p.peri, p.on.peri),
                ("sand", p.sand, p.on.sand),
                ("focus", p.focus, p.on.focus),
            ] {
                let (hp, ht) = (to_oklch(pastel).2, to_oklch(thin).2);
                assert!(turn(hp, ht) <= 8.0, "{} {name}: {hp:.0}° became {ht:.0}°", p.name);
            }
        }
    }

    #[test]
    fn the_colour_maths_is_right() {
        assert_eq!(oklch(1.0, 0.0, 0.0), 0xffffff);
        assert_eq!(oklch(0.0, 0.0, 0.0), 0x000000);
        // A round trip, for a colour inside the gamut.
        let (l, c, h) = to_oklch(oklch(0.7, 0.1, 200.0));
        assert!((l - 0.7).abs() < 0.005 && (c - 0.1).abs() < 0.005 && (h - 200.0).abs() < 1.0, "{l} {c} {h}");
        // Out of the gamut it keeps its lightness and hue and gives up chroma.
        let (l, c, h) = to_oklch(oklch(0.6, 0.45, 145.0));
        assert!((l - 0.6).abs() < 0.01 && c < 0.45 && (h - 145.0).abs() < 3.0, "{l} {c} {h}");
        // Solved, it lands on its target either side of the ground.
        let dark_ground = 0x0f0d09;
        let c = solve(58.0, 0.115, dark_ground, 9.0, true);
        assert!((contrast(c, dark_ground) - 9.0).abs() < 0.1);
        let light_ground = 0xf4f0e7;
        let c = solve(58.0, 0.115, light_ground, 9.0, false);
        assert!((contrast(c, light_ground) - 9.0).abs() < 0.1);
    }

    #[test]
    fn lean_is_mix_in_whole_numbers() {
        assert_eq!(lean(0x000000, 0xffffff, 500), 0x808080);
        assert_eq!(lean(0x102030, 0xf0e0d0, 0), 0x102030);
        assert_eq!(lean(0x102030, 0xf0e0d0, 1000), 0xf0e0d0);
        assert_eq!(lean(0x102030, 0xf0e0d0, 5000), 0xf0e0d0, "past the end is the end");
        for permille in [30, 170, 550, 900] {
            let (a, b) = (0x1b1914, 0xe9e1cc);
            assert!(within(lean(a, b, permille), crate::mix(a, b, permille as f32 / 1000.0), 1));
        }
    }

    #[test]
    fn the_studio_skins_are_their_palettes() {
        for (k, p) in
            [(&Skin::STUDIO_DARK, Palette::STUDIO_DARK), (&Skin::STUDIO_LIGHT, Palette::STUDIO_LIGHT)]
        {
            assert_eq!(
                (k.name, k.bg, k.text, k.dim, k.faint, k.rule),
                (p.name, p.ground, p.text, p.dim, p.faint, p.edge)
            );
            assert_eq!(k.lit, [p.green, p.amber, p.red]);
            // Drawn thin on the desk, a signal is its on-desk colour; the pastel is the meter's.
            assert_eq!((k.green, k.amber, k.red), (p.on.green, p.on.amber, p.on.red));
            assert_eq!(k.window, p.well);
            // The meter is the same on both desks: its window, its lit and unlit LEDs, its text.
            let (d, l) = (&Skin::STUDIO_DARK, &Skin::STUDIO_LIGHT);
            assert_eq!(
                (d.window, d.window_text, d.window_dim, d.lit, d.unlit),
                (l.window, l.window_text, l.window_dim, l.lit, l.unlit)
            );
            // And the desk's text reads on the desk.
            assert!(contrast(k.text, k.bg) >= 13.5 && contrast(k.dim, k.bg) >= 6.3, "{}", k.name);
        }
    }
}
