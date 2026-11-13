//! A pen for drawing a panel laid out on a logical canvas: rectangles and text placed in
//! logical units and drawn at the screen's own resolution through a [`View`], so a panel's code
//! reads as the page's does (`fillRect`, `fillText` in a box).
//!
//! Moved here from dexter-kms's `draw.rs` (2026-11-07), for Tape's panel and the ones after.

use crate::{Fonts, Rect, Rgb, Surface, View, skin};

/// Where text sits across its box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Centre,
    Right,
}

/// A surface, the fonts, and the view from the logical canvas to the surface.
pub struct Pen<'a, 'b> {
    pub sf: &'a mut Surface<'b>,
    pub fonts: &'a Fonts,
    pub v: View,
}

impl Pen<'_, '_> {
    pub fn fill(&mut self, r: Rect, c: Rgb) {
        let d = self.v.rect(r);
        self.sf.fill(d, c);
    }

    /// A rounded rectangle, its corners `rad` and its edge a logical pixel wide.
    pub fn rrect(&mut self, r: Rect, rad: f32, fill: Rgb, edge: Rgb) {
        let d = self.v.rect(r);
        let t = self.v.len(1.0);
        self.sf.rrect(d, self.v.len(rad), fill, edge, t);
    }

    /// `s` in `r`, `size` logical pixels tall, centred up and down. Too wide, it's set a size
    /// smaller at a time, down to two thirds, then cut short with ….
    pub fn text(&mut self, r: Rect, s: &str, size: f32, bold: bool, c: Rgb, a: Align) {
        let d = self.v.rect(r);
        let mut px = size * self.v.scale;
        let (mut f, mut k) = self.fonts.pick(px, bold);
        while f.measure(s, k) > d.w && px > size * self.v.scale * 0.67 {
            px -= 1.0;
            (f, k) = self.fonts.pick(px, bold);
        }
        let mut s: String = s.to_string();
        if f.measure(&s, k) > d.w {
            let mut cs: Vec<char> = s.chars().collect();
            while !cs.is_empty() && (cs.len() + 1) as i32 * (f.width * k) as i32 > d.w {
                cs.pop();
            }
            s = cs.into_iter().chain(std::iter::once('…')).collect();
        }
        let w = f.measure(&s, k);
        let x = match a {
            Align::Left => d.x,
            Align::Centre => d.x + (d.w - w) / 2,
            Align::Right => d.right() - w,
        };
        let y = d.y + (d.h - (f.height * k) as i32) / 2;
        f.draw(self.sf, x, y, &s, c, k);
    }

    /// How wide `s` is at `size`, in logical pixels, set as it would be with room to spare.
    pub fn width(&self, s: &str, size: f32, bold: bool) -> f32 {
        let (f, k) = self.fonts.pick(size * self.v.scale, bold);
        f.measure(s, k) as f32 / self.v.scale
    }

    /// `s` from the logical `x`, centred up and down in `r`, never shrunk or cut: for a line
    /// built a piece at a time, each from where the last ended. The logical x after it.
    pub fn run(&mut self, x: f32, r: Rect, s: &str, size: f32, bold: bool, c: Rgb) -> f32 {
        let d = self.v.rect(r);
        let (f, k) = self.fonts.pick(size * self.v.scale, bold);
        let y = d.y + (d.h - (f.height * k) as i32) / 2;
        let end = f.draw(self.sf, self.v.x(x), y, s, c, k);
        self.v.logical(end, 0).0
    }

    /// `s` running up `r` from its bottom, as a mixer's names run beside its meters; the font
    /// as tall as `r` is wide, cut short with … to fit.
    pub fn text_up(&mut self, r: Rect, s: &str, c: Rgb) {
        let d = self.v.rect(r);
        let (f, _) = self.fonts.pick(d.w as f32, false);
        let fit = (d.h as usize / f.width).max(1);
        let s: String = if s.chars().count() > fit {
            s.chars().take(fit - 1).chain(std::iter::once('…')).collect()
        } else {
            s.to_string()
        };
        let x = d.x + (d.w - f.height as i32) / 2;
        f.draw_up(self.sf, x, d.bottom(), &s, c);
    }

    /// The first of `cs` the font has every character of (the last if none): a symbol where
    /// Terminus has it, a plainer one where it hasn't.
    pub fn first_of(&self, cs: &[&'static str]) -> &'static str {
        let (f, _) = self.fonts.pick(11.0 * self.v.scale, true);
        cs.iter().copied().find(|c| c.chars().all(|ch| f.has(ch))).unwrap_or(cs[cs.len() - 1])
    }

    /// A section's label at the left of `r`, faint, and a rule across the rest, in the dark
    /// skin (the page's `drawRule`).
    pub fn rule(&mut self, r: Rect, label: &str) {
        let (f, k) = self.fonts.pick(9.0 * self.v.scale, true);
        let d = self.v.rect(r);
        let y = d.y + (d.h - (f.height * k) as i32) / 2;
        let end = f.draw(self.sf, d.x, y, label, skin().faint, k);
        let gap = self.v.len(6.0);
        let line = Rect::new(end + gap, d.y + d.h / 2, d.right() - end - gap, self.v.len(1.0));
        self.sf.fill(line, skin().rule);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::Image;
    use crate::dark;
    use crate::font::CONSOLE_FONTS;
    use std::path::Path;

    /// Whether any pixel in `r` is exactly `c`, on an image drawn one to one.
    pub(crate) fn has(img: &Image, r: Rect, c: Rgb) -> bool {
        (r.y..r.bottom()).any(|y| (r.x..r.right()).any(|x| img.px[y as usize * img.w + x as usize] == c))
    }

    pub(crate) fn fonts() -> Option<Fonts> {
        Path::new(CONSOLE_FONTS)
            .join("Uni2-Terminus16.psf.gz")
            .exists()
            .then(|| Fonts::terminus(None).unwrap())
    }

    /// The columns with ink in them, first and last.
    fn ink_x(img: &Image) -> Option<(usize, usize)> {
        let xs: Vec<usize> = (0..img.w).filter(|&x| (0..img.h).any(|y| img.px[y * img.w + x] != 0)).collect();
        Some((*xs.first()?, *xs.last()?))
    }

    #[test]
    fn text_sits_where_its_aligned_and_is_cut_to_fit() {
        let Some(fs) = fonts() else { return };
        let draw = |s: &str, a: Align| {
            let mut img = Image::new(200, 40);
            let mut sf = img.surface();
            let mut p = Pen { sf: &mut sf, fonts: &fs, v: View::fit(200, 40, 200, 40) };
            p.text(Rect::new(0, 0, 200, 40), s, 16.0, false, 0xffffff, a);
            img
        };
        let (l0, _) = ink_x(&draw("MM", Align::Left)).unwrap();
        let (_, r1) = ink_x(&draw("MM", Align::Right)).unwrap();
        let (c0, c1) = ink_x(&draw("MM", Align::Centre)).unwrap();
        assert!(l0 < 3, "{l0}");
        assert!(r1 > 196, "{r1}");
        assert!(((c0 + c1) as i32 - 200).abs() <= 3, "{c0} {c1}");
        // Far too long: cut short, never past the box.
        let long = draw(&"W".repeat(80), Align::Left);
        let (_, end) = ink_x(&long).unwrap();
        assert!(end < 200);
    }

    #[test]
    fn a_rule_runs_to_the_boxs_end() {
        let Some(fs) = fonts() else { return };
        let mut img = Image::new(300, 20);
        let mut sf = img.surface();
        let mut p = Pen { sf: &mut sf, fonts: &fs, v: View::fit(300, 20, 300, 20) };
        p.rule(Rect::new(0, 0, 300, 20), "TRACKS");
        assert_eq!(img.px[10 * 300 + 299], dark::RULE);
        assert!(img.px.contains(&dark::FAINT));
    }

    #[test]
    fn first_of_falls_back() {
        let Some(fs) = fonts() else { return };
        let mut img = Image::new(10, 10);
        let mut sf = img.surface();
        let p = Pen { sf: &mut sf, fonts: &fs, v: View::fit(10, 10, 10, 10) };
        assert_eq!(p.first_of(&["\u{e000}", ">"]), ">");
        assert_eq!(p.first_of(&["A", ">"]), "A");
    }

    #[test]
    fn a_run_carries_on_from_where_the_last_ended() {
        let Some(fs) = fonts() else { return };
        let mut img = Image::new(400, 40);
        let mut sf = img.surface();
        let mut p = Pen { sf: &mut sf, fonts: &fs, v: View::fit(200, 20, 400, 40) };
        let all = Rect::new(0, 0, 200, 20);
        let w = p.width("take ", 12.0, false);
        let x = p.run(10.0, all, "take ", 12.0, false, 0xffffff);
        assert!((x - 10.0 - w).abs() < 0.01, "{x} {w}");
        let end = p.run(x, all, "003", 12.0, false, 0xff0000);
        assert!((end - x - p.width("003", 12.0, false)).abs() < 0.01);
        let (first, last) = ink_x(&img).unwrap();
        assert!(first >= 20 && last < (end * 2.0) as usize, "{first} {last}");
    }
}
