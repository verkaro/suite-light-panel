//! A dial: a knob for a level, with a ring round it lit by the signal it governs, as an
//! interface's own front panel has (`../STYLE.md`, "The parts"). Written here for
//! focusrite-ctrl, the second panel to want a dial (2026-11-10; dexter-kms has its own). Its look
//! follows dexter-kms's: a sweep of three quarters of a turn from about half past seven, the lit
//! part over a `well`, the body a key's colours, a pointer.
//!
//! The ring is Dexter's LED disc as a ring ([`Disc`], with the disc's colours): out when
//! there's no signal, white brighter as it's louder, yellow at a caution, red when hot, and a
//! second ring outside it while a clip is latched. A dial whose source isn't metered has no ring
//! at all, so an unlit ring always means silence. The ballistics, the hold and the clip's latch
//! are the tool's, as for the disc: the dial draws the state it's given.
//!
//! [`knob_pos`] and [`knob_db`] are the law between a level and the sweep.

use std::f32::consts::PI;

use crate::meter::disc_color;
use crate::{Align, Disc, Hits, Pen, Rect, dark, mix};

/// Where the sweep starts, clockwise from three o'clock (y runs down): about half past seven.
pub const DIAL_A0: f32 = 0.75 * PI;
/// How far it goes: three quarters of a turn, to about half past four.
pub const DIAL_SWEEP: f32 = 1.5 * PI;

/// Below this, the sweep's first quarter goes to the bottom of the range.
pub const KNOB_KNEE_DB: f64 = -40.0;
const KNEE_POS: f64 = 0.25;

/// Where a level sits along the sweep, 0 to 1, for a range of `lo` to `hi` dB: off (None) and
/// `lo` at 0, [`KNOB_KNEE_DB`] a quarter of the way, then even in dB to `hi` at 1. So the
/// working range has three quarters of the turn, as a desk's knob does.
pub fn knob_pos(db: Option<f64>, lo: f64, hi: f64) -> f32 {
    let Some(db) = db else { return 0.0 };
    let db = db.clamp(lo, hi);
    let knee = KNOB_KNEE_DB.clamp(lo, hi);
    let p = if db >= knee {
        if hi > knee { KNEE_POS + (1.0 - KNEE_POS) * (db - knee) / (hi - knee) } else { 1.0 }
    } else if knee > lo {
        KNEE_POS * (db - lo) / (knee - lo)
    } else {
        0.0
    };
    p as f32
}

/// The level at a place along the sweep ([`knob_pos`] turned round); None at 0: off.
pub fn knob_db(pos: f32, lo: f64, hi: f64) -> Option<f64> {
    let p = pos.clamp(0.0, 1.0) as f64;
    if p <= 0.0 {
        return None;
    }
    let knee = KNOB_KNEE_DB.clamp(lo, hi);
    Some(if p >= KNEE_POS {
        knee + (p - KNEE_POS) / (1.0 - KNEE_POS) * (hi - knee)
    } else {
        lo + p / KNEE_POS * (knee - lo)
    })
}

/// The angle of a place along the sweep.
pub fn dial_angle(pos: f32) -> f32 {
    DIAL_A0 + DIAL_SWEEP * pos.clamp(0.0, 1.0)
}

/// What a dial shows.
#[derive(Clone, Copy, Debug, Default)]
pub struct Dial<'a> {
    /// Over it: what it's the level of (`MONITOR L`, `ANALOGUE 1`).
    pub label: &'a str,
    /// Under it, in figures: `-6.0`, `OFF`.
    pub value: &'a str,
    /// Along the sweep, 0 to 1 ([`knob_pos`]).
    pub pos: f32,
    /// A mark on the sweep (0 dB's place), if the range has one.
    pub mark: Option<f32>,
    /// The signal it governs. None: not metered, so no ring.
    pub ring: Option<Disc>,
    /// A second ring inside the first: the signal after the knob's gain (Warren, 2026-11-10,
    /// for focusrite-ctrl). The outer ring is then what comes in. None: one ring.
    pub inner: Option<Disc>,
    /// Held by the interface's own knob (a line on HW): dimmed, `KNOB` said, not to be turned.
    pub held: bool,
    /// The hand is on it.
    pub cursor: bool,
    /// Being typed in place (`i`): this in place of the value, in a readout with an amber edge.
    pub typing: Option<&'a str>,
}

/// Where in a dial a point falls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialPart {
    /// The knob and its sweep: turn it.
    Knob,
    /// The ring: clear its clip.
    Ring,
}

/// A dial's circle in `r`: its centre and outer radius, logical. The label takes the 14 at
/// the top, the circle comes straight under it as wide as fits, and the value straight under
/// that, so a tall box doesn't leave the value adrift.
fn circle(r: Rect) -> (f32, f32, f32) {
    let rad = (r.w as f32).min((r.h as f32 - 28.0).max(8.0)) / 2.0;
    (r.x as f32 + r.w as f32 / 2.0, r.y as f32 + 14.0 + rad, rad)
}

// The rings from the outside in, as parts of the outer radius.
const CLIP_OUT: f32 = 1.0;
const CLIP_T: f32 = 0.06;
const RING_OUT: f32 = 0.9;
const RING_T: f32 = 0.12;
// With a second ring, the two share the first's room: what comes in outside, after the knob
// inside.
const TWO_OUT: f32 = 0.93;
const TWO_T: f32 = 0.08;
const TWO_IN: f32 = 0.83;
const SWEEP_OUT: f32 = 0.72;
const SWEEP_T: f32 = 0.09;
const BODY: f32 = 0.58;

/// Where a point of the logical canvas falls in a dial drawn in `r`: the ring outside the
/// sweep, the knob inside it, or neither.
pub fn dial_part(r: Rect, x: f32, y: f32) -> Option<DialPart> {
    let (cx, cy, rad) = circle(r);
    let d = (x - cx).hypot(y - cy) / rad;
    if d <= SWEEP_OUT + 0.02 {
        Some(DialPart::Knob)
    } else if d <= CLIP_OUT {
        Some(DialPart::Ring)
    } else {
        None
    }
}

impl Pen<'_, '_> {
    /// A dial in `r`. With an id it's somewhere to click, and the bar lists it by its label;
    /// [`dial_part`] says whether a click fell on the knob or the ring.
    pub fn dial(&mut self, hits: &mut Hits, r: Rect, d: &Dial, id: &str) {
        let (lx, ly, lrad) = circle(r);
        let (cx, cy) = (self.v.x(lx) as f32, self.v.y(ly) as f32);
        let rad = lrad * self.v.scale;
        let dim = |c| if d.held { mix(dark::PANEL, c, 0.45) } else { c };

        match (d.ring, d.inner) {
            (Some(disc), None) => {
                if disc == Disc::Clip {
                    self.sf.ring(cx, cy, rad * CLIP_OUT, (rad * CLIP_T).max(1.0), disc_color(Disc::Hot));
                }
                self.sf.ring(cx, cy, rad * RING_OUT, (rad * RING_T).max(1.5), disc_color(disc));
            }
            (outer, Some(inner)) => {
                // A latched clip of what comes in: the outer ring, red, a hair thicker.
                if let Some(o) = outer {
                    let t = if o == Disc::Clip { TWO_T * 1.3 } else { TWO_T };
                    self.sf.ring(cx, cy, rad * TWO_OUT, (rad * t).max(1.5), disc_color(o));
                }
                self.sf.ring(cx, cy, rad * TWO_IN, (rad * TWO_T).max(1.5), disc_color(inner));
            }
            (None, None) => {}
        }

        let sweep = (DIAL_A0, DIAL_A0 + DIAL_SWEEP);
        let t = (rad * SWEEP_T).max(1.5);
        self.sf.arc(cx, cy, rad * SWEEP_OUT, t, sweep, dark::WELL);
        if d.pos > 0.0 {
            let lit = if d.cursor { dark::AMBER } else { dark::DIM };
            self.sf.arc(cx, cy, rad * SWEEP_OUT, t, (DIAL_A0, dial_angle(d.pos)), dim(lit));
        }
        if let Some(m) = d.mark {
            let a = dial_angle(m);
            let (ca, sa) = (a.cos(), a.sin());
            let (i0, i1) = (rad * (SWEEP_OUT - SWEEP_T - 0.04), rad * (SWEEP_OUT + 0.04));
            self.sf.line(cx + ca * i0, cy + sa * i0, cx + ca * i1, cy + sa * i1, 1.0, dim(dark::TEXT));
        }

        let body = if d.cursor { dark::BTN_HI } else { dark::BTN };
        self.sf.disc(cx, cy, rad * BODY, dim(dark::BTN_EDGE));
        self.sf.disc(cx, cy, rad * BODY - self.v.scale.max(1.0), dim(body));
        let a = dial_angle(d.pos);
        let (ca, sa) = (a.cos(), a.sin());
        let (i0, i1) = (rad * BODY * 0.25, rad * BODY * 0.9);
        let w = (2.0 * self.v.scale).max(1.5);
        self.sf.line(cx + ca * i0, cy + sa * i0, cx + ca * i1, cy + sa * i1, w, dim(dark::TEXT));

        let label = Rect::new(r.x, r.y, r.w, 13);
        let color = if d.cursor { dark::TEXT } else { dark::DIM };
        self.text(label, d.label, 9.0, true, dim(color), Align::Centre);
        let under = Rect::new(r.x, (ly + lrad).round() as i32 + 1, r.w, 13);
        match d.typing {
            Some(typed) => {
                self.rrect(under, 2.0, dark::WELL, dark::AMBER);
                self.text(under.inset(1), typed, 10.0, false, dark::READOUT, Align::Centre);
            }
            None if d.held => {
                let said = format!("KNOB {}", d.value);
                self.text(under, &said, 9.0, true, dark::DIM, Align::Centre);
            }
            None => {
                let c = if d.cursor { dark::TEXT } else { dark::DIM };
                self.text(under, d.value, 10.0, false, c, Align::Centre);
            }
        }

        if !id.is_empty() {
            hits.key(r, id, &d.label.to_lowercase(), d.held);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_law_gives_the_working_range_three_quarters() {
        // A mix's gain, −80 to +12, as the Focusrites have it.
        assert_eq!(knob_pos(None, -80.0, 12.0), 0.0);
        assert_eq!(knob_pos(Some(-80.0), -80.0, 12.0), 0.0);
        assert_eq!(knob_pos(Some(-40.0), -80.0, 12.0), 0.25);
        assert_eq!(knob_pos(Some(12.0), -80.0, 12.0), 1.0);
        assert_eq!(knob_pos(Some(40.0), -80.0, 12.0), 1.0, "held to the range");
        let zero = knob_pos(Some(0.0), -80.0, 12.0);
        assert!((zero - (0.25 + 0.75 * 40.0 / 52.0) as f32).abs() < 1e-6);
        // A line's volume, −127 to 0: the bottom 87 dB in the first quarter.
        assert_eq!(knob_pos(Some(0.0), -127.0, 0.0), 1.0);
        assert!((knob_pos(Some(-83.5), -127.0, 0.0) - 0.125).abs() < 1e-6);
    }

    #[test]
    fn the_law_turns_round() {
        for (lo, hi) in [(-80.0, 12.0), (-127.0, 0.0), (-60.0, 0.0)] {
            assert_eq!(knob_db(0.0, lo, hi), None);
            for db in [lo + 0.5, -50.0, -40.0, -12.0, -6.0, -0.5, hi] {
                let back = knob_db(knob_pos(Some(db), lo, hi), lo, hi).unwrap();
                assert!((back - db).abs() < 1e-3, "{db} in {lo}..{hi} came back {back}");
            }
        }
    }

    #[test]
    fn the_sweep_runs_half_past_seven_to_half_past_four() {
        let at = |pos: f32| {
            let a = dial_angle(pos);
            (a.cos(), a.sin())
        };
        let (x, y) = at(0.0);
        assert!(x < 0.0 && y > 0.0, "bottom left");
        let (x, y) = at(0.5);
        assert!(x.abs() < 1e-6 && y < 0.0, "straight up");
        let (x, y) = at(1.0);
        assert!(x > 0.0 && y > 0.0, "bottom right");
        assert_eq!(dial_angle(2.0), dial_angle(1.0));
    }

    use crate::meter::disc_color;
    use crate::pen::tests::fonts;
    use crate::{Image, View};

    /// A dial drawn one to one in 80 × 108, and the colour at a part of its outer radius,
    /// straight up from the centre (40, 54).
    fn drawn(d: &Dial) -> Option<(Image, Hits)> {
        let fs = fonts()?;
        let mut img = Image::new(80, 108);
        let mut hits = Hits::new();
        {
            let mut sf = img.surface();
            let mut p = Pen { sf: &mut sf, fonts: &fs, v: View::fit(80, 108, 80, 108) };
            p.dial(&mut hits, Rect::new(0, 0, 80, 108), d, "line1");
        }
        Some((img, hits))
    }

    fn at(img: &Image, part: f32) -> u32 {
        let y = (54.0 - 40.0 * part) as usize;
        img.px[y * img.w + 40]
    }

    #[test]
    fn the_ring_says_the_signal() {
        let ring = RING_OUT - RING_T / 2.0;
        let clip = CLIP_OUT - CLIP_T / 2.0;
        let dial = |r: Option<Disc>| Dial {
            label: "MONITOR L",
            value: "-6.0",
            pos: 0.8,
            ring: r,
            ..Dial::default()
        };
        let Some((img, hits)) = drawn(&dial(None)) else { return };
        assert_eq!(at(&img, ring), 0, "no ring where nothing's metered");
        assert_eq!(hits.at(40.0, 54.0), Some("line1"));
        assert_eq!(hits.iter().next().unwrap().name, "monitor l", "the bar lists it");
        for d in [Disc::Off, Disc::Signal(0.5), Disc::Signal(1.0), Disc::Caution, Disc::Hot] {
            let (img, _) = drawn(&dial(Some(d))).unwrap();
            assert_eq!(at(&img, ring), disc_color(d), "{d:?}");
            assert_eq!(at(&img, clip), 0, "no clip ring for {d:?}");
        }
        let (img, _) = drawn(&dial(Some(Disc::Clip))).unwrap();
        assert_eq!(at(&img, ring), disc_color(Disc::Hot));
        assert_eq!(at(&img, clip), disc_color(Disc::Hot), "the latch's second ring");
    }

    #[test]
    fn the_sweep_lights_to_the_value_and_amber_under_the_hand() {
        let sweep = SWEEP_OUT - SWEEP_T / 2.0;
        let up = |pos: f32, cursor: bool| {
            let (img, _) = drawn(&Dial { label: "A", value: "0", pos, cursor, ..Dial::default() })?;
            Some(at(&img, sweep))
        };
        let Some(c) = up(0.4, false) else { return };
        assert_eq!(c, dark::WELL, "short of the top, the well shows");
        assert_eq!(up(0.6, false), Some(dark::DIM));
        assert_eq!(up(0.6, true), Some(dark::AMBER));
    }

    #[test]
    fn held_by_the_knob_is_dimmed_and_not_pressed() {
        let d = Dial { label: "MONITOR L", value: "-12.0", pos: 0.7, held: true, ..Dial::default() };
        let Some((img, hits)) = drawn(&d) else { return };
        let sweep = SWEEP_OUT - SWEEP_T / 2.0;
        assert_eq!(at(&img, sweep), mix(dark::PANEL, dark::DIM, 0.45));
        assert_eq!(hits.at(40.0, 54.0), None, "a click passes it by");
    }

    #[test]
    fn two_rings_in_and_after_the_knob() {
        let d = Dial {
            label: "PCM 1",
            value: "-6.0",
            pos: 0.7,
            ring: Some(Disc::Hot),
            inner: Some(Disc::Caution),
            ..Dial::default()
        };
        let Some((img, _)) = drawn(&d) else { return };
        assert_eq!(at(&img, TWO_OUT - TWO_T / 2.0), disc_color(Disc::Hot), "what comes in, outside");
        assert_eq!(at(&img, TWO_IN - TWO_T / 2.0), disc_color(Disc::Caution), "after the knob, inside");
        let d = Dial { ring: None, inner: Some(Disc::Off), ..d };
        let (img, _) = drawn(&d).unwrap();
        assert_eq!(at(&img, TWO_OUT - TWO_T / 2.0), 0, "nothing metered coming in");
    }

    #[test]
    fn a_click_on_the_knob_or_the_ring() {
        let r = Rect::new(0, 0, 80, 108);
        let (cx, cy, rad) = circle(r);
        assert_eq!((cx, cy, rad), (40.0, 54.0, 40.0));
        assert_eq!(dial_part(r, cx, cy), Some(DialPart::Knob));
        assert_eq!(dial_part(r, cx + rad * 0.7, cy), Some(DialPart::Knob));
        assert_eq!(dial_part(r, cx + rad * 0.85, cy), Some(DialPart::Ring));
        assert_eq!(dial_part(r, cx, cy - rad * 0.99), Some(DialPart::Ring));
        assert_eq!(dial_part(r, 1.0, 15.0), None, "the corner is neither");
    }
}
