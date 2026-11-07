//! The family's two meters, drawn from LED counts a tool has already given its ballistics
//! (`as-meter`, Dexter's `meter`): how many LEDs are lit, and the held peak's LED.
//!
//! - The **ladder**: a column of LEDs, bottom up, green, then amber, then red.
//! - The **disc**: one LED a track, for when there's no room for a ladder (Dexter's remote, on a
//!   phone): white, brighter as it's louder, then yellow, then red, held with the peak so a
//!   caution doesn't flicker; ringed red when clipped, armed or recording.

use crate::{Rect, Rgb, Surface, hex, mix};

/// A ladder's colours, lit and unlit, one per LED from the bottom.
#[derive(Clone, Debug)]
pub struct LedColors {
    pub lit: Vec<Rgb>,
    pub unlit: Vec<Rgb>,
}

impl LedColors {
    /// `n` LEDs: green to the eleventh of sixteen, amber to the fifteenth, then red, scaled to
    /// `n` (Dexter's −9 / −6 / −1 dBFS split).
    pub fn bands(n: usize, lit: [Rgb; 3], unlit: [Rgb; 3]) -> LedColors {
        let band = |k: usize| {
            let at = k * 16 / n.max(1);
            if at < 11 {
                0
            } else if at < 15 {
                1
            } else {
                2
            }
        };
        LedColors {
            lit: (0..n).map(|k| lit[band(k)]).collect(),
            unlit: (0..n).map(|k| unlit[band(k)]).collect(),
        }
    }

    /// Dexter's dark skin, 16 LEDs.
    pub fn dexter_dark() -> LedColors {
        LedColors::bands(
            16,
            [hex("#56d364"), hex("#ffb020"), hex("#ff4b3e")],
            [hex("#1d2a1f"), hex("#352a15"), hex("#361b17")],
        )
    }
}

/// A ladder in `r` (device pixels): `lit` LEDs from the bottom, and the LED `peak` − 1 too
/// (0: no peak held). `gap` between LEDs; `glow` how far a lit one's light spills, 0 for none.
pub fn ladder(s: &mut Surface, r: Rect, lit: usize, peak: usize, c: &LedColors, gap: i32, glow: i32) {
    let n = c.lit.len();
    if n == 0 {
        return;
    }
    let on = |k: usize| k < lit || (peak > 0 && k == peak - 1);
    let seg = (r.h - gap * (n as i32 - 1)) as f32 / n as f32;
    let top = |k: usize| r.bottom() - ((k + 1) as f32 * seg + k as f32 * gap as f32).round() as i32;
    let bottom = |k: usize| r.bottom() - (k as f32 * (seg + gap as f32)).round() as i32;
    let led = |k: usize| Rect::new(r.x, top(k), r.w, bottom(k) - top(k));
    for k in 0..n {
        if !on(k) {
            s.fill(led(k), c.unlit[k]);
        }
    }
    if glow > 0 {
        for k in (0..n).filter(|&k| on(k)) {
            s.glow_rect(led(k), glow, c.lit[k], 0.35);
        }
    }
    for k in (0..n).filter(|&k| on(k)) {
        let l = led(k);
        s.fill(l, c.lit[k]);
        // A hot core: the middle third nearer white, as a lit LED looks.
        let core = Rect::new(l.x + l.w / 3, l.y + l.h / 4, (l.w / 3).max(1), (l.h / 2).max(1));
        s.fill(core, mix(c.lit[k], 0xffffff, 0.35));
    }
}

/// What a disc LED shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Disc {
    Off,
    /// White at this brightness, 0.15 to 1.
    Signal(f32),
    Caution,
    Hot,
    Clip,
}

impl Disc {
    fn band(lit: usize, thresholds: &[f32]) -> Disc {
        if lit == 0 {
            return Disc::Off;
        }
        let db = thresholds.get(lit - 1).copied().unwrap_or(-48.0);
        if db >= -1.0 {
            Disc::Hot
        } else if db >= -6.0 {
            Disc::Caution
        } else {
            // From a glimmer at −48 to full at −6, in twentieths.
            let a = (0.2 + 0.8 * (db + 48.0) / 42.0).clamp(0.15, 1.0);
            Disc::Signal((a * 20.0).round() / 20.0)
        }
    }

    /// A track's disc from its meter: `lit` LEDs lit, `held` the held peak's LED + 1 (0: none),
    /// `thresholds` the dB each LED lights at, from the bottom. The clip latch first; then
    /// caution or hot while the held peak is, so they don't flicker; else white by the level.
    pub fn from_meter(lit: usize, held: usize, clip: bool, thresholds: &[f32]) -> Disc {
        if clip {
            return Disc::Clip;
        }
        let now = Disc::band(lit, thresholds);
        match Disc::band(held, thresholds) {
            Disc::Hot => Disc::Hot,
            Disc::Caution if now != Disc::Hot => Disc::Caution,
            _ => now,
        }
    }
}

const LED_OFF: Rgb = hex("#16140f");
const LED_WHITE: Rgb = hex("#f4f1ea");
const LED_CAUTION: Rgb = hex("#ffd23f");
const LED_HOT: Rgb = hex("#ff4b3e");
const WELL: Rgb = 0x000000;

/// A disc LED of radius `r` centred at (cx, cy), device pixels, with a dark well round it and a
/// ring outside that when `ring` is given (armed, recording, clipped).
pub fn disc_led(s: &mut Surface, cx: f32, cy: f32, r: f32, d: Disc, ring: Option<Rgb>) {
    let t = (r / 6.0).max(1.5);
    let ring = ring.or(if d == Disc::Clip { Some(LED_HOT) } else { None });
    if let Some(rc) = ring {
        s.ring(cx, cy, r + 2.0 * t, t, rc);
    }
    s.disc(cx, cy, r + t, WELL);
    let (c, glow) = match d {
        Disc::Off => (LED_OFF, 0.0),
        Disc::Signal(a) => (mix(LED_OFF, LED_WHITE, a), 0.15 + 0.6 * a),
        Disc::Caution => (LED_CAUTION, 0.8),
        Disc::Hot => (LED_HOT, 0.9),
        Disc::Clip => (LED_HOT, 1.0),
    };
    if glow > 0.0 {
        s.glow(cx, cy, r, r * 0.9 * glow, c, 0.45);
    }
    s.disc(cx, cy, r, c);
    if d != Disc::Off {
        // The hot middle, as light through a lens.
        s.disc(cx - r * 0.2, cy - r * 0.2, r * 0.45, mix(c, 0xffffff, 0.35));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TH: [f32; 16] =
        [-48., -42., -36., -30., -27., -24., -21., -18., -15., -12., -9., -6., -4.5, -3., -2., -1.];

    #[test]
    fn disc_follows_the_remote() {
        assert_eq!(Disc::from_meter(0, 0, false, &TH), Disc::Off);
        assert_eq!(Disc::from_meter(1, 0, false, &TH), Disc::Signal(0.2));
        assert_eq!(Disc::from_meter(11, 0, false, &TH), Disc::Signal(0.95));
        assert_eq!(Disc::from_meter(12, 0, false, &TH), Disc::Caution);
        assert_eq!(Disc::from_meter(16, 0, false, &TH), Disc::Hot);
        // The held peak keeps a caution on while the level falls away.
        assert_eq!(Disc::from_meter(3, 13, false, &TH), Disc::Caution);
        assert_eq!(Disc::from_meter(3, 16, false, &TH), Disc::Hot);
        assert_eq!(Disc::from_meter(3, 16, true, &TH), Disc::Clip);
    }

    #[test]
    fn ladder_bands() {
        let c = LedColors::dexter_dark();
        assert_eq!(c.lit[10], hex("#56d364"));
        assert_eq!(c.lit[11], hex("#ffb020"));
        assert_eq!(c.lit[15], hex("#ff4b3e"));
    }
}
