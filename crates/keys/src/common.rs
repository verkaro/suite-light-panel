//! The family's common keys (`../STYLE.md`, "Keys"), held by value as `as-page-kit`'s
//! `page-kit.js` has them in `KEYS`, and a tool's own list held to them. A test compares the
//! two tables when `../suite-audio-libs` is beside this suite, as the dark skin's colours are
//! compared, so they can't drift apart unseen.
//!
//! The table is data, not a handler: what `s` does is each tool's own. A key is named as a
//! page names it, by its `KeyboardEvent.key` (`"j"`, `" "`, `"Escape"`, `"ArrowLeft"`), so a
//! tool's `keys.json` serves its page and its panel alike. [`Key::name`] gives a pressed key
//! that name.

use crate::{Key, code};

/// Each common key and what it means, in the page's order.
pub const KEYS: [(&str, &str); 23] = [
    ("?", "the keys, over the screen"),
    (":", "the bar: what this screen does"),
    ("Escape", "back out of what's open"),
    (" ", "play and stop"),
    ("Tab", "the next area or view"),
    ("ArrowLeft", "along what's there"),
    ("ArrowRight", "along what's there"),
    ("ArrowUp", "along what's there"),
    ("ArrowDown", "along what's there"),
    ("q", "quit this client"),
    ("s", "stop, then to the start"),
    ("j", "the next"),
    ("k", "the one before"),
    ("h", "back, a nudge by ear"),
    ("l", "on, a nudge by ear"),
    ("H", "the first"),
    ("L", "the last"),
    ("/", "search"),
    ("x", "take this away"),
    ("u", "undo"),
    ("i", "start changing this in place"),
    ("Enter", "open or choose; keep what's typed"),
    ("y", "yes to a question"),
];

/// What the family means by `key`, if it's a common one.
pub fn means(key: &str) -> Option<&'static str> {
    KEYS.iter().find(|(k, _)| *k == key).map(|(_, m)| *m)
}

/// A key as `?` shows it: `Space` for `" "`, `Esc`, the arrows as arrows.
pub fn shown(key: &str) -> &str {
    match key {
        " " => "Space",
        "Escape" => "Esc",
        "ArrowLeft" => "←",
        "ArrowRight" => "→",
        "ArrowUp" => "↑",
        "ArrowDown" => "↓",
        "Backspace" => "⌫",
        "PageUp" => "PgUp",
        "PageDown" => "PgDn",
        k => k,
    }
}

/// One of a tool's keys, as its `keys.json` lists it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Declared {
    pub key: String,
    /// What it does. For a common key, the family's words for it.
    pub does: String,
    /// Where, or on what: "a row".
    pub note: String,
    /// The screen it's on.
    pub screen: String,
    /// Why, when a common key means something else here. Empty when it doesn't.
    pub differs: String,
}

/// Holds a tool's list to the family's table: a common key whose `does` isn't the table's,
/// word for word, with no `differs`, is wrong. So a change to the table fails here when a
/// tool's pin moves onto it, until the tool has dealt with it. Says everything wrong, a line
/// each.
pub fn check(list: &[Declared]) -> Result<(), String> {
    let mut wrong = Vec::new();
    for d in list {
        if d.key.is_empty() || d.does.is_empty() || d.screen.is_empty() {
            wrong.push(format!("an entry wants key, does and screen: {d:?}"));
            continue;
        }
        if let Some(means) = means(&d.key)
            && means != d.does
            && d.differs.is_empty()
        {
            wrong.push(format!(
                "{} on {}: says \"{}\"; the family's is \"{means}\" (or say why, in differs)",
                d.key, d.screen, d.does
            ));
        }
    }
    if wrong.is_empty() { Ok(()) } else { Err(wrong.join("\n")) }
}

impl Key {
    /// The key as a page names it (`KeyboardEvent.key`), which is how [`KEYS`] and a tool's
    /// `keys.json` name it: what it types, or `Escape`, `Enter`, `Tab`, `Backspace`, `Delete`,
    /// `ArrowLeft` and the rest, `Home`, `End`, `PageUp`, `PageDown`. `None` for a modifier
    /// alone, a key with no name here, and anything with Ctrl, Alt or the meta key held.
    pub fn name(&self) -> Option<String> {
        if self.mods.ctrl || self.mods.alt || self.mods.meta {
            return None;
        }
        if let Some(c) = self.ch {
            return Some(c.to_string());
        }
        let name = match self.code {
            code::ESC => "Escape",
            code::ENTER | code::KPENTER => "Enter",
            code::TAB => "Tab",
            code::BACKSPACE => "Backspace",
            code::DELETE => "Delete",
            code::LEFT => "ArrowLeft",
            code::RIGHT => "ArrowRight",
            code::UP => "ArrowUp",
            code::DOWN => "ArrowDown",
            code::HOME => "Home",
            code::END => "End",
            code::PAGEUP => "PageUp",
            code::PAGEDOWN => "PageDown",
            _ => return None,
        };
        Some(name.to_string())
    }
}

/// The key that types `c` on a US layout, and whether Shift is held for it.
pub fn code_of(c: char) -> Option<(u16, bool)> {
    (2..=57u16).find_map(|code| {
        let (plain, shifted) = crate::us(code)?;
        if plain == c {
            Some((code, false))
        } else if shifted == c {
            Some((code, true))
        } else {
            None
        }
    })
}

/// Every key a keyboard has a name for here, each pressed once: for a panel's test that no key
/// does something its `keys.json` doesn't list.
pub fn every() -> Vec<Key> {
    use crate::{Mods, State};
    let key =
        |code, shift, ch| Key { code, state: State::Press, mods: Mods { shift, ..Mods::default() }, ch };
    let mut out = Vec::new();
    for code in 2..=57u16 {
        if let Some((plain, shifted)) = crate::us(code) {
            out.push(key(code, false, Some(plain)));
            if shifted != plain {
                out.push(key(code, true, Some(shifted)));
            }
        }
    }
    for code in [
        code::ESC,
        code::ENTER,
        code::TAB,
        code::BACKSPACE,
        code::DELETE,
        code::LEFT,
        code::RIGHT,
        code::UP,
        code::DOWN,
        code::HOME,
        code::END,
        code::PAGEUP,
        code::PAGEDOWN,
    ] {
        out.push(key(code, false, None));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Mods, State};

    /// `KEYS` from page-kit.js: each `{"key": "…", "means": "…"}`.
    fn page_kit_keys(js: &str) -> Vec<(String, String)> {
        let at = js.find("const KEYS = [").expect("KEYS") + "const KEYS = [".len();
        let body = &js[at..at + js[at..].find("\n];").expect("its end")];
        body.lines()
            .filter_map(|line| {
                let rest = line.trim().strip_prefix("{\"key\": \"")?;
                let (key, rest) = rest.split_once("\", \"means\": \"")?;
                let (means, _) = rest.rsplit_once("\"}")?;
                Some((key.to_string(), means.to_string()))
            })
            .collect()
    }

    #[test]
    fn the_same_as_the_pages_table() {
        let js =
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../../suite-audio-libs/crates/page-kit/src/page-kit.js");
        let Ok(js) = std::fs::read_to_string(js) else {
            eprintln!("no suite-audio-libs beside this suite: page-kit.js not compared");
            return;
        };
        let page = page_kit_keys(&js);
        let ours: Vec<(String, String)> = KEYS.iter().map(|(k, m)| (k.to_string(), m.to_string())).collect();
        assert_eq!(page, ours);
    }

    fn entry(key: &str, does: &str, differs: &str) -> Declared {
        Declared {
            key: key.into(),
            does: does.into(),
            screen: "library".into(),
            differs: differs.into(),
            ..Declared::default()
        }
    }

    #[test]
    fn a_common_key_used_for_something_else_is_refused_unless_it_says_why() {
        assert_eq!(check(&[entry("j", "the next", ""), entry("t", "tag this", "")]), Ok(()));
        let e = check(&[entry("s", "solo", "")]).unwrap_err();
        assert!(e.contains("s on library") && e.contains("stop, then to the start"), "{e}");
        assert_eq!(check(&[entry("s", "solo", "because")]), Ok(()));
        assert!(check(&[entry("j", "", "")]).unwrap_err().contains("wants key"));
    }

    #[test]
    fn a_key_is_named_as_a_page_names_it() {
        let key = |code, ch, mods| Key { code, state: State::Press, mods, ch };
        let none = Mods::default();
        assert_eq!(key(36, Some('j'), none).name().as_deref(), Some("j"));
        assert_eq!(key(code::SPACE, Some(' '), none).name().as_deref(), Some(" "));
        assert_eq!(key(code::ESC, None, none).name().as_deref(), Some("Escape"));
        assert_eq!(key(code::KPENTER, None, none).name().as_deref(), Some("Enter"));
        assert_eq!(key(code::DOWN, None, none).name().as_deref(), Some("ArrowDown"));
        assert_eq!(key(code::LEFTSHIFT, None, Mods { shift: true, ..none }).name(), None);
        assert_eq!(key(code::ESC, None, Mods { ctrl: true, alt: true, ..none }).name(), None);
        // Every common key can be typed and comes back by its name.
        for (k, _) in KEYS {
            let mut cs = k.chars();
            if let (Some(c), None) = (cs.next(), cs.next()) {
                let (code, shift) = code_of(c).unwrap_or_else(|| panic!("{k:?} has no key"));
                assert_eq!(key(code, Some(c), Mods { shift, ..none }).name().as_deref(), Some(k));
            }
        }
        assert_eq!(code_of(':'), Some((39, true)));
        // Every key there is has a name, and no two the same.
        let mut names: Vec<String> = every().iter().map(|k| k.name().expect("a name")).collect();
        let n = names.len();
        names.sort();
        names.dedup();
        assert!(names.len() == n && n > 100, "{n}");
        assert!(KEYS.iter().all(|(k, _)| names.iter().any(|n| n == k)));
        assert_eq!(code_of('é'), None);
    }

    #[test]
    fn shown_as_help_shows_them() {
        assert_eq!((shown(" "), shown("Escape"), shown("j"), shown("ArrowUp")), ("Space", "Esc", "j", "↑"));
        assert_eq!(means("u"), Some("undo"));
        assert_eq!(means("t"), None);
    }
}
