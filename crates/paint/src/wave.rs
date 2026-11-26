//! A waveform on a panel: a sound's lowest and highest sample a column, drawn straight or in
//! dB, over whatever span the tool asked its server for. For looking: nothing is played from
//! it. A tool zooms by asking for another span, and maps a click back to a time with
//! [`Wave::time_at`].
//!
//! A straight picture shows the first milliseconds and hides a tail. In dB the tail shows,
//! down to a floor, and a curve can be laid over it: a decay, say.
//!
//! To place a mark by (`as-wave`'s `RFC-waveform.md`, w2): a [`Window`] follows the mark, as
//! wide as its nudge step asks, and [`Pen::wave_with_whole`] draws it over a strip of the
//! [`Whole`] sound with the window's place lit. Where the window runs past the sound's ends it's
//! empty. `as-page-kit`'s `kitWave` is the same on a page, and the two are held to one table
//! of numbers.

use crate::{Align, Pen, Rect, Rgb, skin};

/// How the samples are set up the picture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Scale {
    /// Full scale at the top and bottom, silence across the middle.
    Linear,
    /// Each column's loudest sample, full scale at the top, `floor` dB (below zero) at the
    /// bottom.
    Db { floor: f32 },
}

/// A place marked on the picture: an onset, where a tail starts.
#[derive(Clone, Copy, Debug)]
pub struct Marker<'a> {
    /// In the span's own units.
    pub at: f64,
    pub label: &'a str,
    pub color: Rgb,
}

/// What to draw.
#[derive(Clone, Copy, Debug)]
pub struct Wave<'a> {
    /// The lowest and the highest sample in each column, full scale 1. The same length. As
    /// many columns as the tool likes: they're fitted to the pixels there are.
    pub lo: &'a [f32],
    pub hi: &'a [f32],
    /// The span the columns cover, in the tool's units, and what to call them (`ms`, `s`).
    pub from: f64,
    pub to: f64,
    pub unit: &'a str,
    pub scale: Scale,
    /// A curve over a dB picture, a point a column, in dB; `None` where it has none. Empty for
    /// no curve. Not drawn on a straight picture.
    pub curve: &'a [Option<f32>],
    pub markers: &'a [Marker<'a>],
}

/// The strip along the bottom for the times, in logical pixels.
const AXIS: i32 = 13;

/// A [`Window`] is this many of the mark's nudge steps across, so the step a tool's digits set
/// sets how close the picture is too.
pub const WINDOW_STEPS: f64 = 40.0;
/// A [`Window`] keeps its mark at least this many steps inside each edge, so there's always
/// some of what's ahead of it to see.
pub const EDGE_STEPS: f64 = 2.0;

/// The overview strip's height, and the gap over it, in logical pixels.
const STRIP: i32 = 24;
const STRIP_GAP: i32 = 3;

/// The span a waveform shows while a mark is placed on it: [`WINDOW_STEPS`] steps across. It
/// opens with the mark centred, then scrolls as little as keeps the mark in view, as a
/// [`crate::List`] keeps its chosen row. It's never held to the sound: near an end it runs
/// past it, so a mark there isn't pinned to the frame. In the tool's own units; the tool asks
/// its server to picture `from` to `to`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Window {
    pub from: f64,
    pub to: f64,
}

impl Window {
    /// A window opened on `mark` while its nudge step is `step`.
    pub fn open(mark: f64, step: f64) -> Window {
        let from = mark - WINDOW_STEPS * step / 2.0;
        Window { from, to: from + WINDOW_STEPS * step }
    }

    /// The mark, or the step, may have moved. A new step changes the width with the mark where
    /// it was across the window. No step (zero or less), or no mark, and the window stays.
    pub fn follow(&mut self, mark: f64, step: f64) {
        let width = WINDOW_STEPS * step;
        if !(width > 0.0 && width.is_finite() && mark.is_finite()) {
            return;
        }
        let edge = EDGE_STEPS / WINDOW_STEPS;
        let was = self.to - self.from;
        if (width - was).abs() > width * 1e-9 {
            let across = if was > 0.0 { ((mark - self.from) / was).clamp(edge, 1.0 - edge) } else { 0.5 };
            self.from = mark - across * width;
        }
        let margin = edge * width;
        if mark < self.from + margin {
            self.from = mark - margin;
        } else if mark > self.from + width - margin {
            self.from = mark - width + margin;
        }
        self.to = self.from + width;
    }
}

/// The whole sound a window is part of: its picture, and where it starts and ends, in the
/// window's units.
#[derive(Clone, Copy, Debug)]
pub struct Whole<'a> {
    pub lo: &'a [f32],
    pub hi: &'a [f32],
    pub from: f64,
    pub to: f64,
}

impl Whole<'_> {
    /// `r` for a window over its strip, as [`Pen::wave_with_whole`] draws them: the window's
    /// box, for [`Wave::time_at`], and the strip's, for [`Whole::time_at`].
    pub fn split(r: Rect) -> (Rect, Rect) {
        let window = Rect::new(r.x, r.y, r.w, (r.h - STRIP - STRIP_GAP).max(0));
        (window, Rect::new(r.x, r.bottom() - STRIP, r.w, STRIP))
    }

    /// The time at the logical `x` of the strip in `strip`: where a click fell. Held to the
    /// sound.
    pub fn time_at(&self, strip: Rect, x: f32) -> f64 {
        let t = ((x - (strip.x + 1) as f32) / (strip.w - 2).max(1) as f32).clamp(0.0, 1.0) as f64;
        self.from + (self.to - self.from) * t
    }
}

/// The lowest and highest of the `n` columns that fall on pixel `px` of `across`.
fn fold(lo: &[f32], hi: &[f32], n: usize, px: usize, across: usize) -> (f32, f32) {
    let a = (px * n / across).min(n - 1);
    let b = ((px + 1) * n / across).clamp(a + 1, n);
    let lo = lo[a..b].iter().fold(f32::INFINITY, |m, v| m.min(*v));
    let hi = hi[a..b].iter().fold(f32::NEG_INFINITY, |m, v| m.max(*v));
    (lo, hi)
}

/// A step of 1, 2 or 5 times a power of ten, the smallest that's at least `least`.
fn step(least: f64) -> f64 {
    if least.is_nan() || least <= 0.0 || !least.is_finite() {
        return 1.0;
    }
    let p = 10f64.powf(least.log10().floor());
    [1.0, 2.0, 5.0, 10.0].iter().map(|m| m * p).find(|s| *s >= least * 0.999_999).unwrap_or(10.0 * p)
}

/// A time for the axis: no more decimals than its step needs.
fn said(v: f64, step: f64) -> String {
    let decimals = if step >= 1.0 { 0 } else { (-step.log10()).ceil() as usize };
    format!("{v:.decimals$}")
}

impl Wave<'_> {
    /// Where the columns are drawn, inside `r`: within its edge and above the times.
    pub fn plot(r: Rect) -> Rect {
        Rect::new(r.x + 1, r.y + 1, (r.w - 2).max(0), (r.h - 2 - AXIS).max(0))
    }

    /// The time at the logical `x` of a picture drawn in `r`: where a click fell. Held to the
    /// span.
    pub fn time_at(&self, r: Rect, x: f32) -> f64 {
        let p = Self::plot(r);
        let t = ((x - p.x as f32) / p.w.max(1) as f32).clamp(0.0, 1.0) as f64;
        self.from + (self.to - self.from) * t
    }
}

impl Pen<'_, '_> {
    /// The waveform in `r`: a `well` with the sound in the skin's `tape`, the times along the
    /// bottom, and the markers over it, each with its label.
    pub fn wave(&mut self, r: Rect, w: &Wave) {
        self.wave_over(r, w, None);
    }

    /// A window `w` onto a longer sound, in `r` split as [`Whole::split`] has it: the window,
    /// empty where it runs past the sound's ends, over a strip of the whole sound, faint, with
    /// the window's place lit.
    pub fn wave_with_whole(&mut self, r: Rect, w: &Wave, whole: &Whole) {
        let (window, strip) = Whole::split(r);
        self.wave_over(window, w, Some((whole.from, whole.to)));
        self.strip(strip, w, whole);
    }

    fn strip(&mut self, r: Rect, w: &Wave, whole: &Whole) {
        self.rrect(r, 3.0, skin().well, skin().rule);
        let d = self.v.rect(Rect::new(r.x + 1, r.y + 1, (r.w - 2).max(0), (r.h - 2).max(0)));
        let long = whole.to - whole.from;
        if d.w <= 0 || d.h <= 0 || long.is_nan() || long <= 0.0 {
            return;
        }
        // The window's place, in pixels across: wide enough to see, however long the sound.
        let at = |t: f64| (((t - whole.from) / long).clamp(0.0, 1.0) * d.w as f64).round() as i32;
        let least = self.v.len(3.0).min(d.w);
        let (mut a, mut b) = (at(w.from), at(w.to));
        if b - a < least {
            a = ((a + b - least) / 2).clamp(0, d.w - least);
            b = a + least;
        }
        // The window's mark has rows of its own, over and under the sound, so it never covers
        // any: in dB the quiet is at the bottom.
        let thick = self.v.len(2.0);
        let sound = Rect::new(d.x, d.y + thick, d.w, d.h - 2 * thick);
        let n = whole.lo.len().min(whole.hi.len());
        for px in 0..d.w {
            if n == 0 || sound.h <= 0 {
                break;
            }
            let (lo, hi) = fold(whole.lo, whole.hi, n, px as usize, d.w as usize);
            let lit = (a..b).contains(&px);
            let color = if lit { skin().tape } else { skin().faint };
            self.column(Rect::new(d.x + px, sound.y, 1, sound.h), lo, hi, w.scale, color);
        }
        // The mark, as a list has one for where a long one is: it shows where the sound is
        // silent too. The ground stays the well's, where the sound's colour reads in every skin.
        for y in [d.y, d.bottom() - thick] {
            self.sf.fill(Rect::new(d.x + a, y, b - a, thick), skin().dim);
        }
    }

    /// One pixel across of the sound, `lo` to `hi`, in the box `d`, a pixel wide.
    fn column(&mut self, d: Rect, lo: f32, hi: f32, scale: Scale, color: Rgb) {
        match scale {
            Scale::Linear => {
                let half = d.h as f32 / 2.0;
                let y0 = (half - hi.clamp(-1.0, 1.0) * half) as i32;
                let y1 = (half - lo.clamp(-1.0, 1.0) * half) as i32;
                let y0 = y0.clamp(0, d.h - 1);
                self.sf.fill(Rect::new(d.x, d.y + y0, 1, (y1 - y0).clamp(1, d.h - y0)), color);
            }
            Scale::Db { floor } => {
                let peak = lo.abs().max(hi.abs());
                let db = 20.0 * peak.log10();
                if peak > 0.0 && db > floor {
                    let h = ((d.h as f32 * (1.0 - db.min(0.0) / floor)) as i32).clamp(1, d.h);
                    self.sf.fill(Rect::new(d.x, d.bottom() - h, 1, h), color);
                }
            }
        }
    }

    /// [`Pen::wave`], and with the `sound`'s start and end, nothing drawn past them.
    fn wave_over(&mut self, r: Rect, w: &Wave, sound: Option<(f64, f64)>) {
        self.rrect(r, 3.0, skin().well, skin().rule);
        let plot = Wave::plot(r);
        let d = self.v.rect(plot);
        let span = w.to - w.from;
        if d.w <= 0 || d.h <= 0 || span.is_nan() || span <= 0.0 {
            return;
        }
        let thin = self.v.len(1.0);
        // The pixels across that have sound: all of them, unless the sound's ends are said.
        let (first, end) = sound.map_or((0, d.w), |(from, to)| {
            let at = |t: f64| ((t - w.from) / span).clamp(0.0, 1.0) * d.w as f64;
            let first = at(from).floor() as i32;
            (first, (at(to).ceil() as i32).max(first))
        });
        // The ground: silence across the middle, or a line every 20 dB.
        match w.scale {
            Scale::Linear => {
                self.sf.fill(Rect::new(d.x + first, d.y + d.h / 2, end - first, thin), skin().rule)
            }
            Scale::Db { floor } => {
                let mut db = -20.0;
                while db > floor {
                    let y = d.y + (d.h as f32 * db / floor) as i32;
                    self.sf.fill(Rect::new(d.x + first, y, end - first, thin), skin().rule);
                    db -= 20.0;
                }
            }
        }
        // The sound: for each pixel across, the columns that fall on it.
        let n = w.lo.len().min(w.hi.len());
        for px in first..end {
            if n == 0 {
                break;
            }
            let (lo, hi) = fold(w.lo, w.hi, n, px as usize, d.w as usize);
            self.column(Rect::new(d.x + px, d.y, 1, d.h), lo, hi, w.scale, skin().tape);
        }
        // The curve, over a dB picture.
        if let Scale::Db { floor } = w.scale
            && !w.curve.is_empty()
        {
            let at = |k: usize| -> Option<(f32, f32)> {
                let db = w.curve[k]?;
                let x = d.x as f32 + (k as f32 + 0.5) * d.w as f32 / w.curve.len() as f32;
                let y = d.y as f32 + d.h as f32 * (db.clamp(floor, 0.0) / floor);
                Some((x, y.min(d.bottom() as f32 - 1.0)))
            };
            let mut last: Option<(f32, f32)> = None;
            for k in 0..w.curve.len() {
                let here = at(k);
                if let (Some(a), Some(b)) = (last, here) {
                    self.sf.line(a.0, a.1, b.0, b.1, thin as f32, skin().text);
                }
                last = here;
            }
        }
        if let Scale::Db { floor } = w.scale {
            let mut db = -20.0;
            while db > floor {
                let y = plot.y + (plot.h as f32 * db / floor) as i32;
                let label = Rect::new(plot.x + 3, y - 11, 40, 10);
                if label.y > plot.y {
                    // On a ground of its own: the sound may be behind it.
                    let said = format!("{db:.0}");
                    let wide = self.width(&said, 9.0, false).ceil() as i32 + 4;
                    self.fill(Rect::new(label.x - 2, label.y, wide, label.h), skin().well);
                    self.text(label, &said, 9.0, false, skin().dim, Align::Left);
                }
                db -= 20.0;
            }
        }
        // The times: a tick and its figure, about every 70 logical pixels.
        let each = step(span * 70.0 / plot.w.max(1) as f64);
        let axis = Rect::new(plot.x, plot.bottom(), plot.w, AXIS);
        self.fill(Rect::new(axis.x, axis.y, axis.w, 1), skin().rule);
        let mut t = (w.from / each).ceil() * each;
        let mut first = true;
        while t <= w.to {
            let x = plot.x as f32 + ((t - w.from) / span) as f32 * plot.w as f32;
            let dx = self.v.x(x).min(d.right() - thin);
            self.sf.fill(Rect::new(dx, self.v.y(axis.y as f32), thin, self.v.len(3.0)), skin().dim);
            let mut label = said(t, each);
            if first {
                label = format!("{label} {}", w.unit);
                first = false;
            }
            let wide = self.width(&label, 9.0, false).ceil() as i32 + 2;
            if x as i32 + 3 + wide <= axis.right() {
                self.text(
                    Rect::new(x as i32 + 3, axis.y + 2, wide, AXIS - 2),
                    &label,
                    9.0,
                    false,
                    skin().dim,
                    Align::Left,
                );
            }
            t += each;
        }
        // The markers, each label on a line of its own so two close together don't collide.
        for (k, m) in w.markers.iter().enumerate() {
            if m.at < w.from || m.at > w.to {
                continue;
            }
            let x = plot.x as f32 + ((m.at - w.from) / span) as f32 * plot.w as f32;
            let dx = self.v.x(x).min(d.right() - thin);
            self.sf.fill(Rect::new(dx, d.y, thin, d.h), m.color);
            let wide = self.width(m.label, 9.0, true).ceil() as i32 + 2;
            let y = plot.y + 2 + 10 * k as i32;
            if y + 10 > plot.bottom() {
                continue;
            }
            let (lx, align) = if x as i32 + 4 + wide <= plot.right() {
                (x as i32 + 4, Align::Left)
            } else {
                (x as i32 - 3 - wide, Align::Right)
            };
            self.fill(Rect::new(lx - 1, y, wide + 1, 10), skin().well);
            self.text(Rect::new(lx, y, wide, 10), m.label, 9.0, true, m.color, align);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dark;
    use crate::pen::tests::{fonts, has};
    use crate::{Image, View};

    /// A click and a fall from it: full scale at the first column, down 60 dB by the last.
    fn falling(n: usize) -> (Vec<f32>, Vec<f32>) {
        let hi: Vec<f32> = (0..n).map(|k| 10f32.powf(-3.0 * k as f32 / n as f32)).collect();
        (hi.iter().map(|v| -v).collect(), hi)
    }

    const R: Rect = Rect::new(0, 0, 400, 120);

    fn drawn(w: &Wave) -> Option<Image> {
        let fs = fonts()?;
        let mut img = Image::new(400, 120);
        let mut sf = img.surface();
        let mut p = Pen { sf: &mut sf, fonts: &fs, v: View::fit(400, 120, 400, 120) };
        p.wave(R, w);
        Some(img)
    }

    /// The rows of column `x` that are the sound's colour.
    fn sound(img: &Image, x: usize) -> Vec<usize> {
        (0..img.h).filter(|&y| img.px[y * img.w + x] == dark::TAPE).collect()
    }

    fn wave<'a>(lo: &'a [f32], hi: &'a [f32], scale: Scale) -> Wave<'a> {
        Wave { lo, hi, from: 0.0, to: 500.0, unit: "ms", scale, curve: &[], markers: &[] }
    }

    #[test]
    fn straight_it_shows_the_start_and_in_db_the_tail() {
        let (lo, hi) = falling(2000);
        let Some(lin) = drawn(&wave(&lo, &hi, Scale::Linear)) else { return };
        let plot = Wave::plot(R);
        let first = sound(&lin, 1);
        assert_eq!((first[0], first.len() as i32), (1, plot.h), "full scale fills the height");
        let late = sound(&lin, 390);
        assert!(late.len() <= 2, "down 58 dB, straight: a line. {late:?}");
        assert_eq!(late[0] as i32, plot.y + plot.h / 2);
        let db = drawn(&wave(&lo, &hi, Scale::Db { floor: -90.0 })).unwrap();
        let first = sound(&db, 1);
        assert_eq!((first[0], *first.last().unwrap() as i32), (1, plot.bottom() - 1));
        let late = sound(&db, 390);
        let want = plot.h as f32 * (1.0 - 58.5 / 90.0);
        assert!((late.len() as f32 - want).abs() < 3.0, "{} {want}", late.len());
        assert_eq!(*late.last().unwrap() as i32, plot.bottom() - 1, "it stands on the floor");
        // Below the floor: nothing.
        let quiet = drawn(&wave(&lo, &hi, Scale::Db { floor: -40.0 })).unwrap();
        assert!(sound(&quiet, 390).is_empty());
    }

    #[test]
    fn fewer_columns_than_pixels_and_none_at_all() {
        let (lo, hi) = falling(7);
        let Some(img) = drawn(&wave(&lo, &hi, Scale::Linear)) else { return };
        assert!((1..399).all(|x| !sound(&img, x).is_empty()), "every pixel across has its column");
        let none = drawn(&wave(&[], &[], Scale::Linear)).unwrap();
        assert!(sound(&none, 100).is_empty());
        assert!(has(&none, R, dark::WELL));
        // A span of nothing draws its box and stops.
        let (lo, hi) = falling(10);
        let mut w = wave(&lo, &hi, Scale::Linear);
        w.to = 0.0;
        assert!(sound(&drawn(&w).unwrap(), 100).is_empty());
    }

    #[test]
    fn markers_times_and_a_curve() {
        let (lo, hi) = falling(400);
        let curve: Vec<Option<f32>> =
            (0..400).map(|k| if k < 40 { None } else { Some(-60.0 * (k - 40) as f32 / 360.0) }).collect();
        let markers = [
            Marker { at: 50.0, label: "ONSET", color: dark::GREEN },
            Marker { at: 495.0, label: "TAIL", color: dark::AMBER },
            Marker { at: 900.0, label: "PAST", color: dark::RED },
        ];
        let w = Wave { curve: &curve, markers: &markers, ..wave(&lo, &hi, Scale::Db { floor: -90.0 }) };
        let Some(img) = drawn(&w) else { return };
        let plot = Wave::plot(R);
        let col = |x: i32, c| {
            (plot.y..plot.bottom()).filter(|&y| img.px[y as usize * 400 + x as usize] == c).count()
        };
        let onset = (plot.x as f32 + plot.w as f32 / 10.0).round() as i32;
        assert_eq!(col(onset, dark::GREEN) as i32, plot.h, "a marker runs the height");
        assert!(has(&img, Rect::new(onset + 3, 0, 60, 14), dark::GREEN), "its label beside it");
        assert!(has(&img, Rect::new(300, 10, 90, 14), dark::AMBER), "near the edge, the label goes left");
        assert!(!has(&img, R, dark::RED), "outside the span: not drawn");
        assert!(has(&img, plot, dark::TEXT), "the curve");
        assert_eq!(col(plot.x + 20, dark::TEXT), 0, "and none where it has no point");
        let axis = Rect::new(0, plot.bottom(), 400, 14);
        assert!(has(&img, axis, dark::DIM), "the times");
        assert!(has(&img, Rect::new(plot.x, plot.y, 40, plot.h), dark::DIM), "the dB lines' figures");
        // A click back to a time.
        assert_eq!(w.time_at(R, plot.x as f32), 0.0);
        assert!((w.time_at(R, onset as f32) - 50.0).abs() < 1.0);
        assert_eq!(w.time_at(R, 5000.0), 500.0);
    }

    #[test]
    fn steps_are_round() {
        assert_eq!(step(0.7), 1.0);
        assert_eq!(step(1.2), 2.0);
        assert_eq!(step(30.0), 50.0);
        assert_eq!(step(0.003), 0.005);
        assert_eq!(step(100.0), 100.0);
        assert_eq!(step(0.0), 1.0);
        assert_eq!(
            (said(150.0, 50.0), said(0.25, 0.05), said(2.5, 0.5)),
            ("150".into(), "0.25".into(), "2.5".into())
        );
    }

    /// The table both halves are held to, and the numbers in page-kit.js, when the audio suite
    /// is beside this one.
    #[test]
    fn the_window_moves_as_the_pages_does() {
        let kit = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../suite-audio-libs/crates/page-kit/");
        let (Ok(cases), Ok(js)) = (
            std::fs::read_to_string(format!("{kit}testdata/wave-view.txt")),
            std::fs::read_to_string(format!("{kit}src/page-kit.js")),
        ) else {
            eprintln!("no suite-audio-libs beside this suite: the window's table not compared");
            return;
        };
        let said = format!(r#"const KIT_WAVE = {{ "steps": {WINDOW_STEPS}, "edge": {EDGE_STEPS} }};"#);
        assert!(js.contains(&said), "page-kit.js doesn't say {said}");
        assert_eq!(run(&cases), 22, "every line of the table");
    }

    /// Runs a table of the window's cases, and says how many lines it held.
    fn run(cases: &str) -> usize {
        let mut win = Window { from: 0.0, to: 0.0 };
        let mut held = 0;
        for line in cases.lines().filter(|l| l.starts_with("open ") || l.starts_with("follow ")) {
            let mut words = line.split_whitespace();
            let what = words.next().unwrap();
            let n: Vec<f64> = words.map(|w| w.parse().unwrap()).collect();
            if what == "open" {
                win = Window::open(n[0], n[1]);
            } else {
                win.follow(n[0], n[1]);
            }
            assert!((win.from - n[2]).abs() < 1e-9 && (win.to - n[3]).abs() < 1e-9, "{line}: {win:?}");
            held += 1;
        }
        held
    }

    /// The same rules with no sibling suite.
    #[test]
    fn a_window_follows_its_mark() {
        let own = "open 1000 5 900 1100\nfollow 1095 5 905 1105\nfollow 900 5 890 1090\n\
                   open 1000 50 0 2000\nfollow 1500 10 1200 1600\nopen 0 10 -200 200\n";
        assert_eq!(run(own), 6);
        let mut win = Window::open(10.0, 1.0);
        win.follow(f64::NAN, 1.0);
        win.follow(20.0, f64::INFINITY);
        assert_eq!(win, Window { from: -10.0, to: 30.0 });
        // However it's walked, the mark stays two steps inside.
        let mut mark = 0.0;
        for k in 0..5_000i64 {
            let step = [1.0, 5.0, 30.0, 500.0][(k / 97 % 4) as usize] / 7.0;
            mark += ((k * 31 % 11) - 5) as f64 * step;
            win.follow(mark, step);
            let inside = EDGE_STEPS * step - 1e-6;
            assert!(mark - win.from >= inside && win.to - mark >= inside, "{k}: {mark} {win:?}");
            assert!((win.to - win.from - WINDOW_STEPS * step).abs() < 1e-6, "{k}");
        }
    }

    const OVER: Rect = Rect::new(0, 0, 400, 147);

    fn windowed(w: &Wave, whole: &Whole) -> Option<Image> {
        let fs = fonts()?;
        let mut img = Image::new(400, 147);
        let mut sf = img.surface();
        let mut p = Pen { sf: &mut sf, fonts: &fs, v: View::fit(400, 147, 400, 147) };
        p.wave_with_whole(OVER, w, whole);
        Some(img)
    }

    /// The columns across that have `color` somewhere in `r`.
    fn across(img: &Image, r: Rect, color: Rgb) -> Vec<i32> {
        (r.x..r.right())
            .filter(|&x| (r.y..r.bottom()).any(|y| img.px[y as usize * img.w + x as usize] == color))
            .collect()
    }

    #[test]
    fn past_an_end_a_window_is_empty() {
        let (lo, hi) = (vec![-0.5f32; 200], vec![0.5f32; 200]);
        let whole = Whole { lo: &lo, hi: &hi, from: 0.0, to: 1000.0 };
        let (window, _) = Whole::split(OVER);
        let plot = Wave::plot(window);
        let upper = Rect::new(plot.x, plot.y, plot.w, plot.h);
        for scale in [Scale::Linear, Scale::Db { floor: -90.0 }] {
            // A quarter of it before the start.
            let w = Wave { from: -100.0, to: 300.0, ..wave(&lo, &hi, scale) };
            let Some(img) = windowed(&w, &whole) else { return };
            let quarter = plot.x + plot.w / 4;
            let sound = across(&img, upper, dark::TAPE);
            assert_eq!((sound[0], *sound.last().unwrap()), (quarter, plot.right() - 1), "{scale:?}");
            // No ground there either: clear of the dB figures at the left, the well alone.
            let blank = Rect::new(plot.x + 50, plot.y, quarter - plot.x - 51, plot.h);
            assert!(across(&img, blank, dark::RULE).is_empty(), "{scale:?}");
            // Past the end, and wholly past it.
            let w = Wave { from: 900.0, to: 1100.0, ..wave(&lo, &hi, scale) };
            let img = windowed(&w, &whole).unwrap();
            let sound = across(&img, upper, dark::TAPE);
            assert_eq!(*sound.last().unwrap(), plot.x + plot.w / 2 - 1, "{scale:?}");
            let w = Wave { from: 2000.0, to: 2400.0, ..wave(&lo, &hi, scale) };
            assert!(across(&windowed(&w, &whole).unwrap(), upper, dark::TAPE).is_empty(), "{scale:?}");
        }
    }

    #[test]
    fn the_strip_lights_the_windows_place() {
        let (lo, hi) = (vec![-0.5f32; 200], vec![0.5f32; 200]);
        let whole = Whole { lo: &lo, hi: &hi, from: 0.0, to: 1000.0 };
        let (window, strip) = Whole::split(OVER);
        assert_eq!((window.bottom() + STRIP_GAP, strip.bottom()), (strip.y, OVER.bottom()));
        let inner = Rect::new(strip.x + 1, strip.y + 1, strip.w - 2, strip.h - 2);
        let lit = |from, to| {
            let w = Wave { from, to, ..wave(&lo, &hi, Scale::Linear) };
            let img = windowed(&w, &whole)?;
            let (lit, faint) = (across(&img, inner, dark::TAPE), across(&img, inner, dark::FAINT));
            assert_eq!(lit.len() + faint.len(), inner.w as usize, "the whole sound across: {from}");
            Some((lit[0] - inner.x, lit.len() as i32))
        };
        let Some(quarter) = lit(250.0, 500.0) else { return };
        assert_eq!(quarter, (100, 99), "398 pixels across");
        assert_eq!(lit(-100.0, 100.0).unwrap(), (0, 40), "past the start: what's in the sound");
        // A window too narrow to see is still shown, and one wholly past an end is at that end.
        let thin = lit(500.0, 500.04).unwrap();
        assert!(thin.1 == 3 && (thin.0 - 198).abs() <= 2, "{thin:?}");
        assert_eq!(lit(5000.0, 5001.0).unwrap(), (395, 3));
        assert_eq!(lit(-9.0, -8.0).unwrap(), (0, 3));
        // A click on the strip is a time in the whole sound.
        assert_eq!(whole.time_at(strip, 1.0), 0.0);
        assert!((whole.time_at(strip, 200.0) - 500.0).abs() < 2.0);
        assert_eq!(whole.time_at(strip, 900.0), 1000.0);
    }

    #[test]
    fn the_strips_mark_covers_none_of_the_quiet() {
        // A tail 4 dB over the floor: a pixel or two tall in the strip.
        let v = 10f32.powf(-56.0 / 20.0);
        let (lo, hi) = (vec![-v; 200], vec![v; 200]);
        let whole = Whole { lo: &lo, hi: &hi, from: 0.0, to: 1000.0 };
        let w = Wave { from: 250.0, to: 500.0, ..wave(&lo, &hi, Scale::Db { floor: -60.0 }) };
        let Some(img) = windowed(&w, &whole) else { return };
        let (_, strip) = Whole::split(OVER);
        let inner = Rect::new(strip.x + 1, strip.y + 1, strip.w - 2, strip.h - 2);
        assert_eq!(across(&img, inner, dark::TAPE).len(), 99, "the quiet, all across the window's place");
    }
}
