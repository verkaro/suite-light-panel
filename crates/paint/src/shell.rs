//! The shell: an LED meter that coils in on its own eye (Warren, 2026-11-16, for capstan's
//! client). A chain of round LEDs winds in along a logarithmic spiral to a red eye at its centre,
//! small and green on the outside, larger and amber as it closes in, the last one tangent to
//! the eye, which is far bigger than any of them. So the red is what the whole figure is
//! about, and it's the biggest thing on the face: green is the most of the meter and the least
//! of what's read across a room.
//!
//! The scale is coarse on purpose, about a dozen LEDs, each a threshold the tool names in dBFS
//! (`lit_at` turns a level into how many are lit), the leading one lit by how far the level is
//! into its step, so a few big LEDs still move smoothly. Flat, no glow. The ballistics, the
//! hold and the clip's latch are the tool's, as for the ladder and the disc: this draws the state
//! it's given.
//!
//! The eye is the last LED. Out, it's a ring only, so a wall of quiet tracks stays calm. Lit, it
//! fills red, flat like the rest; latched, it gets a bright rim. Its place is [`Placed::eye`], for a click
//! that clears the latch.
//!
//! [`Shell::new`] is geometry only, no pixels, and is made once per layout. [`led_shell`] draws.
//! In a skin's colours: [`crate::Skin::shell_leds`].

use crate::{LedColors, Pen, Rect, Rgb, Surface, mix, skin};

/// How a shell is wound. The defaults are what Warren settled on by eye (2026-11-16): twelve
/// LEDs growing 2.6 times, the eye 1.2 times the last of them (it was 2.4, then 1.6: cut again), a crook
/// round half a turn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShellShape {
    /// LEDs before the eye (the eye is one more). At least 2.
    pub dots: usize,
    /// The last LED's radius over the first's.
    pub growth: f32,
    /// The last LED's radius over the eye's: under 1, so the coil arcs over it.
    pub last_over_eye: f32,
    /// How fast the spiral opens: its radius multiplies by `e^b` a radian. The golden spiral's
    /// is 0.306. Smaller, the tail coils closer.
    pub b: f32,
    /// Between neighbouring LEDs, in eye radii.
    pub gap: f32,
    /// Where the tail starts round the eye, radians, clockwise from three o'clock.
    pub start: f32,
    /// Wound the other way.
    pub mirror: bool,
}

impl Default for ShellShape {
    fn default() -> ShellShape {
        ShellShape {
            dots: 12,
            growth: 2.6,
            last_over_eye: 0.85,
            b: 0.16,
            gap: 0.10,
            start: -0.4,
            mirror: false,
        }
    }
}

/// A circle: centre and radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dot {
    pub x: f32,
    pub y: f32,
    pub r: f32,
}

/// A shell's geometry: the LEDs on a canvas where the eye is at the origin with radius 1.
#[derive(Clone, Debug, PartialEq)]
pub struct Shell {
    dots: Vec<Dot>,
    /// Left, right, top, bottom, eye included.
    bounds: (f32, f32, f32, f32),
}

/// A shell's circles in a rectangle, in pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    /// The LEDs, from the tail in.
    pub dots: Vec<Dot>,
    pub eye: Dot,
}

impl Shell {
    /// The geometry for `shape`: each LED tangent to the next (less the gap), the last tangent
    /// to the eye, the spiral's length solved so they meet exactly.
    pub fn new(shape: &ShellShape) -> Shell {
        let n = shape.dots.max(2);
        let b = shape.b.max(0.02);
        let gap = shape.gap.max(0.0);
        let growth = shape.growth.max(1.0);
        let last = shape.last_over_eye.clamp(0.05, 1.0);
        let sign = if shape.mirror { -1.0f32 } else { 1.0 };
        let rad: Vec<f32> = (0..n).map(|i| last / growth * growth.powf(i as f32 / (n - 1) as f32)).collect();
        // The last LED's centre sits this far from the eye's.
        let rin = 1.0 + last + gap;
        // For a spiral of total angle `t`, ending at radius `rin`: the chain walked in from its
        // outer end, each LED found by bisection to be tangent to the one before. Returns the
        // chain and the angle its last LED fell at.
        let march = |t: f32| -> (Vec<Dot>, f32) {
            let at = |th: f32| {
                let r = rin * (b * (t - th)).exp();
                let a = shape.start + sign * th;
                (a.cos() * r, a.sin() * r)
            };
            let first = at(0.0);
            let mut dots = vec![Dot { x: first.0, y: first.1, r: rad[0] }];
            let mut th = 0.0f32;
            for i in 1..n {
                let (p, want) = (dots[i - 1], rad[i - 1] + rad[i] + gap);
                let (mut lo, mut hi) = (th, th + 1.5);
                for _ in 0..50 {
                    let m = (lo + hi) / 2.0;
                    let q = at(m);
                    if (q.0 - p.x).hypot(q.1 - p.y) < want { lo = m } else { hi = m }
                }
                th = hi;
                let q = at(th);
                dots.push(Dot { x: q.0, y: q.1, r: rad[i] });
            }
            (dots, th)
        };
        // Too short a spiral overshoots the eye (its last LED falls past `t`); too long, falls
        // short of it. Bisect to where it lands.
        let (mut lo, mut hi) = (0.05f32, 60.0f32);
        for _ in 0..60 {
            let m = (lo + hi) / 2.0;
            if march(m).1 > m { lo = m } else { hi = m }
        }
        let (dots, _) = march((lo + hi) / 2.0);
        let mut bounds = (-1.0f32, 1.0f32, -1.0f32, 1.0f32);
        for d in &dots {
            bounds = (
                bounds.0.min(d.x - d.r),
                bounds.1.max(d.x + d.r),
                bounds.2.min(d.y - d.r),
                bounds.3.max(d.y + d.r),
            );
        }
        Shell { dots, bounds }
    }

    /// LEDs before the eye.
    pub fn len(&self) -> usize {
        self.dots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.dots.is_empty()
    }

    /// Width over height of the whole figure, for a layout that gives it a cell.
    pub fn aspect(&self) -> f32 {
        (self.bounds.1 - self.bounds.0) / (self.bounds.3 - self.bounds.2)
    }

    /// The shell as large as fits in `r`, centred, as circles in pixels.
    pub fn place(&self, r: Rect) -> Placed {
        let (w, h) = (self.bounds.1 - self.bounds.0, self.bounds.3 - self.bounds.2);
        let k = (r.w as f32 / w).min(r.h as f32 / h).max(0.0);
        let ox = r.x as f32 + r.w as f32 / 2.0 - (self.bounds.0 + self.bounds.1) / 2.0 * k;
        let oy = r.y as f32 + r.h as f32 / 2.0 - (self.bounds.2 + self.bounds.3) / 2.0 * k;
        let to = |d: &Dot| Dot { x: ox + d.x * k, y: oy + d.y * k, r: d.r * k };
        Placed { dots: self.dots.iter().map(to).collect(), eye: to(&Dot { x: 0.0, y: 0.0, r: 1.0 }) }
    }
}

/// What a shell shows.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShellLevel {
    /// How many LEDs are lit, the eye the last of them: 3.4 is three and two fifths of the
    /// fourth, lit by that much. [`lit_at`] makes it from a level.
    pub lit: f32,
    /// The held peak's LED + 1 (0: none), lit too.
    pub peak: usize,
    /// A clip latched: the eye stays lit, with its rim, whatever the level.
    pub clip: bool,
}

/// How many LEDs a level of `db` lights, given the dBFS each lights at, from the tail, the eye's
/// last (`thresholds` rising). A whole number at a threshold, between two the fraction of the
/// step across; below the first, a step's ramp. This is how a dozen LEDs move smoothly.
pub fn lit_at(thresholds: &[f32], db: f32) -> f32 {
    let n = thresholds.len();
    if n == 0 || db.is_nan() {
        return 0.0;
    }
    let count = thresholds.iter().take_while(|&&t| t <= db).count();
    if count >= n {
        return n as f32;
    }
    let (from, to) = if count == 0 {
        let step = if n > 1 { thresholds[1] - thresholds[0] } else { 6.0 };
        (thresholds[0] - step, thresholds[0])
    } else {
        (thresholds[count - 1], thresholds[count])
    };
    let part = if to > from { ((db - from) / (to - from)).clamp(0.0, 1.0) } else { 0.0 };
    count as f32 + part
}

/// A shell in `r`: its LEDs as `c` lights them, and the eye. `c` needs one colour more than the
/// shell has LEDs (the eye's, last): [`LedColors::shell`]. Fewer, and nothing is drawn.
pub fn led_shell(s: &mut Surface, r: Rect, shell: &Shell, level: &ShellLevel, c: &LedColors) {
    let n = shell.len();
    if n == 0 || c.lit.len() <= n || c.unlit.len() <= n {
        return;
    }
    let p = shell.place(r);
    let lit = if level.lit.is_finite() { level.lit.clamp(0.0, (n + 1) as f32) } else { 0.0 };
    let held = |k: usize| level.peak > 0 && k == level.peak - 1;
    let amount = |k: usize| if held(k) { 1.0 } else { (lit - k as f32).clamp(0.0, 1.0) };
    for (k, d) in p.dots.iter().enumerate() {
        s.disc(d.x, d.y, d.r, mix(c.unlit[k], c.lit[k], amount(k)));
    }
    let a = if level.clip { 1.0 } else { amount(n) };
    let e = p.eye;
    let (lit_c, off_c) = (c.lit[n], c.unlit[n]);
    if a <= 0.0 {
        // Out: a ring, no more.
        s.ring(e.x, e.y, e.r, (e.r * 0.08).max(1.5), off_c);
        return;
    }
    s.disc(e.x, e.y, e.r, mix(off_c, lit_c, a));
    if level.clip {
        s.ring(e.x, e.y, e.r, (e.r * 0.1).max(2.0), mix(lit_c, 0xffffff, 0.7));
    }
}

/// What to write in the LEDs at the critical end: the dBFS of the last `count` LEDs before the
/// eye, as text, and nothing in the rest nor in the eye. One entry a threshold, for
/// [`Pen::shell_marked`]. `-9`, `-4.5`: whole numbers bare.
pub fn db_marks(thresholds: &[f32], count: usize) -> Vec<String> {
    let n = thresholds.len();
    (0..n)
        .map(|k| {
            if k + 1 < n && k + 1 + count >= n {
                let db = thresholds[k];
                if db.fract() == 0.0 { format!("{}", db as i32) } else { format!("{db:.1}") }
            } else {
                String::new()
            }
        })
        .collect()
}

impl Pen<'_, '_> {
    /// A shell in `r`, logical: [`led_shell`] through the pen's view.
    pub fn shell(&mut self, r: Rect, shell: &Shell, level: &ShellLevel, c: &LedColors) {
        let r = self.v.rect(r);
        led_shell(self.sf, r, shell, level, c);
    }

    /// A shell with text inside some of its LEDs (`marks`, one an LED, empty for none: see
    /// [`db_marks`]). Drawn bold in the largest Terminus that fits the bulb, dark on a lit LED and
    /// dim on an unlit one, so the number is there whether or not the level has reached it. A
    /// bulb too small for the smallest font is left blank, not crowded.
    pub fn shell_marked(
        &mut self,
        r: Rect,
        shell: &Shell,
        level: &ShellLevel,
        c: &LedColors,
        marks: &[String],
    ) {
        let r = self.v.rect(r);
        led_shell(self.sf, r, shell, level, c);
        let n = shell.len();
        if c.lit.len() <= n {
            return;
        }
        let lit = if level.lit.is_finite() { level.lit.clamp(0.0, (n + 1) as f32) } else { 0.0 };
        let placed = shell.place(r);
        for (k, (d, text)) in placed.dots.iter().zip(marks).enumerate() {
            if text.is_empty() {
                continue;
            }
            let held = level.peak > 0 && k == level.peak - 1;
            let on = held || lit - k as f32 >= 0.5;
            let colour = if on { skin().bg } else { skin().dim };
            // The largest font whose text sits inside the bulb's width (0.78 of it) and is
            // not taller than a bulb's radius and a bit.
            let mut px = d.r * 1.1;
            let found = loop {
                let (f, s) = self.fonts.pick(px, true);
                if (f.height * s) as f32 <= d.r * 1.1 + 0.5 && f.measure(text, s) as f32 <= d.r * 1.56 {
                    break Some((f, s));
                }
                if px < 12.0 {
                    break None;
                }
                px -= 1.0;
            };
            let Some((f, s)) = found else { continue };
            let (w, h) = (f.measure(text, s), (f.height * s) as i32);
            f.draw(
                self.sf,
                (d.x - w as f32 / 2.0).round() as i32,
                (d.y - h as f32 / 2.0).round() as i32,
                text,
                colour,
                s,
            );
        }
    }
}

impl LedColors {
    /// For a shell of `dots` LEDs and its eye: green to LED `amber_from`, amber from it, and
    /// the eye, the last, in the bands' red.
    pub fn shell(dots: usize, amber_from: usize, lit: [Rgb; 3], unlit: [Rgb; 3]) -> LedColors {
        let band = |k: usize| {
            if k >= dots {
                2
            } else if k >= amber_from {
                1
            } else {
                0
            }
        };
        LedColors {
            lit: (0..=dots).map(|k| lit[band(k)]).collect(),
            unlit: (0..=dots).map(|k| unlit[band(k)]).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Image, hex};

    const TH: [f32; 13] = [-48., -42., -36., -30., -24., -18., -12., -9., -6., -4., -2., -1., 0.];

    fn dist(a: &Dot, b: &Dot) -> f32 {
        (a.x - b.x).hypot(a.y - b.y)
    }

    /// Every default or odd shape the part is likely to be asked for.
    fn shapes() -> Vec<ShellShape> {
        let d = ShellShape::default();
        let mut v = vec![d];
        for dots in [2, 3, 6, 8, 16, 24] {
            v.push(ShellShape { dots, ..d });
        }
        for b in [0.08, 0.12, 0.306] {
            v.push(ShellShape { b, ..d });
        }
        for last_over_eye in [0.2, 0.7, 1.0] {
            v.push(ShellShape { last_over_eye, ..d });
        }
        v.push(ShellShape { growth: 1.0, ..d });
        v.push(ShellShape { mirror: true, ..d });
        v
    }

    #[test]
    fn leds_are_tangent_in_a_chain_and_the_last_to_the_eye() {
        for sh in shapes() {
            let s = Shell::new(&sh);
            let eye = Dot { x: 0.0, y: 0.0, r: 1.0 };
            for w in s.dots.windows(2) {
                let want = w[0].r + w[1].r + sh.gap;
                assert!(
                    (dist(&w[0], &w[1]) - want).abs() < 1e-3,
                    "{sh:?}: {} for {want}",
                    dist(&w[0], &w[1])
                );
            }
            let last = s.dots.last().unwrap();
            let want = eye.r + last.r + sh.gap;
            assert!(
                (dist(last, &eye) - want).abs() < 2e-3,
                "{sh:?}: the last LED is {} from the eye",
                dist(last, &eye)
            );
        }
    }

    #[test]
    fn nothing_overlaps_and_the_sizes_are_the_shape() {
        for sh in shapes() {
            let s = Shell::new(&sh);
            let n = s.len();
            assert_eq!(n, sh.dots.max(2));
            for i in 0..n {
                for j in i + 2..n {
                    assert!(
                        dist(&s.dots[i], &s.dots[j]) >= s.dots[i].r + s.dots[j].r - 1e-3,
                        "{sh:?}: LEDs {i} and {j} overlap"
                    );
                }
                // The eye, except the last LED that touches it.
                if i + 1 < n {
                    let eye = Dot { x: 0.0, y: 0.0, r: 1.0 };
                    assert!(
                        dist(&s.dots[i], &eye) >= 1.0 + s.dots[i].r - 1e-3,
                        "{sh:?}: LED {i} is in the eye"
                    );
                }
            }
            let (first, last) = (s.dots[0].r, s.dots[n - 1].r);
            assert!((last - sh.last_over_eye.clamp(0.05, 1.0)).abs() < 1e-5);
            assert!((last / first - sh.growth.max(1.0)).abs() < 1e-3);
            assert!(s.dots.windows(2).all(|w| w[1].r >= w[0].r - 1e-6), "{sh:?}: grows");
        }
    }

    #[test]
    fn mirrored_is_the_same_shell_wound_the_other_way() {
        let (a, b) = (ShellShape::default(), ShellShape { mirror: true, ..ShellShape::default() });
        let (sa, sb) = (Shell::new(&a), Shell::new(&b));
        assert_ne!(sa, sb);
        let eye = Dot { x: 0.0, y: 0.0, r: 1.0 };
        for (p, q) in sa.dots.iter().zip(&sb.dots) {
            assert!((p.r - q.r).abs() < 1e-6);
            assert!((dist(p, &eye) - dist(q, &eye)).abs() < 2e-3);
        }
    }

    #[test]
    fn it_fits_its_rectangle_and_is_centred() {
        let s = Shell::new(&ShellShape::default());
        for r in [Rect::new(10, 20, 200, 200), Rect::new(0, 0, 300, 120), Rect::new(5, 5, 90, 400)] {
            let p = s.place(r);
            let all = p.dots.iter().chain([&p.eye]);
            let (mut x0, mut x1, mut y0, mut y1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
            for d in all {
                x0 = x0.min(d.x - d.r);
                x1 = x1.max(d.x + d.r);
                y0 = y0.min(d.y - d.r);
                y1 = y1.max(d.y + d.r);
            }
            assert!(x0 >= r.x as f32 - 0.01 && x1 <= r.right() as f32 + 0.01, "{r:?} wide");
            assert!(y0 >= r.y as f32 - 0.01 && y1 <= r.bottom() as f32 + 0.01, "{r:?} tall");
            // The limiting side is filled, the other centred.
            let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
            assert!((cx - (r.x as f32 + r.w as f32 / 2.0)).abs() < 0.01);
            assert!((cy - (r.y as f32 + r.h as f32 / 2.0)).abs() < 0.01);
            assert!((x1 - x0 - r.w as f32).abs() < 0.01 || (y1 - y0 - r.h as f32).abs() < 0.01);
        }
        assert!(s.aspect() > 0.5 && s.aspect() < 2.0, "roughly square: {}", s.aspect());
    }

    #[test]
    fn a_level_lights_leds_by_the_steps_between_thresholds() {
        // At a threshold that LED is just lit; between two, the next is lit by how far across.
        assert_eq!(lit_at(&TH, -48.0), 1.0);
        assert_eq!(lit_at(&TH, -42.0), 2.0);
        assert_eq!(lit_at(&TH, -45.0), 1.5);
        assert_eq!(lit_at(&TH, 0.0), 13.0);
        assert_eq!(lit_at(&TH, 12.0), 13.0);
        // Below the first, a step's ramp: a glimmer, then nothing.
        assert_eq!(lit_at(&TH, -54.0), 0.0);
        assert_eq!(lit_at(&TH, -51.0), 0.5);
        assert_eq!(lit_at(&TH, -200.0), 0.0);
        assert_eq!(lit_at(&TH, f32::NEG_INFINITY), 0.0);
        assert_eq!(lit_at(&TH, f32::NAN), 0.0);
        assert_eq!(lit_at(&[], -6.0), 0.0);
        // It never goes back as the level rises.
        let mut last = 0.0;
        for k in 0..600 {
            let v = lit_at(&TH, -60.0 + k as f32 * 0.1);
            assert!(v >= last);
            last = v;
        }
    }

    // Twelve LEDs and the eye.
    fn draw(level: ShellLevel) -> (Image, Placed) {
        let s = Shell::new(&ShellShape::default());
        let c = LedColors::shell(
            12,
            8,
            [hex("#56d364"), hex("#ffb020"), hex("#ff4b3e")],
            [hex("#1d2a1f"), hex("#352a15"), hex("#361b17")],
        );
        let mut img = Image::new(300, 300);
        let mut sf = img.surface();
        sf.clear(0x101010);
        let r = Rect::new(0, 0, 300, 300);
        led_shell(&mut sf, r, &s, &level, &c);
        (img, s.place(r))
    }

    fn at(img: &Image, d: &Dot) -> u32 {
        img.px[(d.y.round() as usize) * img.w + d.x.round() as usize]
    }

    #[test]
    fn the_eye_is_a_ring_when_out_and_red_when_lit() {
        let (img, p) = draw(ShellLevel::default());
        assert_eq!(at(&img, &p.eye), 0x101010, "out: the middle is the background");
        // The ring is drawn at the rim.
        let rim = Dot { x: p.eye.x + p.eye.r - 1.0, ..p.eye };
        assert_ne!(at(&img, &rim), 0x101010);
        let (img, p) = draw(ShellLevel { lit: 13.0, ..ShellLevel::default() });
        let mid = Dot { x: p.eye.x + p.eye.r * 0.5, ..p.eye };
        assert_eq!(at(&img, &mid), hex("#ff4b3e"));
    }

    #[test]
    fn a_latched_clip_holds_the_eye_lit_whatever_the_level() {
        let (img, p) = draw(ShellLevel { lit: 2.0, clip: true, ..ShellLevel::default() });
        let mid = Dot { x: p.eye.x + p.eye.r * 0.5, ..p.eye };
        assert_eq!(at(&img, &mid), hex("#ff4b3e"));
        // And the bright rim, which a lit eye alone hasn't.
        let rim = Dot { x: p.eye.x + p.eye.r - 1.0, ..p.eye };
        let (plain, p2) = draw(ShellLevel { lit: 13.0, ..ShellLevel::default() });
        let rim2 = Dot { x: p2.eye.x + p2.eye.r - 1.0, ..p2.eye };
        assert_ne!(at(&img, &rim), at(&plain, &rim2));
    }

    #[test]
    fn the_leading_led_is_lit_by_its_fraction_and_the_peak_stands_apart() {
        let green = hex("#56d364");
        let off = hex("#1d2a1f");
        let (img, p) = draw(ShellLevel { lit: 3.5, ..ShellLevel::default() });
        assert_eq!(at(&img, &p.dots[2]), green);
        let half = at(&img, &p.dots[3]);
        assert!(half != green && half != off, "the fourth, half lit: {half:06x}");
        assert_eq!(at(&img, &p.dots[4]), off);
        // The held peak LED is lit, with nothing between it and the level.
        let (img, p) = draw(ShellLevel { lit: 3.0, peak: 7, ..ShellLevel::default() });
        assert_eq!(at(&img, &p.dots[6]), green);
        assert_eq!(at(&img, &p.dots[5]), off);
    }

    #[test]
    fn it_does_not_draw_with_too_few_colours_and_does_not_panic_on_odd_levels() {
        let s = Shell::new(&ShellShape::default());
        let mut img = Image::new(100, 100);
        let mut sf = img.surface();
        sf.clear(0x101010);
        led_shell(
            &mut sf,
            Rect::new(0, 0, 100, 100),
            &s,
            &ShellLevel { lit: 5.0, ..ShellLevel::default() },
            &LedColors::bands(12, [1, 2, 3], [4, 5, 6]),
        );
        assert!(img.px.iter().all(|&p| p == 0x101010));
        let c = LedColors::shell(12, 8, [1, 2, 3], [4, 5, 6]);
        let mut sf = img.surface();
        for lit in [f32::NAN, f32::INFINITY, -3.0, 1e9] {
            led_shell(&mut sf, Rect::new(0, 0, 100, 100), &s, &ShellLevel { lit, peak: 99, clip: false }, &c);
        }
        led_shell(&mut sf, Rect::new(0, 0, 0, 0), &s, &ShellLevel::default(), &c);
        led_shell(&mut sf, Rect::new(-50, -50, 100, 100), &s, &ShellLevel::default(), &c);
    }

    #[test]
    fn the_shell_bands_are_green_then_amber_then_the_eye_red() {
        let c = LedColors::shell(12, 8, [1, 2, 3], [4, 5, 6]);
        assert_eq!(c.lit.len(), 13);
        assert_eq!(c.lit[7], 1);
        assert_eq!(c.lit[8], 2);
        assert_eq!(c.lit[11], 2);
        assert_eq!(c.lit[12], 3);
        assert_eq!(c.unlit[12], 6);
    }

    #[test]
    fn the_critical_leds_are_marked_with_their_db_and_the_eye_is_not() {
        let m = db_marks(&TH, 5);
        assert_eq!(m.len(), 13);
        let said: Vec<(usize, &str)> =
            m.iter().enumerate().filter(|(_, s)| !s.is_empty()).map(|(k, s)| (k, s.as_str())).collect();
        assert_eq!(said, [(7, "-9"), (8, "-6"), (9, "-4"), (10, "-2"), (11, "-1")]);
        assert!(db_marks(&TH, 0).iter().all(|s| s.is_empty()));
        assert_eq!(db_marks(&TH, 99).iter().filter(|s| !s.is_empty()).count(), 12);
        assert_eq!(db_marks(&TH, 99)[12], "");
        assert_eq!(db_marks(&[-6.0, -4.5, 0.0], 1)[1], "-4.5");
        assert!(db_marks(&[], 5).is_empty());
    }

    // The shell drawn through a pen, with marks or without, on a 300 × 300 image.
    fn through_pen(marks: Option<&[String]>, r: Rect) -> Option<Image> {
        let fonts = crate::Fonts::terminus(None).ok()?;
        let s = Shell::new(&ShellShape::default());
        let c = LedColors::shell(
            12,
            8,
            [hex("#56d364"), hex("#ffb020"), hex("#ff4b3e")],
            [hex("#1d2a1f"), hex("#352a15"), hex("#361b17")],
        );
        let mut img = Image::new(300, 300);
        let mut sf = img.surface();
        sf.clear(0x101010);
        let mut pen = Pen { sf: &mut sf, fonts: &fonts, v: crate::View::fit(300, 300, 300, 300) };
        let level = ShellLevel { lit: 11.0, ..ShellLevel::default() };
        match marks {
            Some(m) => pen.shell_marked(r, &s, &level, &c, m),
            None => pen.shell(r, &s, &level, &c),
        }
        Some(img)
    }

    #[test]
    fn marks_are_written_inside_their_bulbs_and_nowhere_else() {
        let r = Rect::new(0, 0, 300, 300);
        let (Some(plain), Some(marked)) = (through_pen(None, r), through_pen(Some(&db_marks(&TH, 5)), r))
        else {
            return; // no Terminus here
        };
        let p = Shell::new(&ShellShape::default()).place(r);
        let inside = |x: usize, y: usize, k: usize| {
            let d = p.dots[k];
            (x as f32 + 0.5 - d.x).hypot(y as f32 + 0.5 - d.y) <= d.r
        };
        let mut changed = [0usize; 12];
        for y in 0..300 {
            for x in 0..300 {
                if plain.px[y * 300 + x] != marked.px[y * 300 + x] {
                    let owner = (0..12).find(|&k| inside(x, y, k));
                    let k = owner.unwrap_or_else(|| panic!("text outside a bulb at ({x}, {y})"));
                    assert!(k >= 7, "LED {k} has no mark");
                    changed[k] += 1;
                }
            }
        }
        for (k, n) in changed.iter().enumerate() {
            assert_eq!(*n > 0, k >= 7, "LED {k}: {n} pixels of text");
        }
    }

    #[test]
    fn a_bulb_too_small_for_the_smallest_font_is_left_blank() {
        let r = Rect::new(0, 0, 40, 40);
        let (Some(plain), Some(marked)) = (through_pen(None, r), through_pen(Some(&db_marks(&TH, 5)), r))
        else {
            return;
        };
        assert_eq!(plain.px, marked.px);
    }
}
