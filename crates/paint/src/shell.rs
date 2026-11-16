//! The shell: an LED meter that follows the golden spiral (Warren, 2026-11-16, for capstan's
//! client). Round LEDs on a logarithmic spiral, each one's size in proportion to where it sits on
//! it, so each is a fixed step bigger than the one before and each touches the next: the
//! Fibonacci circles, a nautilus. Small and green at the coil, larger and amber as it opens
//! out, and the last, the eye, the biggest and red. Nothing is placed by hand: the spiral
//! and the one ratio make every LED.
//!
//! Read across a room, green is the most of the meter and the least of what matters. Growing
//! the LEDs puts the weight where the danger is, and the last few are big enough to carry their
//! dBFS inside them.
//!
//! The scale is coarse on purpose, about a dozen LEDs, each a threshold the tool names in dBFS
//! (`lit_at` turns a level into how many are lit), the leading one lit by how far the level is
//! into its step, so a few big LEDs still move smoothly. Flat, no glow. The ballistics, the
//! hold and the clip's latch are the tool's, as for the ladder and the disc: this draws the state
//! it's given.
//!
//! The eye is the last LED, drawn as the others are; a latched clip gives it a bright rim. Its place
//! is [`Placed::eye`], for a click that clears the latch.
//!
//! [`Shell::new`] is geometry only, no pixels, and is made once per layout. [`led_shell`] draws.
//! In a skin's colours, on a track's surface: [`crate::Skin::shell_leds`], [`crate::Tier`].

use crate::{LedColors, Pen, Rect, Rgb, Surface, mix, skin};

/// The golden spiral's pitch: its radius multiplies by the golden ratio every quarter turn.
pub const GOLDEN_B: f32 = 0.306_349;

/// How a shell is wound.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShellShape {
    /// LEDs before the eye (the eye is one more). At least 2.
    pub dots: usize,
    /// The eye's radius over the first LED's: how far the LEDs grow in all.
    pub growth: f32,
    /// The spiral's pitch: its radius multiplies by `e^b` a radian. The golden spiral's is
    /// [`GOLDEN_B`], the default. It must not turn far in a step, `ln(growth) / dots / b`, which
    /// wants to stay under about 0.8 of a radian: a few LEDs growing a lot, or a pitch under
    /// about 0.2, come round on themselves and overlap.
    pub b: f32,
    /// Between neighbouring LEDs, as a fraction of the two radii.
    pub gap: f32,
    /// Where the coil's first LED sits round its centre, radians, clockwise from three o'clock.
    pub start: f32,
    /// Wound the other way.
    pub mirror: bool,
}

impl Default for ShellShape {
    fn default() -> ShellShape {
        ShellShape { dots: 12, growth: 6.0, b: GOLDEN_B, gap: 0.12, start: 2.4, mirror: false }
    }
}

/// A circle: centre and radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dot {
    pub x: f32,
    pub y: f32,
    pub r: f32,
}

/// A shell's geometry: the LEDs and the eye, as circles on a canvas whose scale is the first
/// LED's distance from the spiral's centre.
#[derive(Clone, Debug, PartialEq)]
pub struct Shell {
    /// The LEDs, then the eye.
    circles: Vec<Dot>,
    /// Left, right, top, bottom.
    bounds: (f32, f32, f32, f32),
}

/// A shell's circles in a rectangle, in pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    /// The LEDs, from the coil out.
    pub dots: Vec<Dot>,
    pub eye: Dot,
}

impl Shell {
    /// The geometry for `shape`. On the spiral `r = e^(b·θ)` the LED `i` sits at the angle that
    /// takes the radius up by one step, `g = growth^(1/dots)`, and has a radius `c` times the
    /// spiral's there; `c` is whatever makes two neighbours touch (less the gap), which has a
    /// closed form, so every LED's place is exact.
    pub fn new(shape: &ShellShape) -> Shell {
        let n = shape.dots.max(2);
        let b = shape.b.max(0.02);
        let growth = shape.growth.max(1.05);
        let sign = if shape.mirror { -1.0f32 } else { 1.0 };
        let g = growth.powf(1.0 / n as f32);
        let step = g.ln() / b;
        let chord = (1.0 + g * g - 2.0 * g * step.cos()).sqrt();
        let c = chord / ((1.0 + g) * (1.0 + shape.gap.max(0.0)));
        let circles: Vec<Dot> = (0..=n)
            .map(|i| {
                let radius = g.powi(i as i32);
                let a = shape.start + sign * step * i as f32;
                Dot { x: a.cos() * radius, y: a.sin() * radius, r: c * radius }
            })
            .collect();
        let mut bounds = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for d in &circles {
            bounds = (
                bounds.0.min(d.x - d.r),
                bounds.1.max(d.x + d.r),
                bounds.2.min(d.y - d.r),
                bounds.3.max(d.y + d.r),
            );
        }
        Shell { circles, bounds }
    }

    /// LEDs before the eye.
    pub fn len(&self) -> usize {
        self.circles.len() - 1
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
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
        let n = self.len();
        Placed { dots: self.circles[..n].iter().map(to).collect(), eye: to(&self.circles[n]) }
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
    s.disc(e.x, e.y, e.r, mix(c.unlit[n], c.lit[n], a));
    if level.clip {
        s.ring(e.x, e.y, e.r, (e.r * 0.1).max(2.0), mix(c.lit[n], 0xffffff, 0.7));
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
    /// [`db_marks`]). All one size: the largest bold Terminus that fits the smallest bulb marked,
    /// so the numbers read as a set. Dark on a lit LED and dim on an unlit one, so the number is
    /// there whether or not the level has reached it. If even the smallest font doesn't fit that
    /// bulb, none is drawn, not some.
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
        let marked: Vec<(usize, &Dot, &String)> = placed
            .dots
            .iter()
            .zip(marks)
            .enumerate()
            .filter(|(_, (_, t))| !t.is_empty())
            .map(|(k, (d, t))| (k, d, t))
            .collect();
        let Some(smallest) = marked.iter().map(|(_, d, _)| d.r).reduce(f32::min) else { return };
        let longest = marked.iter().map(|(_, _, t)| t.chars().count()).max().unwrap_or(0);
        // Text no taller than a bulb's radius and a bit, and no wider than 0.78 of its width.
        let mut px = smallest * 1.1;
        let found = loop {
            let (f, s) = self.fonts.pick(px, true);
            if (f.height * s) as f32 <= smallest * 1.1 + 0.5
                && (longest * f.width * s) as f32 <= smallest * 1.56
            {
                break Some((f, s));
            }
            if px < 12.0 {
                break None;
            }
            px -= 1.0;
        };
        let Some((f, s)) = found else { return };
        for (k, d, text) in marked {
            let held = level.peak > 0 && k == level.peak - 1;
            let colour = if held || lit - k as f32 >= 0.5 { skin().window } else { skin().window_dim };
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

/// How far an unlit LED is from its ground towards its lit colour: far enough to show the scale's
/// shape, near enough to stay quiet. Held by what it's unlit *on*, not by a colour of its own, so
/// it reads as well on a recording tier as on the page.
pub const UNLIT_MIX: f32 = 0.17;

impl LedColors {
    /// For a shell of `dots` LEDs and its eye: green to LED `amber_from`, amber from it, and the
    /// eye, the last, in the bands' red. Unlit, each is [`UNLIT_MIX`] of the way from `ground`,
    /// the surface it's on ([`crate::Skin::tier`]), to its lit colour.
    pub fn shell(dots: usize, amber_from: usize, lit: [Rgb; 3], ground: Rgb) -> LedColors {
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
            unlit: (0..=dots).map(|k| mix(ground, lit[band(k)], UNLIT_MIX)).collect(),
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

    /// The shapes the part is likely to be asked for: the golden pitch and a little either
    /// side of it, for any number of LEDs and how far they grow, so long as no step turns more
    /// than 0.8 of a radian (`ShellShape::b`).
    fn shapes() -> Vec<ShellShape> {
        let d = ShellShape::default();
        let mut v = vec![d];
        for dots in [2, 3, 6, 8, 12, 16, 24] {
            for growth in [2.0f32, 4.5, 8.0] {
                for b in [0.25, GOLDEN_B, 0.4] {
                    if growth.ln() / dots as f32 / b <= 0.8 {
                        v.push(ShellShape { dots, growth, b, ..d });
                    }
                }
            }
        }
        v.push(ShellShape { mirror: true, ..d });
        v.push(ShellShape { gap: 0.0, ..d });
        v.push(ShellShape { start: -1.0, ..d });
        v
    }

    #[test]
    fn each_led_touches_the_next_less_the_gap() {
        for sh in shapes() {
            let s = Shell::new(&sh);
            for w in s.circles.windows(2) {
                let want = (w[0].r + w[1].r) * (1.0 + sh.gap);
                assert!(
                    (dist(&w[0], &w[1]) - want).abs() < 1e-4 * want.max(1.0),
                    "{sh:?}: {} for {want}",
                    dist(&w[0], &w[1])
                );
            }
        }
    }

    #[test]
    fn the_sizes_are_one_ratio_a_step_and_the_eye_is_the_last() {
        for sh in shapes() {
            let s = Shell::new(&sh);
            let n = sh.dots.max(2);
            assert_eq!((s.len(), s.circles.len()), (n, n + 1));
            let (first, eye) = (s.circles[0].r, s.circles[n].r);
            assert!(
                (eye / first - sh.growth.max(1.05)).abs() < 1e-3 * sh.growth,
                "{sh:?}: grows {}",
                eye / first
            );
            let step = s.circles[1].r / s.circles[0].r;
            for w in s.circles.windows(2) {
                assert!((w[1].r / w[0].r - step).abs() < 1e-4, "{sh:?}: a constant ratio");
            }
            // And each is the same distance from the spiral's centre as its size says: it's on
            // the curve, not placed by hand.
            let c = s.circles[0].r / s.circles[0].x.hypot(s.circles[0].y);
            for d in &s.circles {
                assert!((d.r / d.x.hypot(d.y) - c).abs() < 1e-4, "{sh:?}: size follows radius");
            }
        }
    }

    #[test]
    fn nothing_overlaps() {
        for sh in shapes() {
            let s = Shell::new(&sh);
            for i in 0..s.circles.len() {
                for j in i + 2..s.circles.len() {
                    let (a, b) = (&s.circles[i], &s.circles[j]);
                    assert!(dist(a, b) >= a.r + b.r - 1e-3, "{sh:?}: LEDs {i} and {j} overlap");
                }
            }
        }
    }

    #[test]
    fn the_defaults_are_the_golden_spiral() {
        // A quarter turn of it is the golden ratio more radius.
        let sh = ShellShape::default();
        assert_eq!(sh.b, GOLDEN_B);
        let phi = (1.0 + 5f32.sqrt()) / 2.0;
        assert!((((std::f32::consts::FRAC_PI_2) * GOLDEN_B).exp() - phi).abs() < 1e-4);
        assert_eq!(Shell::new(&sh).len(), 12);
    }

    #[test]
    fn mirrored_is_the_same_shell_wound_the_other_way() {
        let (a, b) = (ShellShape::default(), ShellShape { mirror: true, ..ShellShape::default() });
        let (sa, sb) = (Shell::new(&a), Shell::new(&b));
        assert_ne!(sa, sb);
        for (p, q) in sa.circles.iter().zip(&sb.circles) {
            assert!((p.r - q.r).abs() < 1e-6);
            assert!((p.x.hypot(p.y) - q.x.hypot(q.y)).abs() < 1e-4);
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

    const LIT: [Rgb; 3] = [hex("#56d364"), hex("#ffb020"), hex("#ff4b3e")];
    const GROUND: Rgb = 0x101010;
    /// What a band's LED is when unlit, on `GROUND`.
    fn unlit(band: usize) -> Rgb {
        mix(GROUND, LIT[band], UNLIT_MIX)
    }

    // Twelve LEDs and the eye.
    fn draw(level: ShellLevel) -> (Image, Placed) {
        let s = Shell::new(&ShellShape::default());
        let c = LedColors::shell(12, 8, LIT, GROUND);
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
    fn the_eye_is_an_led_like_the_rest_dim_when_out_and_red_when_lit() {
        let (img, p) = draw(ShellLevel::default());
        assert_eq!(at(&img, &p.eye), unlit(2));
        let (img, p) = draw(ShellLevel { lit: 13.0, ..ShellLevel::default() });
        assert_eq!(at(&img, &p.eye), LIT[2]);
        // Flat: every pixel inside it, short of the rim a clip gets, is the one colour.
        for (dx, dy) in [(0.0, 0.0), (0.5, 0.0), (-0.5, 0.0), (0.0, 0.5), (0.0, -0.6), (-0.3, -0.3)] {
            let d = Dot { x: p.eye.x + dx * p.eye.r, y: p.eye.y + dy * p.eye.r, ..p.eye };
            assert_eq!(at(&img, &d), LIT[2], "({dx}, {dy})");
        }
    }

    #[test]
    fn a_latched_clip_holds_the_eye_lit_whatever_the_level_and_rims_it() {
        let (img, p) = draw(ShellLevel { lit: 2.0, clip: true, ..ShellLevel::default() });
        assert_eq!(at(&img, &p.eye), LIT[2]);
        let rim = Dot { x: p.eye.x + p.eye.r - 1.0, ..p.eye };
        let (plain, p2) = draw(ShellLevel { lit: 13.0, ..ShellLevel::default() });
        let rim2 = Dot { x: p2.eye.x + p2.eye.r - 1.0, ..p2.eye };
        assert_ne!(at(&img, &rim), at(&plain, &rim2));
    }

    #[test]
    fn the_leading_led_is_lit_by_its_fraction_and_the_peak_stands_apart() {
        let (green, off) = (LIT[0], unlit(0));
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
        let few = LedColors::bands(12, [1, 2, 3], [4, 5, 6]);
        led_shell(
            &mut sf,
            Rect::new(0, 0, 100, 100),
            &s,
            &ShellLevel { lit: 5.0, ..ShellLevel::default() },
            &few,
        );
        assert!(img.px.iter().all(|&p| p == 0x101010));
        let c = LedColors::shell(12, 8, [0x10, 0x20, 0x30], 0);
        let mut sf = img.surface();
        for lit in [f32::NAN, f32::INFINITY, -3.0, 1e9] {
            led_shell(&mut sf, Rect::new(0, 0, 100, 100), &s, &ShellLevel { lit, peak: 99, clip: false }, &c);
        }
        led_shell(&mut sf, Rect::new(0, 0, 0, 0), &s, &ShellLevel::default(), &c);
        led_shell(&mut sf, Rect::new(-50, -50, 100, 100), &s, &ShellLevel::default(), &c);
    }

    #[test]
    fn the_shell_bands_are_green_then_amber_then_the_eye_red() {
        let c = LedColors::shell(12, 8, [0x10, 0x20, 0x30], 0);
        assert_eq!(c.lit.len(), 13);
        assert_eq!(c.lit[7], 0x10);
        assert_eq!(c.lit[8], 0x20);
        assert_eq!(c.lit[11], 0x20);
        assert_eq!(c.lit[12], 0x30);
        assert_eq!(c.unlit[12], mix(0, 0x30, UNLIT_MIX));
        // Unlit is on its ground: a lighter ground gives lighter unlit LEDs.
        let lifted = LedColors::shell(12, 8, [0x10, 0x20, 0x30], 0x404040);
        assert!(lifted.unlit.iter().zip(&c.unlit).all(|(a, b)| a > b));
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
        let c = LedColors::shell(12, 8, LIT, GROUND);
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
    fn marks_are_inside_their_bulbs_all_one_size() {
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
        // Per bulb, the rows its text covers.
        let mut rows: [(usize, usize); 12] = [(usize::MAX, 0); 12];
        for y in 0..300 {
            for x in 0..300 {
                if plain.px[y * 300 + x] != marked.px[y * 300 + x] {
                    let k = (0..12)
                        .find(|&k| inside(x, y, k))
                        .unwrap_or_else(|| panic!("text outside a bulb at ({x}, {y})"));
                    assert!(k >= 7, "LED {k} has no mark");
                    rows[k] = (rows[k].0.min(y), rows[k].1.max(y));
                }
            }
        }
        let heights: Vec<usize> = (7..12).map(|k| rows[k].1 + 1 - rows[k].0).collect();
        assert!(rows[..7].iter().all(|r| r.0 == usize::MAX), "an unmarked LED has text");
        assert!(heights.iter().all(|&h| h > 1 && h == heights[0]), "one size, got heights {heights:?}");
    }

    #[test]
    fn a_bulb_too_small_for_the_smallest_font_leaves_all_the_marks_out() {
        let r = Rect::new(0, 0, 40, 40);
        let (Some(plain), Some(marked)) = (through_pen(None, r), through_pen(Some(&db_marks(&TH, 5)), r))
        else {
            return;
        };
        assert_eq!(plain.px, marked.px);
    }
}
