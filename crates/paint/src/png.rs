//! A surface written out as an RGB PNG: to look at what a panel drew, without a screen.

use crate::Surface;
use std::io::{self, Write};
use std::path::Path;

fn crc32(chunks: &[&[u8]]) -> u32 {
    let mut table = [0u32; 256];
    for (n, t) in table.iter_mut().enumerate() {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB88320 ^ (c >> 1) } else { c >> 1 };
        }
        *t = c;
    }
    let mut c = !0u32;
    for b in chunks.iter().flat_map(|c| c.iter()) {
        c = table[((c ^ *b as u32) & 0xff) as usize] ^ (c >> 8);
    }
    !c
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend((data.len() as u32).to_be_bytes());
    out.extend(kind);
    out.extend(data);
    out.extend(crc32(&[kind, data]).to_be_bytes());
}

pub fn write_png(path: &Path, s: &Surface) -> io::Result<()> {
    let mut raw = Vec::with_capacity((s.w * 3 + 1) * s.h);
    for y in 0..s.h {
        raw.push(0);
        for &p in &s.px[y * s.stride..y * s.stride + s.w] {
            raw.extend([(p >> 16) as u8, (p >> 8) as u8, p as u8]);
        }
    }
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut ihdr = Vec::new();
    ihdr.extend((s.w as u32).to_be_bytes());
    ihdr.extend((s.h as u32).to_be_bytes());
    ihdr.extend([8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6));
    chunk(&mut out, b"IEND", &[]);
    std::fs::File::create(path)?.write_all(&out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn crc_of_iend() {
        assert_eq!(super::crc32(&[b"IEND"]), 0xAE426082);
    }
}
