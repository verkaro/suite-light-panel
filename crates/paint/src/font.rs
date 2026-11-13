//! The Linux console's bitmap fonts (PC Screen Font, PSF1 and PSF2, gzipped or not), as Debian's
//! `console-setup` installs them under `/usr/share/consolefonts`. Read at run time, so none is
//! carried here. Terminus is the one to want: sharp at every size from 12 to 32 pixels.

use crate::{Rgb, Surface};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

pub struct Font {
    pub width: usize,
    pub height: usize,
    row_bytes: usize,
    glyphs: Vec<u8>,
    map: HashMap<char, usize>,
    count: usize,
}

fn gunzip(b: &[u8]) -> io::Result<Vec<u8>> {
    if b.len() < 18 || b[0] != 0x1f || b[1] != 0x8b || b[2] != 8 {
        return Err(io::Error::other("not gzip"));
    }
    let flags = b[3];
    let mut at = 10;
    if flags & 4 != 0 {
        let n = u16::from_le_bytes([b[at], b[at + 1]]) as usize;
        at += 2 + n;
    }
    for bit in [8u8, 16] {
        if flags & bit != 0 {
            while at < b.len() && b[at] != 0 {
                at += 1;
            }
            at += 1;
        }
    }
    if flags & 2 != 0 {
        at += 2;
    }
    let body = b.get(at..).ok_or_else(|| io::Error::other("gzip cut short"))?;
    miniz_oxide::inflate::decompress_to_vec(body).map_err(|e| io::Error::other(format!("gzip: {e:?}")))
}

impl Font {
    pub fn load(path: &Path) -> io::Result<Font> {
        let raw = std::fs::read(path)?;
        let data = if raw.starts_with(&[0x1f, 0x8b]) { gunzip(&raw)? } else { raw };
        Font::parse(&data).map_err(|e| io::Error::other(format!("{}: {e}", path.display())))
    }

    pub fn parse(d: &[u8]) -> Result<Font, String> {
        let u32at = |i: usize| u32::from_le_bytes(d[i..i + 4].try_into().unwrap()) as usize;
        if d.len() >= 32 && d[..4] == [0x72, 0xb5, 0x4a, 0x86] {
            let (hdr, flags, count, size, height, width) =
                (u32at(8), u32at(12), u32at(16), u32at(20), u32at(24), u32at(28));
            let end = hdr + count * size;
            let glyphs = d.get(hdr..end).ok_or("cut short")?.to_vec();
            let mut map = HashMap::new();
            if flags & 1 != 0 {
                // Each glyph's characters in UTF-8, then 0xFE and its sequences, ended by 0xFF.
                let mut g = 0;
                let mut at = end;
                let mut seq = false;
                let mut start = at;
                while at < d.len() && g < count {
                    match d[at] {
                        0xFF => {
                            if !seq {
                                add_utf8(&d[start..at], g, &mut map);
                            }
                            g += 1;
                            seq = false;
                            at += 1;
                            start = at;
                        }
                        0xFE => {
                            if !seq {
                                add_utf8(&d[start..at], g, &mut map);
                            }
                            seq = true;
                            at += 1;
                        }
                        _ => at += 1,
                    }
                }
            } else {
                for g in 0..count.min(256) {
                    map.insert(char::from(g as u8), g);
                }
            }
            Ok(Font { width, height, row_bytes: width.div_ceil(8), glyphs, map, count })
        } else if d.len() >= 4 && d[..2] == [0x36, 0x04] {
            let mode = d[2];
            let height = d[3] as usize;
            let count = if mode & 1 != 0 { 512 } else { 256 };
            let end = 4 + count * height;
            let glyphs = d.get(4..end).ok_or("cut short")?.to_vec();
            let mut map = HashMap::new();
            if mode & 2 != 0 {
                let mut g = 0;
                let mut at = end;
                let mut seq = false;
                while at + 1 < d.len() && g < count {
                    let u = u16::from_le_bytes([d[at], d[at + 1]]);
                    at += 2;
                    match u {
                        0xFFFF => {
                            g += 1;
                            seq = false;
                        }
                        0xFFFE => seq = true,
                        _ if !seq => {
                            if let Some(c) = char::from_u32(u as u32) {
                                map.entry(c).or_insert(g);
                            }
                        }
                        _ => {}
                    }
                }
            } else {
                for g in 0..256 {
                    map.insert(char::from(g as u8), g);
                }
            }
            Ok(Font { width: 8, height, row_bytes: 1, glyphs, map, count })
        } else {
            Err("not a PSF font".into())
        }
    }

    fn glyph(&self, c: char) -> Option<&[u8]> {
        let g = self.map.get(&c).or_else(|| self.map.get(&'?')).copied()?;
        if g >= self.count {
            return None;
        }
        let size = self.row_bytes * self.height;
        self.glyphs.get(g * size..(g + 1) * size)
    }

    pub fn has(&self, c: char) -> bool {
        self.map.contains_key(&c)
    }

    /// Width in pixels of `s` at `scale` (each glyph pixel `scale` square).
    pub fn measure(&self, s: &str, scale: usize) -> i32 {
        (s.chars().count() * self.width * scale) as i32
    }

    /// Draws `s` with its top left at (x, y); returns the x after it.
    pub fn draw(&self, sf: &mut Surface, x: i32, y: i32, s: &str, c: Rgb, scale: usize) -> i32 {
        let mut cx = x;
        for ch in s.chars() {
            if let Some(g) = self.glyph(ch) {
                for row in 0..self.height {
                    for col in 0..self.width {
                        if g[row * self.row_bytes + col / 8] & (0x80 >> (col % 8)) != 0 {
                            let (px, py) = (cx + (col * scale) as i32, y + (row * scale) as i32);
                            for dy in 0..scale as i32 {
                                for dx in 0..scale as i32 {
                                    sf.put(px + dx, py + dy, c, 1.0);
                                }
                            }
                        }
                    }
                }
            }
            cx += (self.width * scale) as i32;
        }
        cx
    }

    /// Draws `s` running up the screen (turned a quarter left), its baseline on the right:
    /// (x, y) is where the first character's bottom left lands.
    pub fn draw_up(&self, sf: &mut Surface, x: i32, y: i32, s: &str, c: Rgb) {
        let mut cy = y;
        for ch in s.chars() {
            if let Some(g) = self.glyph(ch) {
                for row in 0..self.height {
                    for col in 0..self.width {
                        if g[row * self.row_bytes + col / 8] & (0x80 >> (col % 8)) != 0 {
                            sf.put(x + row as i32, cy - col as i32 - 1, c, 1.0);
                        }
                    }
                }
            }
            cy -= self.width as i32;
        }
    }
}

fn add_utf8(b: &[u8], g: usize, map: &mut HashMap<char, usize>) {
    if let Ok(s) = std::str::from_utf8(b) {
        for c in s.chars() {
            map.entry(c).or_insert(g);
        }
    }
}

/// A family of one font in several sizes, regular and bold, to pick from by pixel height.
pub struct Fonts {
    regular: Vec<Font>,
    bold: Vec<Font>,
}

/// Where Debian keeps the console's fonts.
pub const CONSOLE_FONTS: &str = "/usr/share/consolefonts";

impl Fonts {
    /// Terminus from `dir` (default [`CONSOLE_FONTS`]): every `Uni2-Terminus*` and
    /// `Uni2-TerminusBold*` there, the VGA-shaped variants left out.
    pub fn terminus(dir: Option<&Path>) -> io::Result<Fonts> {
        let dir = dir.map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from(CONSOLE_FONTS));
        let mut regular = Vec::new();
        let mut bold = Vec::new();
        for e in std::fs::read_dir(&dir)? {
            let p = e?.path();
            let Some(n) = p.file_name().and_then(|n| n.to_str()) else { continue };
            if !n.starts_with("Uni2-Terminus") || n.contains("VGA") {
                continue;
            }
            let f = Font::load(&p)?;
            if n.starts_with("Uni2-TerminusBold") { bold.push(f) } else { regular.push(f) }
        }
        if regular.is_empty() && bold.is_empty() {
            return Err(io::Error::other(format!(
                "no Terminus fonts in {} (Debian: apt install console-setup-linux)",
                dir.display()
            )));
        }
        regular.sort_by_key(|f| f.height);
        bold.sort_by_key(|f| f.height);
        Ok(Fonts { regular, bold })
    }

    /// For a terminal's grid on a screen of `w` × `h` pixels: the largest regular Terminus that
    /// still gives at least `cols` × `rows` cells (else the smallest there is), with its bold of
    /// the same size where there is one. Drawn at its own pixels, never scaled.
    pub fn for_grid(&self, w: usize, h: usize, cols: usize, rows: usize) -> (&Font, Option<&Font>) {
        let set = if self.regular.is_empty() { &self.bold } else { &self.regular };
        let f = set.iter().rev().find(|f| w / f.width >= cols && h / f.height >= rows).unwrap_or(&set[0]);
        (f, self.bold_of(f))
    }

    /// The regular Terminus `height` pixels tall (`--font 24`), with its bold, if there is one.
    pub fn sized(&self, height: usize) -> Option<(&Font, Option<&Font>)> {
        let set = if self.regular.is_empty() { &self.bold } else { &self.regular };
        set.iter().find(|f| f.height == height).map(|f| (f, self.bold_of(f)))
    }

    fn bold_of(&self, f: &Font) -> Option<&Font> {
        self.bold.iter().find(|b| (b.width, b.height) == (f.width, f.height))
    }

    /// The largest font no taller than `px` (or the smallest there is), and the whole-number
    /// scale that brings it nearest to `px` past the largest size.
    pub fn pick(&self, px: f32, bold: bool) -> (&Font, usize) {
        let set = if bold && !self.bold.is_empty() { &self.bold } else { &self.regular };
        let set = if set.is_empty() { &self.bold } else { set };
        let largest = set.last().unwrap();
        if px > largest.height as f32 * 1.5 {
            let scale = (px / largest.height as f32).round().max(1.0) as usize;
            return (largest, scale);
        }
        let f = set.iter().rev().find(|f| f.height as f32 <= px + 0.5).unwrap_or(&set[0]);
        (f, 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn psf2_with_unicode_table() {
        // Two glyphs of 8 × 2: glyph 0 for 'A', glyph 1 for 'B' and 'b'.
        let mut d = vec![0x72, 0xb5, 0x4a, 0x86];
        for v in [0u32, 32, 1, 2, 2, 2, 8] {
            d.extend(v.to_le_bytes());
        }
        d.extend([0xff, 0x81, 0x18, 0x18]);
        d.extend(b"A\xffBb\xff");
        let f = Font::parse(&d).unwrap();
        assert_eq!((f.width, f.height), (8, 2));
        assert_eq!(f.glyph('A'), Some(&[0xff, 0x81][..]));
        assert_eq!(f.glyph('b'), Some(&[0x18, 0x18][..]));
    }

    #[test]
    fn the_systems_terminus_loads_if_installed() {
        if !Path::new(CONSOLE_FONTS).join("Uni2-Terminus16.psf.gz").exists() {
            return;
        }
        let fs = Fonts::terminus(None).unwrap();
        let (f, s) = fs.pick(16.0, false);
        assert_eq!((f.height, s), (16, 1));
        assert!(f.has('—') && f.has('·') && f.has('A'));
        let (f, s) = fs.pick(64.0, true);
        assert_eq!((f.height, s), (32, 2));
        let (f, b) = fs.for_grid(1920, 1080, 80, 24);
        assert_eq!((f.width, f.height), (16, 32), "120 × 33 on 1920 × 1080");
        assert_eq!(b.map(|b| (b.width, b.height)), Some((16, 32)));
        let (f, _) = fs.for_grid(800, 480, 80, 24);
        assert_eq!((f.width, f.height), (10, 20), "80 × 24 on 800 × 480");
        assert_eq!(fs.sized(24).map(|(f, _)| f.width), Some(12));
        assert!(fs.sized(25).is_none());
    }
}
