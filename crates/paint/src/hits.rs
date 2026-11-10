//! Where a click can land: what a panel drew this frame, by id, the last added on top, as the
//! page kit's `HITS` are. A panel clears it before each draw and asks it where a click fell.
//!
//! A key drawn with [`Pen::key`] also says what the command bar calls it and the section it's
//! in, so the bar's levels come from the screen (`../STYLE.md`, "The command bar").
//!
//! [`Pen::key`]: crate::Pen::key

use crate::Rect;

/// One place a click can land.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    /// On the logical canvas.
    pub r: Rect,
    pub id: String,
    /// What the bar calls it. Empty: it isn't in the bar.
    pub name: String,
    /// The section it was drawn in. Empty: none, so the bar has it at the top.
    pub section: String,
    /// Not to be pressed now: a click passes it by, and the bar shows it faint.
    pub faint: bool,
}

/// Everything a click can land on, as last drawn.
#[derive(Clone, Debug, Default)]
pub struct Hits {
    list: Vec<Hit>,
    section: String,
}

impl Hits {
    pub fn new() -> Hits {
        Hits::default()
    }

    /// Before a draw: nothing, and no section.
    pub fn clear(&mut self) {
        self.list.clear();
        self.section.clear();
    }

    /// The section the keys drawn next fall in. Empty for none.
    pub fn section(&mut self, name: &str) {
        self.section = name.to_string();
    }

    /// A place to click that the bar doesn't list: a row, a waveform, a box.
    pub fn add(&mut self, r: Rect, id: &str) {
        self.list.push(Hit {
            r,
            id: id.to_string(),
            name: String::new(),
            section: String::new(),
            faint: false,
        });
    }

    /// A key: a place to click that the bar lists as `name`, in the section set last.
    pub fn key(&mut self, r: Rect, id: &str, name: &str, faint: bool) {
        self.list.push(Hit {
            r,
            id: id.to_string(),
            name: name.to_string(),
            section: self.section.clone(),
            faint,
        });
    }

    /// What's at a point of the logical canvas: the last drawn there that can be pressed.
    pub fn at(&self, x: f32, y: f32) -> Option<&str> {
        self.list.iter().rev().find(|h| !h.faint && h.r.holds(x, y)).map(|h| h.id.as_str())
    }

    /// Where something is, by its id.
    pub fn find(&self, id: &str) -> Option<&Hit> {
        self.list.iter().rev().find(|h| h.id == id)
    }

    /// Everything, in the order drawn.
    pub fn iter(&self) -> impl Iterator<Item = &Hit> {
        self.list.iter()
    }
}

/// What the bar calls a key, from its label: without what's only drawn (arrows, `(Enter)`),
/// lower case. `"TAKE 003 ▾"` is `take 003`.
pub fn bar_name(label: &str) -> String {
    let mut depth = 0;
    let mut kept = String::new();
    for c in label.chars() {
        match c {
            '(' => depth += 1,
            ')' if depth > 0 => depth -= 1,
            c if depth == 0 && (c.is_alphanumeric() || c == ' ') => kept.extend(c.to_lowercase()),
            _ => {}
        }
    }
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_drawn_is_on_top_and_a_faint_one_is_passed_by() {
        let mut h = Hits::new();
        h.add(Rect::new(0, 0, 100, 100), "under");
        h.section("TAKES");
        h.key(Rect::new(10, 10, 20, 20), "trash", "trash", false);
        h.key(Rect::new(40, 10, 20, 20), "empty", "empty trash", true);
        assert_eq!(h.at(15.0, 15.0), Some("trash"));
        assert_eq!(h.at(45.0, 15.0), Some("under"), "a faint key can't be pressed");
        assert_eq!(h.at(200.0, 15.0), None);
        assert_eq!(h.find("empty").map(|k| (k.section.as_str(), k.faint)), Some(("TAKES", true)));
        assert_eq!(h.find("under").map(|k| k.section.as_str()), Some(""));
        h.clear();
        assert_eq!(h.iter().count(), 0);
        h.key(Rect::new(0, 0, 1, 1), "a", "a", false);
        assert_eq!(h.find("a").unwrap().section, "", "a clear ends the section");
    }

    #[test]
    fn a_keys_name_for_the_bar() {
        assert_eq!(bar_name("TAKE 003 ▾"), "take 003");
        assert_eq!(bar_name("PLAY (Space)"), "play");
        assert_eq!(bar_name("  EMPTY   TRASH "), "empty trash");
        assert_eq!(bar_name("▶"), "");
    }
}
