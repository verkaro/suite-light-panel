//! The command bar (`../STYLE.md`, "The command bar"): vi's command line along a panel's
//! footer, in place of a pop-up. The panel's half of `kitBar` in `as-page-kit`'s `page-kit.js`,
//! and it acts as that does.
//!
//! Four uses:
//!
//! - [`Bar::ask`]: a yes or no question. `y` does it, anything else lets it go.
//! - [`Bar::offer`]: something the very next key can take up, with no timer. Its letter takes
//!   it, Esc only closes it, and any other key goes on to do what it always does.
//! - [`Bar::type_in`]: a line typed. Enter keeps it, Esc drops it, and it never closes by
//!   itself.
//! - [`Bar::choose`]: a choice or a command. Type to narrow, Tab to the next match, Enter to
//!   do it. An option can open another level, and Esc goes back up one.
//!
//! Where the page's bar is given functions, this one gives back what was decided, as an
//! [`Act`] naming the id the tool gave. A panel hands every key to [`Bar::key`] first, a click
//! on a `bar:` hit to [`Bar::tap`] and a click anywhere else to [`Bar::let_go`], and draws it
//! last with [`Bar::draw`].

use std::time::{Duration, Instant};

use lp_keys::{Key, State, code};
use lp_paint::{Align, Hits, KeyLook, Pen, Rect, skin};

use crate::line::Line;

/// One thing the bar can do, or a level of them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Opt {
    /// What's typed for it, lower case.
    pub name: String,
    /// What the tool is told when it's chosen.
    pub id: String,
    /// Not now: shown faint, and refused as a click on its key would be.
    pub faint: bool,
    /// Words to ask first, for something dangerous: `y` does it. Empty: not asked.
    pub ask: String,
    /// The next level's options. Empty: it's a thing to do.
    pub level: Vec<Opt>,
}

impl Opt {
    /// Something to do.
    pub fn new(name: &str, id: &str) -> Opt {
        Opt { name: name.into(), id: id.into(), ..Opt::default() }
    }

    /// A level: its options come up in the same place.
    pub fn level(name: &str, level: Vec<Opt>) -> Opt {
        Opt { name: name.into(), level, ..Opt::default() }
    }

    /// Not now.
    pub fn faint(mut self, faint: bool) -> Opt {
        self.faint = faint;
        self
    }

    /// Asked first, in these words.
    pub fn ask(mut self, said: &str) -> Opt {
        self.ask = said.into();
        self
    }
}

/// What the bar decided, for the tool to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Act {
    /// The thing with this id: a question answered `y`, an offer taken up, an option chosen.
    Do(String),
    /// A line typed and kept. The bar has closed. If the line won't do, open it again with
    /// [`Bar::type_in`] and say why with [`Bar::refuse`].
    Typed { id: String, value: String },
}

/// What became of a key.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Took {
    /// The bar took the key: the panel does nothing else with it. Only an offer lets one go on.
    pub taken: bool,
    pub act: Option<Act>,
}

impl Took {
    fn taken() -> Took {
        Took { taken: true, act: None }
    }

    fn did(act: Act) -> Took {
        Took { taken: true, act: Some(act) }
    }
}

#[derive(Clone, Debug)]
enum Use {
    Ask { said: String, id: String },
    Offer { said: String, letter: char, word: String, id: String },
    Type { said: String, line: Line, id: String, wrong: Option<String> },
    Choose(Choosing),
}

#[derive(Clone, Debug)]
struct Choosing {
    /// The levels opened, the last the one shown.
    levels: Vec<Vec<Opt>>,
    path: Vec<String>,
    typed: String,
    /// Which match Tab is on.
    sel: Option<usize>,
    wrong: Option<String>,
    opened: Instant,
}

impl Choosing {
    fn level(&self) -> &[Opt] {
        self.levels.last().map_or(&[], Vec::as_slice)
    }

    /// The options that start with what's typed: all of them when nothing is.
    fn matches(&self) -> Vec<&Opt> {
        let typed = self.typed.to_lowercase();
        self.level().iter().filter(|o| o.name.starts_with(&typed)).collect()
    }

    fn fresh(&mut self) {
        self.typed.clear();
        self.sel = None;
        self.wrong = None;
        self.opened = Instant::now();
    }
}

/// How long after a level opens a click on an option is ignored, so the second half of a
/// double-click doesn't land on what's appeared under the pointer.
const SETTLE: Duration = Duration::from_millis(300);

/// The bar. Closed until one of its uses opens it.
#[derive(Clone, Debug, Default)]
pub struct Bar {
    s: Option<Use>,
}

impl Bar {
    pub fn new() -> Bar {
        Bar::default()
    }

    pub fn is_open(&self) -> bool {
        self.s.is_some()
    }

    /// Whether a line is being typed: the one use that a click elsewhere, or anything else,
    /// mustn't close.
    pub fn is_typing(&self) -> bool {
        matches!(self.s, Some(Use::Type { .. }))
    }

    pub fn close(&mut self) {
        self.s = None;
    }

    /// A question: `y` gives [`Act::Do`] with `id`, anything else lets it go.
    pub fn ask(&mut self, said: &str, id: &str) {
        self.s = Some(Use::Ask { said: said.into(), id: id.into() });
    }

    /// An offer the very next key decides: `letter` takes it up ([`Act::Do`] with `id`), Esc
    /// only closes it, and any other key closes it and goes on. `word` says what the letter
    /// is for: `mark 7  i note`.
    pub fn offer(&mut self, said: &str, letter: char, word: &str, id: &str) {
        self.s = Some(Use::Offer { said: said.into(), letter, word: word.into(), id: id.into() });
    }

    /// A line to type, starting from `value`, of at most `most` characters (0: any number).
    /// Enter gives [`Act::Typed`].
    pub fn type_in(&mut self, said: &str, value: &str, most: usize, id: &str) {
        let line = Line::at_most(value, most);
        self.s = Some(Use::Type { said: said.into(), line, id: id.into(), wrong: None });
    }

    /// What's wrong with the line being typed, said in red beside it until the next key.
    pub fn refuse(&mut self, why: &str) {
        if let Some(Use::Type { wrong, .. }) = &mut self.s {
            *wrong = Some(why.into());
        }
    }

    /// What an open question, offer or line says, changed: for words that are still coming,
    /// such as a mark's number.
    pub fn resay(&mut self, now: &str) {
        if let Some(Use::Ask { said, .. } | Use::Offer { said, .. } | Use::Type { said, .. }) = &mut self.s {
            *said = now.into();
        }
    }

    /// A choice or a command, from these options.
    pub fn choose(&mut self, options: Vec<Opt>) {
        self.s = Some(Use::Choose(Choosing {
            levels: vec![options],
            path: vec![],
            typed: String::new(),
            sel: None,
            wrong: None,
            opened: Instant::now(),
        }));
    }

    /// The screen's levels, from the keys drawn last: a section is a level and its keys the
    /// options in it, a key in no section an option at the top. Each option's id is its key's,
    /// so choosing it is a click on the key. `extra` are the tool's own, after: what has no
    /// place on the screen.
    pub fn screen(hits: &Hits, extra: Vec<Opt>) -> Vec<Opt> {
        let mut top: Vec<Opt> = Vec::new();
        for h in hits.iter().filter(|h| !h.name.is_empty()) {
            let o = Opt::new(&h.name, &h.id).faint(h.faint);
            if h.section.is_empty() {
                top.push(o);
                continue;
            }
            let name = h.section.to_lowercase();
            match top.iter_mut().find(|l| l.name == name && !l.level.is_empty()) {
                Some(level) => level.level.push(o),
                None => top.push(Opt::level(&name, vec![o])),
            }
        }
        top.extend(extra);
        top
    }

    /// An option taken: refused, opened as a level, asked about, or done.
    fn take(&mut self, o: Opt) -> Option<Act> {
        let Some(Use::Choose(c)) = &mut self.s else { return None };
        if o.faint {
            c.wrong = Some(format!("{}: not now", o.name));
            return None;
        }
        if !o.level.is_empty() {
            c.levels.push(o.level);
            c.path.push(o.name);
            c.fresh();
            return None;
        }
        if !o.ask.is_empty() {
            self.ask(&o.ask, &o.id);
            return None;
        }
        self.s = None;
        Some(Act::Do(o.id))
    }

    fn enter(&mut self) -> Option<Act> {
        match &mut self.s {
            Some(Use::Type { line, id, .. }) => {
                let act = Act::Typed { id: id.clone(), value: line.text() };
                self.s = None;
                Some(act)
            }
            Some(Use::Choose(c)) => {
                let found = c.matches();
                if !c.typed.is_empty() && found.is_empty() {
                    c.wrong = Some(format!("no {} here", c.typed));
                    return None;
                }
                if c.typed.is_empty() && c.sel.is_none() {
                    return None;
                }
                let o = found.get(c.sel.unwrap_or(0)).copied().cloned()?;
                self.take(o)
            }
            _ => None,
        }
    }

    /// Esc: up a level, and closed from the top.
    fn escape(&mut self) {
        if let Some(Use::Choose(c)) = &mut self.s
            && !c.path.is_empty()
        {
            c.levels.pop();
            c.path.pop();
            c.fresh();
            return;
        }
        self.s = None;
    }

    /// A key. Closed, the bar takes nothing. Open, it takes every key but the one an offer
    /// lets go on.
    pub fn key(&mut self, k: &Key) -> Took {
        let Some(s) = &mut self.s else { return Took::default() };
        if k.state == State::Release {
            // A key let go isn't the next key. An offer mustn't swallow the release of
            // whatever key made it.
            return Took { taken: !matches!(s, Use::Offer { .. }), act: None };
        }
        let Some(name) = k.name() else {
            // A modifier alone, or a chord: not a key to the bar, and not the tool's either
            // while it's open.
            return Took::taken();
        };
        match s {
            Use::Ask { id, .. } => {
                // A held key's repeat isn't the next key: it would close what it just opened.
                if k.state == State::Repeat {
                    return Took::taken();
                }
                let id = id.clone();
                self.s = None;
                if name == "y" { Took::did(Act::Do(id)) } else { Took::taken() }
            }
            Use::Offer { letter, id, .. } => {
                if k.state == State::Repeat {
                    return Took::taken();
                }
                let (letter, id) = (*letter, id.clone());
                self.s = None;
                if k.ch == Some(letter) {
                    Took::did(Act::Do(id))
                } else {
                    Took { taken: k.code == code::ESC, act: None }
                }
            }
            Use::Type { line, wrong, .. } => {
                match name.as_str() {
                    "Escape" => self.s = None,
                    "Enter" => return Took { taken: true, act: self.enter() },
                    _ => {
                        line.key(k);
                        *wrong = None;
                    }
                }
                Took::taken()
            }
            Use::Choose(c) => {
                match name.as_str() {
                    "Escape" => self.escape(),
                    "Enter" => return Took { taken: true, act: self.enter() },
                    "Tab" => {
                        let n = c.matches().len();
                        if n > 0 {
                            c.sel = Some(match (c.sel, k.mods.shift) {
                                (None, false) => 0,
                                (None, true) => n - 1,
                                (Some(i), false) => (i + 1) % n,
                                (Some(i), true) => (i + n - 1) % n,
                            });
                        }
                    }
                    // On an empty bar it does nothing, so a held one can't run on into the
                    // tool's own Backspace.
                    "Backspace" => {
                        c.typed.pop();
                        c.sel = None;
                        c.wrong = None;
                    }
                    _ => {
                        if let Some(ch) = k.ch {
                            c.typed.push(ch);
                            c.sel = None;
                            c.wrong = None;
                        }
                    }
                }
                Took::taken()
            }
        }
    }

    /// A click on the bar, by its hit's id: `bar:yes`, `bar:offer`, `bar:done`, `bar:esc`,
    /// `bar:opt:N`.
    pub fn tap(&mut self, id: &str) -> Option<Act> {
        let what = id.strip_prefix("bar:")?;
        match (what, &self.s) {
            ("yes", Some(Use::Ask { id, .. })) | ("offer", Some(Use::Offer { id, .. })) => {
                let id = id.clone();
                self.s = None;
                Some(Act::Do(id))
            }
            ("done", Some(Use::Type { .. })) => self.enter(),
            ("esc", Some(_)) => {
                self.escape();
                None
            }
            (opt, Some(Use::Choose(c))) => {
                let i: usize = opt.strip_prefix("opt:")?.parse().ok()?;
                if c.opened.elapsed() < SETTLE {
                    return None;
                }
                let o = c.level().get(i)?.clone();
                self.take(o)
            }
            _ => None,
        }
    }

    /// A click anywhere else: a line being typed stays, anything else is let go.
    pub fn let_go(&mut self) {
        if !self.is_typing() {
            self.s = None;
        }
    }

    /// Lets a click on an option through at once: for a test, which doesn't double-click.
    pub fn settled(&mut self) {
        if let Some(Use::Choose(c)) = &mut self.s {
            c.opened = Instant::now().checked_sub(SETTLE).unwrap_or(c.opened);
        }
    }

    /// The bar along `r`, the footer. Closed, it says `idle`, faint: a hint, or nothing. Open,
    /// its edge is amber, and what can be clicked is put in `hits` as `bar:…`.
    pub fn draw(&self, pen: &mut Pen, hits: &mut Hits, r: Rect, idle: &str) {
        const SIZE: f32 = 13.0;
        pen.rrect(r, 3.0, skin().well, if self.s.is_some() { skin().amber } else { skin().rule });
        let Some(s) = &self.s else {
            pen.text(Rect::new(r.x + 10, r.y, r.w - 20, r.h), idle, 11.0, false, skin().faint, Align::Left);
            return;
        };
        let end = (r.right() - 10) as f32;
        let mut x = (r.x + 10) as f32;
        // A piece of the line, from where the last ended; with an id, somewhere to click.
        let say = |pen: &mut Pen, hits: &mut Hits, x: &mut f32, s: &str, c, id: &str| {
            let from = *x;
            *x = pen.run(from, r, s, SIZE, true, c);
            if !id.is_empty() {
                hits.add(Rect::new(from as i32 - 5, r.y, (*x - from) as i32 + 10, r.h), id);
            }
        };
        match s {
            Use::Ask { said, .. } => {
                say(pen, hits, &mut x, &format!("{said}  "), skin().text, "");
                say(pen, hits, &mut x, "y", skin().amber, "bar:yes");
                say(pen, hits, &mut x, "  anything else lets it go", skin().faint, "");
            }
            Use::Offer { said, letter, word, .. } => {
                say(pen, hits, &mut x, &format!("{said}  "), skin().text, "");
                say(pen, hits, &mut x, &format!("{letter} {word}"), skin().dim, "bar:offer");
            }
            Use::Type { said, line, wrong, .. } => {
                say(pen, hits, &mut x, &format!("{said}  "), skin().dim, "");
                let keys = 146;
                let room = end - keys as f32 - x;
                let each = pen.width("M", SIZE, true).max(1.0);
                let text: Vec<char> = line.text().chars().collect();
                // As much as fits around the caret.
                let fit = ((room / each) as usize).max(2);
                let from = if line.caret() >= fit { line.caret() + 1 - fit } else { 0 };
                let shown: String = text[from..].iter().take(fit).collect();
                let start = x;
                say(pen, hits, &mut x, &shown, skin().readout, "");
                let at = start + (line.caret() - from) as f32 * each;
                let caret = Rect::new(
                    pen.v.x(at),
                    pen.v.y(r.y as f32 + 4.0),
                    pen.v.len(1.5),
                    pen.v.len(r.h as f32 - 8.0),
                );
                pen.sf.fill(caret, skin().amber);
                if let Some(why) = wrong {
                    say(pen, hits, &mut x, &format!("   {why}"), skin().red, "");
                } else if line.full() {
                    say(pen, hits, &mut x, "   no longer than this", skin().faint, "");
                }
                let done = Rect::new(r.right() - 142, r.y + 2, 70, r.h - 4);
                let cancel = Rect::new(r.right() - 68, r.y + 2, 64, r.h - 4);
                let off = KeyLook { off_bar: true, ..KeyLook::default() };
                pen.key(hits, done, "DONE", "bar:done", off);
                pen.key(hits, cancel, "CANCEL", "bar:esc", off);
            }
            Use::Choose(c) => {
                let path =
                    if c.path.is_empty() { ":".to_string() } else { format!(":{} ", c.path.join(" ")) };
                say(pen, hits, &mut x, &path, skin().amber, "");
                if !c.typed.is_empty() {
                    let found = c.matches();
                    say(pen, hits, &mut x, &c.typed, skin().readout, "");
                    let caret = Rect::new(
                        pen.v.x(x),
                        pen.v.y(r.y as f32 + 4.0),
                        pen.v.len(1.5),
                        pen.v.len(r.h as f32 - 8.0),
                    );
                    pen.sf.fill(caret, skin().amber);
                    x += 3.0;
                    // The first match, or the one Tab is on, finished after the caret.
                    if let Some(o) = found.get(c.sel.unwrap_or(0)) {
                        let rest: String = o.name.chars().skip(c.typed.chars().count()).collect();
                        say(pen, hits, &mut x, &rest, skin().dim, "");
                    }
                    if let Some(why) = &c.wrong {
                        say(pen, hits, &mut x, &format!("   {why}"), skin().red, "");
                    }
                    return;
                }
                // The options, greyed and clickable, scrolled so the one Tab is on shows.
                let level = c.level();
                let gap = 18.0;
                if c.path.is_empty() {
                    x += 8.0;
                }
                let wide = |pen: &Pen, o: &Opt| pen.width(&o.name, SIZE, true);
                let sel = c.sel.unwrap_or(0).min(level.len().saturating_sub(1));
                let mut first = 0;
                while first < sel
                    && x + level[first..=sel].iter().map(|o| wide(pen, o) + gap).sum::<f32>() > end - 20.0
                {
                    first += 1;
                }
                for (i, o) in level.iter().enumerate().skip(first) {
                    if x + wide(pen, o) > end - 20.0 {
                        say(pen, hits, &mut x, "…", skin().faint, "");
                        break;
                    }
                    let color = if c.sel == Some(i) {
                        skin().text
                    } else if o.faint {
                        skin().faint
                    } else {
                        skin().dim
                    };
                    say(pen, hits, &mut x, &o.name, color, &format!("bar:opt:{i}"));
                    x += gap;
                }
                if let Some(why) = &c.wrong {
                    x += 4.0;
                    say(pen, hits, &mut x, why, skin().red, "");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::keys_of;
    use lp_keys::Mods;
    use lp_paint::dark;
    use lp_paint::{Fonts, Image, View};

    /// Types `keys` at the bar: what it did, and whether every key was taken.
    fn typed(b: &mut Bar, keys: &str) -> (Vec<Act>, bool) {
        let mut acts = vec![];
        let mut all = true;
        for k in keys_of(keys).unwrap() {
            let t = b.key(&k);
            all &= t.taken;
            acts.extend(t.act);
        }
        (acts, all)
    }

    fn does(id: &str) -> Vec<Act> {
        vec![Act::Do(id.into())]
    }

    fn options() -> Vec<Opt> {
        vec![
            Opt::new("play", "play"),
            Opt::new("export", "export").faint(true),
            Opt::level(
                "takes",
                vec![
                    Opt::new("trash", "trash").ask("trash take 3?"),
                    Opt::new("tag", "tag"),
                    Opt::new("take", "take"),
                ],
            ),
            Opt::new("plain", "plain"),
        ]
    }

    #[test]
    fn closed_it_takes_nothing() {
        let mut b = Bar::new();
        assert_eq!(typed(&mut b, "y{esc}{enter}"), (vec![], false));
        assert!(!b.is_open());
        assert_eq!(b.tap("bar:yes"), None);
    }

    #[test]
    fn a_question_is_answered_with_y_and_anything_else_lets_it_go() {
        let mut b = Bar::new();
        b.ask("revert the last recording?", "revert");
        assert_eq!(typed(&mut b, "y"), (does("revert"), true));
        assert!(!b.is_open());
        b.ask("revert?", "revert");
        assert_eq!(typed(&mut b, "n"), (vec![], true), "the key is taken, so it does nothing else");
        assert!(!b.is_open());
        // Shift alone, a key held from before, and a key let go aren't an answer.
        b.ask("revert?", "revert");
        let mut k = keys_of("y").unwrap()[0];
        k.state = State::Repeat;
        assert_eq!(b.key(&k), Took::taken());
        k.state = State::Release;
        assert_eq!(b.key(&k), Took::taken());
        let shift = Key { code: code::LEFTSHIFT, state: State::Press, mods: Mods::default(), ch: None };
        assert_eq!(b.key(&shift), Took::taken());
        assert!(b.is_open());
        assert_eq!(b.tap("bar:yes"), Some(Act::Do("revert".into())), "a click on y is a y");
    }

    #[test]
    fn an_offer_is_decided_by_the_very_next_key() {
        let mut b = Bar::new();
        b.offer("mark 7", 'i', "note", "note");
        b.resay("mark 8");
        assert_eq!(typed(&mut b, "i"), (does("note"), true));
        // Any other key goes on to do what it always does, and the offer is gone.
        b.offer("mark 7", 'i', "note", "note");
        assert_eq!(typed(&mut b, "s"), (vec![], false));
        assert!(!b.is_open());
        // Esc only closes it.
        b.offer("mark 7", 'i', "note", "note");
        assert_eq!(typed(&mut b, "{esc}"), (vec![], true));
        assert!(!b.is_open());
        // The key that made the offer, let go, isn't the next key.
        b.offer("mark 7", 'i', "note", "note");
        let mut m = keys_of("m").unwrap()[0];
        m.state = State::Release;
        assert_eq!(b.key(&m), Took::default());
        m.state = State::Repeat;
        assert_eq!(b.key(&m), Took::taken());
        assert!(b.is_open());
        assert_eq!(b.tap("bar:offer"), Some(Act::Do("note".into())));
    }

    #[test]
    fn a_line_is_typed_and_never_closes_by_itself() {
        let mut b = Bar::new();
        b.type_in("name", "m1", 0, "name");
        assert!(b.is_typing());
        b.let_go();
        assert!(b.is_open(), "a click elsewhere leaves a line being typed");
        let (acts, all) = typed(&mut b, "{backspace}2 s{left}{left}!{tab}{enter}");
        assert!(all, "every key is the bar's: an s stops nothing");
        assert_eq!(acts, [Act::Typed { id: "name".into(), value: "m2! s".into() }]);
        assert!(!b.is_open());
        // Refused: opened again with why, which the next key clears.
        b.type_in("name", "m2", 4, "name");
        b.refuse("m2 is used already");
        assert!(matches!(&b.s, Some(Use::Type { wrong: Some(w), .. }) if w == "m2 is used already"));
        assert_eq!(typed(&mut b, "345"), (vec![], true));
        assert!(matches!(&b.s, Some(Use::Type { wrong: None, line, .. }) if line.text() == "m234"));
        assert_eq!(b.tap("bar:done"), Some(Act::Typed { id: "name".into(), value: "m234".into() }));
        b.type_in("name", "m1", 0, "name");
        assert_eq!(typed(&mut b, "x{esc}"), (vec![], true));
        assert!(!b.is_open(), "Esc drops it");
    }

    #[test]
    fn a_command_is_narrowed_by_typing_and_done_with_enter() {
        let mut b = Bar::new();
        b.choose(options());
        // Enter on nothing typed and nothing chosen does nothing.
        assert_eq!(typed(&mut b, "{enter}"), (vec![], true));
        assert!(b.is_open());
        assert_eq!(typed(&mut b, "pla{enter}"), (does("play"), true), "the first match");
        assert!(!b.is_open());
        b.choose(options());
        assert_eq!(typed(&mut b, "pl{tab}{tab}{enter}"), (does("plain"), true), "Tab to the next match");
        b.choose(options());
        assert_eq!(typed(&mut b, "zz{enter}"), (vec![], true));
        assert!(matches!(&b.s, Some(Use::Choose(c)) if c.wrong.as_deref() == Some("no zz here")));
        // Backspace clears what's typed, and on an empty bar does nothing.
        assert_eq!(typed(&mut b, "{backspace}{backspace}{backspace}{backspace}"), (vec![], true));
        assert!(matches!(&b.s, Some(Use::Choose(c)) if c.typed.is_empty() && c.wrong.is_none()));
        // What's faint is refused, as a click on its key would be.
        assert_eq!(typed(&mut b, "ex{enter}"), (vec![], true));
        assert!(matches!(&b.s, Some(Use::Choose(c)) if c.wrong.as_deref() == Some("export: not now")));
        // Tab with nothing typed walks the options, and back with Shift.
        b.choose(options());
        let mut back = keys_of("{tab}").unwrap()[0];
        back.mods.shift = true;
        b.key(&back);
        assert_eq!(typed(&mut b, "{enter}"), (does("plain"), true));
        b.let_go();
        b.choose(options());
        b.let_go();
        assert!(!b.is_open(), "a click elsewhere lets a choice go");
    }

    #[test]
    fn a_level_opens_in_the_same_place_and_esc_goes_up_one() {
        let mut b = Bar::new();
        b.choose(options());
        assert_eq!(typed(&mut b, "tak{enter}"), (vec![], true));
        assert!(matches!(&b.s, Some(Use::Choose(c)) if c.path == ["takes"] && c.typed.is_empty()));
        // Names can be reused from level to level: "ta" is tag here.
        assert_eq!(typed(&mut b, "ta{enter}"), (does("tag"), true));
        b.choose(options());
        typed(&mut b, "takes{enter}");
        assert_eq!(typed(&mut b, "{esc}"), (vec![], true));
        assert!(matches!(&b.s, Some(Use::Choose(c)) if c.path.is_empty()), "up a level");
        typed(&mut b, "{esc}");
        assert!(!b.is_open(), "and closed from the top");
        // Something dangerous asks, in words, and y does it.
        b.choose(options());
        assert_eq!(typed(&mut b, "takes{enter}tr{enter}"), (vec![], true));
        assert!(matches!(&b.s, Some(Use::Ask { said, .. }) if said == "trash take 3?"));
        assert_eq!(typed(&mut b, "y"), (does("trash"), true));
        b.choose(options());
        typed(&mut b, "takes{enter}tr{enter}n");
        assert!(!b.is_open());
    }

    #[test]
    fn a_click_just_after_a_level_opens_is_ignored() {
        let mut b = Bar::new();
        b.choose(options());
        assert_eq!(b.tap("bar:opt:0"), None, "the second half of a double-click");
        assert!(b.is_open());
        b.settled();
        assert_eq!(b.tap("bar:opt:2"), None, "a level");
        assert_eq!(b.tap("bar:opt:1"), None, "and again, just opened");
        b.settled();
        assert_eq!(b.tap("bar:opt:9"), None);
        assert_eq!(b.tap("bar:opt:1"), Some(Act::Do("tag".into())));
        b.choose(options());
        typed(&mut b, "takes{enter}");
        assert_eq!(b.tap("bar:esc"), None);
        assert!(matches!(&b.s, Some(Use::Choose(c)) if c.path.is_empty()));
        assert_eq!(b.tap("row:3"), None, "not the bar's");
    }

    #[test]
    fn the_screens_levels_come_from_its_keys() {
        let mut hits = Hits::new();
        hits.add(Rect::new(0, 0, 10, 10), "row:0");
        hits.key(Rect::new(0, 0, 10, 10), "keys", "keys", false);
        hits.section("ROOM");
        hits.key(Rect::new(0, 0, 10, 10), "keep", "keep", false);
        hits.key(Rect::new(0, 0, 10, 10), "export", "export", true);
        hits.section("LINE");
        hits.key(Rect::new(0, 0, 10, 10), "play", "play", false);
        let got = Bar::screen(&hits, vec![Opt::new("quit", "quit")]);
        let names: Vec<&str> = got.iter().map(|o| o.name.as_str()).collect();
        assert_eq!(names, ["keys", "room", "line", "quit"], "a row isn't a command");
        assert_eq!(got[1].level, [Opt::new("keep", "keep"), Opt::new("export", "export").faint(true)]);
        let mut b = Bar::new();
        b.choose(got);
        assert_eq!(
            typed(&mut b, "r{enter}k{enter}"),
            (does("keep"), true),
            "choosing it is a click on its key"
        );
    }

    fn drawn(b: &Bar, hits: &mut Hits) -> Option<Image> {
        let fs = Fonts::terminus(None).ok()?;
        let mut img = Image::new(900, 26);
        let mut sf = img.surface();
        let mut p = Pen { sf: &mut sf, fonts: &fs, v: View::fit(900, 26, 900, 26) };
        b.draw(&mut p, hits, Rect::new(0, 0, 900, 26), "? the keys   : what this screen does");
        Some(img)
    }

    fn has(img: &Image, c: u32) -> bool {
        img.px.contains(&c)
    }

    #[test]
    fn drawn_along_the_footer_with_what_can_be_clicked() {
        let mut b = Bar::new();
        let mut hits = Hits::new();
        let Some(idle) = drawn(&b, &mut hits) else { return };
        assert!(has(&idle, dark::FAINT) && !has(&idle, dark::AMBER) && hits.iter().count() == 0);

        b.ask("trash take 3?", "trash");
        let img = drawn(&b, &mut hits).unwrap();
        assert!(has(&img, dark::AMBER) && has(&img, dark::TEXT));
        let yes = hits.find("bar:yes").unwrap().r;
        assert_eq!(hits.at(yes.x as f32 + 6.0, 10.0), Some("bar:yes"));

        hits.clear();
        b.choose(options());
        let img = drawn(&b, &mut hits).unwrap();
        assert!(has(&img, dark::DIM) && has(&img, dark::FAINT), "greyed, and what's not for now fainter");
        let ids: Vec<&str> = hits.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(ids, ["bar:opt:0", "bar:opt:1", "bar:opt:2", "bar:opt:3"]);
        // In order along the bar, none over another.
        let xs: Vec<(i32, i32)> = hits.iter().map(|h| (h.r.x, h.r.right())).collect();
        assert!(xs.windows(2).all(|w| w[0].1 <= w[1].0), "{xs:?}");

        hits.clear();
        typed(&mut b, "pl");
        let img = drawn(&b, &mut hits).unwrap();
        assert!(has(&img, dark::READOUT) && has(&img, dark::DIM), "what's typed, and the match finished");
        assert_eq!(hits.iter().count(), 0, "typing: the options are gone");
        typed(&mut b, "zz{enter}");
        assert!(has(&drawn(&b, &mut hits).unwrap(), dark::RED));

        hits.clear();
        b.type_in("name", "m1", 0, "name");
        let img = drawn(&b, &mut hits).unwrap();
        assert!(has(&img, dark::READOUT) && has(&img, dark::BTN));
        assert!(hits.find("bar:done").is_some() && hits.find("bar:esc").is_some());

        // More options than fit: cut with …, and Tab scrolls to the one it's on.
        hits.clear();
        let many: Vec<Opt> = (0..40).map(|i| Opt::new(&format!("option{i}"), &format!("o{i}"))).collect();
        b.choose(many);
        drawn(&b, &mut hits).unwrap();
        let shown = hits.iter().count();
        assert!(shown > 3 && shown < 40, "{shown}");
        assert!(hits.iter().all(|h| h.r.right() <= 900));
        for _ in 0..30 {
            typed(&mut b, "{tab}");
        }
        hits.clear();
        drawn(&b, &mut hits).unwrap();
        assert!(hits.find("bar:opt:29").is_some() && hits.find("bar:opt:0").is_none());
    }
}
