//! A line being typed: its text and where the caret is. What the command bar types into, and
//! a form's field (`lp_paint::Field` draws one).

use lp_keys::{Key, code};

/// A line of text and a caret in it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Line {
    text: Vec<char>,
    caret: usize,
    /// The most characters it takes. 0: any number.
    most: usize,
}

impl Line {
    /// Starting from `value`, the caret at its end.
    pub fn new(value: &str) -> Line {
        let text: Vec<char> = value.chars().collect();
        Line { caret: text.len(), text, most: 0 }
    }

    /// The same, taking no more than `most` characters.
    pub fn at_most(value: &str, most: usize) -> Line {
        let mut l = Line::new(value);
        l.most = most;
        l
    }

    pub fn text(&self) -> String {
        self.text.iter().collect()
    }

    /// The caret: before this character.
    pub fn caret(&self) -> usize {
        self.caret
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Whether it's as long as it may be.
    pub fn full(&self) -> bool {
        self.most > 0 && self.text.len() >= self.most
    }

    /// A key pressed or repeating: a character typed at the caret, Backspace and Delete, ← →,
    /// Home and End. Whether it was one of those. Enter, Esc and Tab are the caller's.
    pub fn key(&mut self, k: &Key) -> bool {
        if !k.down() || k.mods.ctrl || k.mods.alt || k.mods.meta {
            return false;
        }
        if let Some(c) = k.ch {
            if !self.full() {
                self.text.insert(self.caret, c);
                self.caret += 1;
            }
            return true;
        }
        match k.code {
            code::BACKSPACE => {
                if self.caret > 0 {
                    self.caret -= 1;
                    self.text.remove(self.caret);
                }
            }
            code::DELETE => {
                if self.caret < self.text.len() {
                    self.text.remove(self.caret);
                }
            }
            code::LEFT => self.caret = self.caret.saturating_sub(1),
            code::RIGHT => self.caret = (self.caret + 1).min(self.text.len()),
            code::HOME => self.caret = 0,
            code::END => self.caret = self.text.len(),
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::keys_of;

    fn typed(l: &mut Line, keys: &str) {
        for k in keys_of(keys).unwrap() {
            assert!(l.key(&k), "{k:?}");
        }
    }

    #[test]
    fn typed_at_the_caret() {
        let mut l = Line::new("stair");
        assert_eq!((l.text().as_str(), l.caret()), ("stair", 5));
        typed(&mut l, "well{left}{left}{left}{left}{backspace}R W{home}a {end}!");
        assert_eq!(l.text(), "a staiR Wwell!");
        typed(&mut l, "{home}{delete}{delete}{backspace}{left}");
        assert_eq!((l.text().as_str(), l.caret()), ("staiR Wwell!", 0));
        typed(&mut l, "{end}{delete}{right}");
        assert_eq!(l.caret(), 12);
        // What isn't the line's is left for whoever holds it.
        for k in keys_of("{enter}{esc}{tab}{up}").unwrap() {
            assert!(!l.key(&k));
        }
        assert_eq!(l.text(), "staiR Wwell!");
    }

    #[test]
    fn no_longer_than_it_may_be() {
        let mut l = Line::at_most("ab", 3);
        typed(&mut l, "cd");
        assert_eq!(l.text(), "abc");
        assert!(l.full() && !l.is_empty());
        typed(&mut l, "{backspace}z");
        assert_eq!(l.text(), "abz");
    }
}
