//! A waveform on a panel: a sound's lowest and highest sample a column, drawn straight or in
//! dB, over whatever span the tool asked its server for. For looking: nothing is played from
//! it. A tool zooms by asking for another span, and maps a click back to a time with
//! [`Wave::time_at`].
//!
//! A straight picture shows the first milliseconds and hides a tail. In dB the tail shows,
//! down to a floor, and a curve can be laid over it: a decay, say.

use crate::{Align, Pen, Rect, Rgb, dark};

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
        self.rrect(r, 3.0, dark::WELL, dark::RULE);
        let plot = Wave::plot(r);
        let d = self.v.rect(plot);
        let span = w.to - w.from;
        if d.w <= 0 || d.h <= 0 || span.is_nan() || span <= 0.0 {
            return;
        }
        let thin = self.v.len(1.0);
        // The ground: silence across the middle, or a line every 20 dB.
        match w.scale {
            Scale::Linear => self.sf.fill(Rect::new(d.x, d.y + d.h / 2, d.w, thin), dark::RULE),
            Scale::Db { floor } => {
                let mut db = -20.0;
                while db > floor {
                    let y = d.y + (d.h as f32 * db / floor) as i32;
                    self.sf.fill(Rect::new(d.x, y, d.w, thin), dark::RULE);
                    db -= 20.0;
                }
            }
        }
        // The sound: for each pixel across, the columns that fall on it.
        let n = w.lo.len().min(w.hi.len());
        for px in 0..d.w as usize {
            if n == 0 {
                break;
            }
            let a = (px * n / d.w as usize).min(n - 1);
            let b = ((px + 1) * n / d.w as usize).clamp(a + 1, n);
            let lo = w.lo[a..b].iter().fold(f32::INFINITY, |m, v| m.min(*v));
            let hi = w.hi[a..b].iter().fold(f32::NEG_INFINITY, |m, v| m.max(*v));
            let x = d.x + px as i32;
            match w.scale {
                Scale::Linear => {
                    let half = d.h as f32 / 2.0;
                    let y0 = (half - hi.clamp(-1.0, 1.0) * half) as i32;
                    let y1 = (half - lo.clamp(-1.0, 1.0) * half) as i32;
                    let y0 = y0.clamp(0, d.h - 1);
                    self.sf.fill(Rect::new(x, d.y + y0, 1, (y1 - y0).clamp(1, d.h - y0)), dark::TAPE);
                }
                Scale::Db { floor } => {
                    let peak = lo.abs().max(hi.abs());
                    let db = 20.0 * peak.log10();
                    if peak > 0.0 && db > floor {
                        let h = ((d.h as f32 * (1.0 - db.min(0.0) / floor)) as i32).clamp(1, d.h);
                        self.sf.fill(Rect::new(x, d.bottom() - h, 1, h), dark::TAPE);
                    }
                }
            }
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
                    self.sf.line(a.0, a.1, b.0, b.1, thin as f32, dark::TEXT);
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
                    self.fill(Rect::new(label.x - 2, label.y, wide, label.h), dark::WELL);
                    self.text(label, &said, 9.0, false, dark::DIM, Align::Left);
                }
                db -= 20.0;
            }
        }
        // The times: a tick and its figure, about every 70 logical pixels.
        let each = step(span * 70.0 / plot.w.max(1) as f64);
        let axis = Rect::new(plot.x, plot.bottom(), plot.w, AXIS);
        self.fill(Rect::new(axis.x, axis.y, axis.w, 1), dark::RULE);
        let mut t = (w.from / each).ceil() * each;
        let mut first = true;
        while t <= w.to {
            let x = plot.x as f32 + ((t - w.from) / span) as f32 * plot.w as f32;
            let dx = self.v.x(x).min(d.right() - thin);
            self.sf.fill(Rect::new(dx, self.v.y(axis.y as f32), thin, self.v.len(3.0)), dark::DIM);
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
                    dark::DIM,
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
            self.fill(Rect::new(lx - 1, y, wide + 1, 10), dark::WELL);
            self.text(Rect::new(lx, y, wide, 10), m.label, 9.0, true, m.color, align);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
