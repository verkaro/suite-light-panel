//! The family's dark skin, as `as-page-kit`'s `page-kit.js` has it (`SKINS.dark`), so a panel
//! on the console and a page in the browser are the same colours. Named as the page names
//! them: `btnEdge` is [`BTN_EDGE`]. The meters' LEDs are [`LedColors::dexter_dark`].
//!
//! Moved here from dexter-kms's `draw.rs` (2026-11-07), for Tape's panel and the ones after.
//! The page's colours with transparency (`glow`, `shade`, `shadow`) aren't here: a panel shades
//! with [`Surface::shade`]'s own amount.
//!
//! [`LedColors::dexter_dark`]: crate::LedColors::dexter_dark
//! [`Surface::shade`]: crate::Surface::shade

use crate::{Rgb, hex};

/// Behind everything, outside the canvas.
pub const PAGE: Rgb = hex("#07060a");
pub const BG: Rgb = hex("#0f0e0b");
pub const PANEL: Rgb = hex("#1b1914");
pub const COLUMN: Rgb = hex("#1b1914");
pub const RULE: Rgb = hex("#37322a");
pub const BTN: Rgb = hex("#2a261f");
pub const BTN_HI: Rgb = hex("#3a352c");
pub const BTN_EDGE: Rgb = hex("#4a443a");
pub const TEXT: Rgb = hex("#e9e1cc");
pub const DIM: Rgb = hex("#9a917c");
pub const FAINT: Rgb = hex("#5d564a");
pub const GREEN: Rgb = hex("#56d364");
pub const AMBER: Rgb = hex("#ffb020");
pub const RED: Rgb = hex("#ff4b3e");
pub const LAMP_OFF: Rgb = hex("#3a342a");
/// Behind a readout.
pub const WELL: Rgb = hex("#0a0907");
pub const READOUT: Rgb = hex("#ffb020");
/// A readout's unlit segments.
pub const GHOST: Rgb = hex("#2a2314");
/// Text on a lit red key.
pub const ON_RED: Rgb = hex("#1a0806");
pub const ARM_TEXT: Rgb = hex("#ff8a7f");
pub const ARM_IDLE: Rgb = hex("#8f8672");
pub const TAPE: Rgb = hex("#5aa9e6");
pub const OVERLAY: Rgb = hex("#0a0907");
pub const MENU: Rgb = hex("#24211b");

/// Every colour here by the page's name for it.
pub const ALL: [(&str, Rgb); 24] = [
    ("page", PAGE),
    ("bg", BG),
    ("panel", PANEL),
    ("column", COLUMN),
    ("rule", RULE),
    ("btn", BTN),
    ("btnHi", BTN_HI),
    ("btnEdge", BTN_EDGE),
    ("text", TEXT),
    ("dim", DIM),
    ("faint", FAINT),
    ("green", GREEN),
    ("amber", AMBER),
    ("red", RED),
    ("lampOff", LAMP_OFF),
    ("well", WELL),
    ("readout", READOUT),
    ("ghost", GHOST),
    ("onRed", ON_RED),
    ("armText", ARM_TEXT),
    ("armIdle", ARM_IDLE),
    ("tape", TAPE),
    ("overlay", OVERLAY),
    ("menu", MENU),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The dark skin's `key: "#rrggbb"` pairs from page-kit.js.
    fn page_kit_dark(js: &str) -> Vec<(String, String)> {
        let at = js.find("  dark: {").expect("SKINS.dark") + "  dark: {".len();
        let body = &js[at..at + js[at..].find("\n  },").expect("its end")];
        let mut out = Vec::new();
        for part in body.split(',') {
            let Some((k, v)) = part.split_once(':') else { continue };
            let (k, v) = (k.trim(), v.trim().trim_matches('"'));
            if v.len() == 7 && v.starts_with('#') {
                out.push((k.to_string(), v.to_string()));
            }
        }
        out
    }

    #[test]
    fn the_same_as_the_pages_dark_skin() {
        let js =
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../../suite-audio-libs/crates/page-kit/src/page-kit.js");
        let Ok(js) = std::fs::read_to_string(js) else {
            eprintln!("no suite-audio-libs beside this suite: page-kit.js not compared");
            return;
        };
        let page = page_kit_dark(&js);
        // Every solid colour of the page's is here, the same.
        for (k, v) in &page {
            let ours = ALL.iter().find(|(n, _)| n == k).unwrap_or_else(|| panic!("{k} isn't here"));
            assert_eq!(ours.1, hex(v), "{k}");
        }
        assert_eq!(page.len(), ALL.len());
    }

    #[test]
    fn named_once_each() {
        let mut names: Vec<_> = ALL.iter().map(|(n, _)| n).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), ALL.len());
        assert_eq!(TEXT, 0xe9e1cc);
    }
}
