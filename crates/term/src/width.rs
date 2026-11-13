//! How many cells a character takes: 0 (a mark, joined to the cell before), 1 or 2. From
//! `width_table.rs`, which `width.sh` writes from glibc's charmap, so the grid agrees with the
//! programs drawing into it, which count through `wcwidth`.

use crate::width_table::WIDTHS;

/// The cells `c` takes. Controls aren't asked about: the parser never prints one.
pub fn width(c: char) -> usize {
    let c = c as u32;
    if c < 0x300 {
        return 1;
    }
    match WIDTHS.binary_search_by(|&(lo, hi, _)| {
        if hi < c {
            std::cmp::Ordering::Less
        } else if lo > c {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    }) {
        Ok(i) => usize::from(WIDTHS[i].2),
        Err(_) => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_sorted_and_apart() {
        assert!(WIDTHS[0].0 >= 0x300, "the fast path skips below U+0300");
        for w in WIDTHS.windows(2) {
            assert!(w[0].0 <= w[0].1 && w[0].1 < w[1].0, "{w:x?}");
        }
    }

    #[test]
    fn some_widths() {
        for (c, w) in [
            ('a', 1),
            ('é', 1),
            ('\u{AD}', 1),
            ('\u{301}', 0),
            ('\u{200B}', 0),
            ('\u{FE0F}', 0),
            ('─', 1),
            ('€', 1),
            ('中', 2),
            ('가', 2),
            ('\u{1160}', 0),
            ('Ａ', 2),
            ('😀', 2),
            ('\u{10FFFF}', 1),
        ] {
            assert_eq!(width(c), w, "{c:?}");
        }
    }
}
