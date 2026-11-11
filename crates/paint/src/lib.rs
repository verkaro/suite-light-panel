//! Drawing a panel into a block of pixels: rectangles, rounded ones, discs with a glow, text in
//! the console's own bitmap fonts, the family's two meters (the LED ladder and the LED disc),
//! and a dial with its signal ring.
//! Pixels are 0x00RRGGBB in rows of `stride`, as a KMS dumb buffer is (`lp-kms`), or an
//! [`Image`] in memory, written out as PNG to look at.
//!
//! A panel is laid out on a logical canvas (Dexter's is 960 × 540) and drawn at the screen's own
//! resolution through a [`View`], so rectangles stay sharp and adjacent ones still meet. A
//! [`Pen`] draws through one.

pub mod dark;
mod dial;
mod font;
mod hits;
mod list;
mod meter;
mod parts;
mod pen;
mod png;
mod wave;

pub use dial::{DIAL_A0, DIAL_SWEEP, Dial, DialPart, KNOB_KNEE_DB, dial_angle, dial_part, knob_db, knob_pos};
pub use font::{Font, Fonts};
pub use hits::{Hit, Hits, bar_name};
pub use list::{Cell, Column, List, ListLook};
pub use meter::{Disc, LedColors, disc_led, ladder};
pub use parts::{Field, FieldState, KeyLook};
pub use pen::{Align, Pen};
pub use png::write_png;
pub use wave::{Marker, Scale, Wave};

/// 0x00RRGGBB.
pub type Rgb = u32;

/// `#rrggbb` (or `rrggbb`) as a colour; black for anything else.
pub const fn hex(s: &str) -> Rgb {
    let b = s.as_bytes();
    let off = if !b.is_empty() && b[0] == b'#' { 1 } else { 0 };
    if b.len() < off + 6 {
        return 0;
    }
    let mut v = 0u32;
    let mut i = 0;
    while i < 6 {
        let c = b[off + i];
        let d = match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            b'A'..=b'F' => c - b'A' + 10,
            _ => 0,
        };
        v = (v << 4) | d as u32;
        i += 1;
    }
    v
}

pub const fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    ((r as u32) << 16) | ((g as u32) << 8) | b as u32
}

/// `a` towards `b` by `t`, 0 to 1.
pub fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    let ch = |s: u32| {
        let x = ((a >> s) & 0xff) as f32;
        let y = ((b >> s) & 0xff) as f32;
        ((x + (y - x) * t).round() as u32) << s
    };
    ch(16) | ch(8) | ch(0)
}

/// In pixels: logical on a [`View`]'s canvas, else the device's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }
    pub fn inset(self, d: i32) -> Rect {
        Rect::new(self.x + d, self.y + d, (self.w - 2 * d).max(0), (self.h - 2 * d).max(0))
    }
    pub fn right(self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(self) -> i32 {
        self.y + self.h
    }
    pub fn centre(self) -> (i32, i32) {
        (self.x + self.w / 2, self.y + self.h / 2)
    }
    /// Whether the point (x, y) is in it: a click on it.
    pub fn holds(self, x: f32, y: f32) -> bool {
        x >= self.x as f32 && y >= self.y as f32 && x < self.right() as f32 && y < self.bottom() as f32
    }
    /// Whether `r` is all inside it.
    pub fn contains(self, r: Rect) -> bool {
        r.x >= self.x && r.y >= self.y && r.right() <= self.right() && r.bottom() <= self.bottom()
    }
}

/// Pixels to draw on, borrowed.
pub struct Surface<'a> {
    pub w: usize,
    pub h: usize,
    pub stride: usize,
    pub px: &'a mut [u32],
}

/// Pixels in memory: for a snapshot, or to draw before copying to the screen.
pub struct Image {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u32>,
}

impl Image {
    pub fn new(w: usize, h: usize) -> Image {
        Image { w, h, px: vec![0; w * h] }
    }
    pub fn surface(&mut self) -> Surface<'_> {
        Surface { w: self.w, h: self.h, stride: self.w, px: &mut self.px }
    }
}

fn blend(d: u32, s: Rgb, a: f32) -> u32 {
    if a >= 1.0 {
        return s;
    }
    if a <= 0.0 {
        return d;
    }
    mix(d, s, a)
}

impl Surface<'_> {
    pub fn clear(&mut self, c: Rgb) {
        for y in 0..self.h {
            self.px[y * self.stride..y * self.stride + self.w].fill(c);
        }
    }

    fn clip(&self, r: Rect) -> Option<(usize, usize, usize, usize)> {
        let x0 = r.x.max(0) as usize;
        let y0 = r.y.max(0) as usize;
        let x1 = (r.right().max(0) as usize).min(self.w);
        let y1 = (r.bottom().max(0) as usize).min(self.h);
        (x0 < x1 && y0 < y1).then_some((x0, y0, x1, y1))
    }

    pub fn fill(&mut self, r: Rect, c: Rgb) {
        if let Some((x0, y0, x1, y1)) = self.clip(r) {
            for y in y0..y1 {
                self.px[y * self.stride + x0..y * self.stride + x1].fill(c);
            }
        }
    }

    /// `c` over what's there, `a` of it (0 to 1).
    pub fn shade(&mut self, r: Rect, c: Rgb, a: f32) {
        if let Some((x0, y0, x1, y1)) = self.clip(r) {
            for y in y0..y1 {
                for p in &mut self.px[y * self.stride + x0..y * self.stride + x1] {
                    *p = blend(*p, c, a);
                }
            }
        }
    }

    /// An outline `t` pixels thick, inside `r`.
    pub fn frame(&mut self, r: Rect, t: i32, c: Rgb) {
        self.fill(Rect::new(r.x, r.y, r.w, t), c);
        self.fill(Rect::new(r.x, r.bottom() - t, r.w, t), c);
        self.fill(Rect::new(r.x, r.y + t, t, r.h - 2 * t), c);
        self.fill(Rect::new(r.right() - t, r.y + t, t, r.h - 2 * t), c);
    }

    pub fn put(&mut self, x: i32, y: i32, c: Rgb, a: f32) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            let p = &mut self.px[y as usize * self.stride + x as usize];
            *p = blend(*p, c, a);
        }
    }

    /// A rectangle with corners of radius `rad`, filled with `fill` inside an edge of `edge`
    /// one `t` thick (`t` 0: no edge). Corners are smoothed.
    pub fn rrect(&mut self, r: Rect, rad: i32, fill: Rgb, edge: Rgb, t: i32) {
        if t > 0 {
            self.rounded(r, rad, edge);
            self.rounded(r.inset(t), rad - t, fill);
        } else {
            self.rounded(r, rad, fill);
        }
    }

    /// A filled rounded rectangle: flat fills, and coverage worked out only in the corners.
    fn rounded(&mut self, r: Rect, rad: i32, c: Rgb) {
        let rad = rad.min(r.w / 2).min(r.h / 2).max(0);
        if rad < 1 {
            self.fill(r, c);
            return;
        }
        self.fill(Rect::new(r.x, r.y + rad, r.w, r.h - 2 * rad), c);
        self.fill(Rect::new(r.x + rad, r.y, r.w - 2 * rad, rad), c);
        self.fill(Rect::new(r.x + rad, r.bottom() - rad, r.w - 2 * rad, rad), c);
        let rf = rad as f32;
        for dy in 0..rad {
            for dx in 0..rad {
                // From the corner's centre, for the top-left corner; mirrored for the others.
                let (qx, qy) = (rf - dx as f32 - 0.5, rf - dy as f32 - 0.5);
                let cover = (rf + 0.5 - (qx * qx + qy * qy).sqrt()).clamp(0.0, 1.0);
                if cover <= 0.0 {
                    continue;
                }
                self.put(r.x + dx, r.y + dy, c, cover);
                self.put(r.right() - 1 - dx, r.y + dy, c, cover);
                self.put(r.x + dx, r.bottom() - 1 - dy, c, cover);
                self.put(r.right() - 1 - dx, r.bottom() - 1 - dy, c, cover);
            }
        }
    }

    /// Copies `src` (the same size or larger) onto this surface: drawing in memory and copying
    /// once is far quicker than blending in a KMS buffer, which is slow to read.
    pub fn copy_from(&mut self, src: &Surface) {
        let w = self.w.min(src.w);
        for y in 0..self.h.min(src.h) {
            self.px[y * self.stride..y * self.stride + w]
                .copy_from_slice(&src.px[y * src.stride..y * src.stride + w]);
        }
    }

    /// A filled disc, its edge smoothed.
    pub fn disc(&mut self, cx: f32, cy: f32, r: f32, c: Rgb) {
        self.ring(cx, cy, r, r, c);
    }

    /// A ring from radius `r - t` to `r` (`t` ≥ `r`: a disc).
    pub fn ring(&mut self, cx: f32, cy: f32, r: f32, t: f32, c: Rgb) {
        let (x0, x1) = ((cx - r - 1.0).floor() as i32, (cx + r + 1.0).ceil() as i32);
        let (y0, y1) = ((cy - r - 1.0).floor() as i32, (cy + r + 1.0).ceil() as i32);
        for y in y0..y1 {
            for x in x0..x1 {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                let outer = (r + 0.5 - d).clamp(0.0, 1.0);
                let inner = if t >= r { 1.0 } else { (d - (r - t) + 0.5).clamp(0.0, 1.0) };
                let a = outer.min(inner);
                if a > 0.0 {
                    self.put(x, y, c, a);
                }
            }
        }
    }

    /// Part of a ring (as [`Surface::ring`]): `sweep` from one angle to another, radians,
    /// clockwise from three o'clock (y runs down), as a dial's.
    pub fn arc(&mut self, cx: f32, cy: f32, r: f32, t: f32, sweep: (f32, f32), c: Rgb) {
        let (a0, a1) = sweep;
        use std::f32::consts::TAU;
        let (x0, x1) = ((cx - r - 1.0).floor() as i32, (cx + r + 1.0).ceil() as i32);
        let (y0, y1) = ((cy - r - 1.0).floor() as i32, (cy + r + 1.0).ceil() as i32);
        let span = a1 - a0;
        for y in y0..y1 {
            for x in x0..x1 {
                let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                if (dy.atan2(dx) - a0).rem_euclid(TAU) > span {
                    continue;
                }
                let d = (dx * dx + dy * dy).sqrt();
                let a = (r + 0.5 - d).clamp(0.0, 1.0).min((d - (r - t) + 0.5).clamp(0.0, 1.0));
                if a > 0.0 {
                    self.put(x, y, c, a);
                }
            }
        }
    }

    /// A line `w` wide from (x0, y0) to (x1, y1), round-ended.
    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, w: f32, c: Rgb) {
        let n = ((x1 - x0).hypot(y1 - y0) * 2.0).ceil().max(1.0) as usize;
        let mut done = std::collections::HashSet::new();
        for i in 0..=n {
            let t = i as f32 / n as f32;
            let (x, y) = (x0 + (x1 - x0) * t, y0 + (y1 - y0) * t);
            let r = w / 2.0;
            for py in (y - r).floor() as i32..=(y + r).ceil() as i32 {
                for px in (x - r).floor() as i32..=(x + r).ceil() as i32 {
                    let d = (px as f32 + 0.5 - x).hypot(py as f32 + 0.5 - y);
                    if d <= r + 0.25 && done.insert((px, py)) {
                        self.put(px, py, c, 1.0);
                    }
                }
            }
        }
    }

    /// Light spilling round a disc of radius `r`: `reach` pixels beyond it, fading out, `a` at
    /// its brightest.
    pub fn glow(&mut self, cx: f32, cy: f32, r: f32, reach: f32, c: Rgb, a: f32) {
        if reach <= 0.0 {
            return;
        }
        let rr = r + reach;
        let (x0, x1) = ((cx - rr).floor() as i32, (cx + rr).ceil() as i32);
        let (y0, y1) = ((cy - rr).floor() as i32, (cy + rr).ceil() as i32);
        for y in y0..y1 {
            for x in x0..x1 {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                if d < r || d > rr {
                    continue;
                }
                let k = 1.0 - (d - r) / reach;
                self.put(x, y, c, a * k * k);
            }
        }
    }

    /// The same as [`Surface::glow`] round a rectangle.
    pub fn glow_rect(&mut self, r: Rect, reach: i32, c: Rgb, a: f32) {
        if reach <= 0 {
            return;
        }
        for y in r.y - reach..r.bottom() + reach {
            for x in r.x - reach..r.right() + reach {
                let dx = (r.x - x).max(x - (r.right() - 1)).max(0);
                let dy = (r.y - y).max(y - (r.bottom() - 1)).max(0);
                if dx == 0 && dy == 0 {
                    continue;
                }
                let d = ((dx * dx + dy * dy) as f32).sqrt();
                if d > reach as f32 {
                    continue;
                }
                let k = 1.0 - d / reach as f32;
                self.put(x, y, c, a * k * k);
            }
        }
    }
}

/// The pointer's arrow: `X` its outline, `.` its fill, the tip at the top left.
const ARROW: [&str; 17] = [
    "X",
    "XX",
    "X.X",
    "X..X",
    "X...X",
    "X....X",
    "X.....X",
    "X......X",
    "X.......X",
    "X........X",
    "X.....XXXXX",
    "X..X..X",
    "X.X X..X",
    "XX  X..X",
    "X    X..X",
    "     X..X",
    "      XX",
];

/// Draws the pointer with its tip at (x, y), each of its pixels `scale` square (2 on a big
/// screen, say): light with a dark edge, to show on anything.
pub fn cursor(s: &mut Surface, x: i32, y: i32, scale: i32) {
    let k = scale.max(1);
    for (row, line) in ARROW.iter().enumerate() {
        for (col, ch) in line.chars().enumerate() {
            let c = match ch {
                'X' => 0x101010,
                '.' => 0xf4f0e6,
                _ => continue,
            };
            s.fill(Rect::new(x + col as i32 * k, y + row as i32 * k, k, k), c);
        }
    }
}

/// A logical canvas fitted to the device's pixels: scaled to fit, centred, the rest left as a
/// border.
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub scale: f32,
    pub ox: f32,
    pub oy: f32,
    pub lw: i32,
    pub lh: i32,
}

impl View {
    pub fn fit(lw: i32, lh: i32, dw: usize, dh: usize) -> View {
        let scale = (dw as f32 / lw as f32).min(dh as f32 / lh as f32);
        let ox = ((dw as f32 - lw as f32 * scale) / 2.0).floor();
        let oy = ((dh as f32 - lh as f32 * scale) / 2.0).floor();
        View { scale, ox, oy, lw, lh }
    }
    pub fn x(&self, x: f32) -> i32 {
        (self.ox + x * self.scale).round() as i32
    }
    pub fn y(&self, y: f32) -> i32 {
        (self.oy + y * self.scale).round() as i32
    }
    /// A logical length in device pixels, at least 1.
    pub fn len(&self, l: f32) -> i32 {
        ((l * self.scale).round() as i32).max(1)
    }
    /// A logical rectangle on the device: each edge mapped, so neighbours still meet.
    pub fn rect(&self, r: Rect) -> Rect {
        let x0 = self.x(r.x as f32);
        let y0 = self.y(r.y as f32);
        Rect::new(x0, y0, self.x(r.right() as f32) - x0, self.y(r.bottom() as f32) - y0)
    }
    /// A device pixel back on the logical canvas: where a click landed.
    pub fn logical(&self, x: i32, y: i32) -> (f32, f32) {
        ((x as f32 - self.ox) / self.scale, (y as f32 - self.oy) / self.scale)
    }
    /// The whole canvas on the device.
    pub fn area(&self) -> Rect {
        self.rect(Rect::new(0, 0, self.lw, self.lh))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours() {
        assert_eq!(hex("#56d364"), 0x56d364);
        assert_eq!(hex("FFB020"), 0xffb020);
        assert_eq!(mix(0x000000, 0xffffff, 0.5), 0x808080);
    }

    #[test]
    fn view_keeps_neighbours_meeting() {
        let v = View::fit(960, 540, 1366, 768);
        let a = v.rect(Rect::new(10, 10, 33, 20));
        let b = v.rect(Rect::new(43, 10, 33, 20));
        assert_eq!(a.right(), b.x);
    }

    #[test]
    fn fill_clips() {
        let mut img = Image::new(4, 4);
        let mut s = img.surface();
        s.fill(Rect::new(-2, -2, 4, 4), 0xff0000);
        assert_eq!(img.px[0], 0xff0000);
        assert_eq!(img.px[1], 0xff0000);
        assert_eq!(img.px[2], 0);
    }
}
