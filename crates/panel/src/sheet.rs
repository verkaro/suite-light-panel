//! The sheet (jackbay's `docs/dev/SPEC.md`, 8.2): FROM, the verb and TO, each word picked in any
//! order, and the lines staged in amber until APPLY. Dexter's remote patch sheet, drawn on a
//! panel. The last of FROM and TO picked stages the line; FROM and the verb stay picked, so
//! several destinations can follow.
//!
//! Moved in from focusrite-ctrl's PATCH (`crates/panel/src/sheet.rs` and `draw.rs`, at
//! `052c497`), 2026-11-12, for jackbay, and made general: it knows words, not what they mean.
//! The tool makes the lists and says what's fed; the [`Picker`] holds the view, takes keys and
//! clicks, and gives back what was decided as a [`Did`], as the bar gives back an `Act`. It
//! never asks: whether APPLY wants a question first is the tool's.

use lp_paint::{Align, Hits, KeyLook, Pen, Rect, dark};

/// A word in FROM's or TO's list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Word {
    /// The tool's own, given back when it's picked.
    pub id: String,
    /// `Analogue 1·2`, `scape out_L/R`.
    pub label: String,
    /// Said small after the label, or empty: a client's name.
    pub small: String,
    /// ● before it: connected to what's picked in the other list.
    pub fed: bool,
    /// The ● in amber: it will be, or won't be, after APPLY.
    pub staged: bool,
}

/// A staged line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Line {
    pub words: String,
    /// `! ` before it, bold: it reaches a speaker, say.
    pub warn: bool,
}

/// A column, `[` `]` between them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Col {
    #[default]
    From,
    Verb,
    To,
    Staged,
}

const COLS: [Col; 4] = [Col::From, Col::Verb, Col::To, Col::Staged];

/// The verbs, in their column's order.
pub const VERBS: [&str; 3] = ["to", "cut", "only"];

/// The view: nothing here is the server's.
#[derive(Clone, Debug, Default)]
pub struct Picker {
    pub col: Col,
    /// Which filter each list shows.
    pub from_kind: usize,
    pub to_kind: usize,
    /// Each side alone, or in pairs.
    pub from_alone: bool,
    pub to_alone: bool,
    pub from_sel: usize,
    pub to_sel: usize,
    pub verb_sel: usize,
    pub staged_sel: usize,
    /// The ids picked.
    pub from: Option<String>,
    pub to: Option<String>,
    /// The verb picked; starts at 0, TO.
    pub verb: usize,
    /// The labels picked, for the sentence once the list no longer shows them.
    said: (String, String),
}

/// What the tool hands in each time: the lists as the picker's kinds and pairs ask for them.
#[derive(Clone, Copy, Debug)]
pub struct Lists<'a> {
    /// The section's label: `PATCH`.
    pub title: &'a str,
    pub from_kinds: &'a [&'a str],
    pub to_kinds: &'a [&'a str],
    pub from: &'a [Word],
    pub to: &'a [Word],
    pub staged: &'a [Line],
    /// Whether `p` is offered.
    pub pairs: bool,
    pub can_undo: bool,
}

/// What the picker decided, for the tool to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Did {
    /// Only the view changed.
    Moved,
    /// A kind or pairs changed: make the lists again.
    Lists,
    Stage {
        from: String,
        verb: &'static str,
        to: String,
    },
    Unstage(usize),
    Apply,
    Drop,
    Undo,
}

fn turn(i: usize, up: bool, n: usize) -> usize {
    if n == 0 {
        0
    } else if up {
        (i + 1) % n
    } else {
        (i + n - 1) % n
    }
}

impl Picker {
    pub fn new(from_alone: bool, to_alone: bool) -> Picker {
        Picker { from_alone, to_alone, ..Picker::default() }
    }

    /// A key, by its page name (`Key::name`). None: not the sheet's, the panel's to take.
    pub fn key(&mut self, name: &str, lists: &Lists) -> Option<Did> {
        let at = COLS.iter().position(|&c| c == self.col).unwrap_or(0);
        let len = self.len(self.col, lists);
        let sel = self.sel(self.col);
        let did = match name {
            "[" => {
                self.col = COLS[at.checked_sub(1)?];
                Did::Moved
            }
            "]" => {
                self.col = *COLS.get(at + 1)?;
                Did::Moved
            }
            "j" | "ArrowDown" => {
                *sel = (*sel + 1).min(len.saturating_sub(1));
                Did::Moved
            }
            "k" | "ArrowUp" => {
                *sel = sel.saturating_sub(1);
                Did::Moved
            }
            "H" => {
                *sel = 0;
                Did::Moved
            }
            "L" => {
                *sel = len.saturating_sub(1);
                Did::Moved
            }
            "h" | "l" | "ArrowLeft" | "ArrowRight" => {
                let up = matches!(name, "l" | "ArrowRight");
                match self.col {
                    Col::From => {
                        self.from_kind = turn(self.from_kind, up, lists.from_kinds.len());
                        self.from_sel = 0;
                        Did::Lists
                    }
                    Col::To => {
                        self.to_kind = turn(self.to_kind, up, lists.to_kinds.len());
                        self.to_sel = 0;
                        Did::Lists
                    }
                    Col::Verb => {
                        self.verb_sel = turn(self.verb_sel, up, VERBS.len());
                        Did::Moved
                    }
                    Col::Staged => Did::Moved,
                }
            }
            "p" if lists.pairs => match self.col {
                Col::From => self.toggle_pairs(true),
                Col::To => self.toggle_pairs(false),
                _ => Did::Moved,
            },
            "Enter" => match self.col {
                Col::From => self.pick_from(self.from_sel, lists),
                Col::To => self.pick_to(self.to_sel, lists),
                Col::Verb => self.pick_verb(self.verb_sel),
                Col::Staged => Did::Moved,
            },
            "x" => match self.col {
                Col::Staged if len > 0 => Did::Unstage(self.staged_sel.min(len - 1)),
                Col::From => {
                    self.from = None;
                    Did::Moved
                }
                Col::To => {
                    self.to = None;
                    Did::Moved
                }
                _ => Did::Moved,
            },
            "A" => Self::can(!lists.staged.is_empty(), Did::Apply),
            "D" => Self::can(!lists.staged.is_empty(), Did::Drop),
            "u" => Self::can(lists.can_undo, Did::Undo),
            "Escape" => {
                if self.from.is_none() && self.to.is_none() {
                    return None;
                }
                self.from = None;
                self.to = None;
                Did::Moved
            }
            _ => return None,
        };
        Some(did)
    }

    /// A click on one of the sheet's hits (`sheet:…`). None: not one of them.
    pub fn tap(&mut self, id: &str, lists: &Lists) -> Option<Did> {
        let rest = id.strip_prefix("sheet:")?;
        let (what, n) = match rest.split_once(':') {
            Some((what, n)) => (what, Some(n.parse::<usize>().ok()?)),
            None => (rest, None),
        };
        let did = match (what, n) {
            ("from", Some(n)) => {
                self.col = Col::From;
                self.from_sel = n;
                self.pick_from(n, lists)
            }
            ("to", Some(n)) => {
                self.col = Col::To;
                self.to_sel = n;
                self.pick_to(n, lists)
            }
            ("verb", Some(n)) if n < VERBS.len() => {
                self.col = Col::Verb;
                self.verb_sel = n;
                self.pick_verb(n)
            }
            ("fk", Some(n)) if n < lists.from_kinds.len() => {
                self.col = Col::From;
                self.from_kind = n;
                self.from_sel = 0;
                Did::Lists
            }
            ("tk", Some(n)) if n < lists.to_kinds.len() => {
                self.col = Col::To;
                self.to_kind = n;
                self.to_sel = 0;
                Did::Lists
            }
            ("fp", None) if lists.pairs => {
                self.col = Col::From;
                self.toggle_pairs(true)
            }
            ("tp", None) if lists.pairs => {
                self.col = Col::To;
                self.toggle_pairs(false)
            }
            ("staged", Some(n)) if n < lists.staged.len() => {
                self.col = Col::Staged;
                self.staged_sel = n;
                Did::Unstage(n)
            }
            ("apply", None) => Self::can(!lists.staged.is_empty(), Did::Apply),
            ("drop", None) => Self::can(!lists.staged.is_empty(), Did::Drop),
            ("undo", None) => Self::can(lists.can_undo, Did::Undo),
            _ => return None,
        };
        Some(did)
    }

    /// The line so far: `scape out_L/R TO …`.
    pub fn sentence(&self, lists: &Lists) -> String {
        let said = |pick: &Option<String>, words: &[Word], kept: &str| match pick {
            None => "…".to_string(),
            Some(id) => words.iter().find(|w| &w.id == id).map_or(kept.to_string(), |w| w.label.clone()),
        };
        let from = said(&self.from, lists.from, &self.said.0);
        let to = said(&self.to, lists.to, &self.said.1);
        format!("{from} {} {to}", VERBS[self.verb.min(VERBS.len() - 1)].to_uppercase())
    }

    /// Keeps each `sel` inside its list, after the lists change.
    pub fn settle(&mut self, lists: &Lists) {
        for col in COLS {
            let len = self.len(col, lists);
            let sel = self.sel(col);
            *sel = (*sel).min(len.saturating_sub(1));
        }
        self.from_kind = self.from_kind.min(lists.from_kinds.len().saturating_sub(1));
        self.to_kind = self.to_kind.min(lists.to_kinds.len().saturating_sub(1));
    }

    fn len(&self, col: Col, lists: &Lists) -> usize {
        match col {
            Col::From => lists.from.len(),
            Col::Verb => VERBS.len(),
            Col::To => lists.to.len(),
            Col::Staged => lists.staged.len(),
        }
    }

    fn sel(&mut self, col: Col) -> &mut usize {
        match col {
            Col::From => &mut self.from_sel,
            Col::Verb => &mut self.verb_sel,
            Col::To => &mut self.to_sel,
            Col::Staged => &mut self.staged_sel,
        }
    }

    /// A key that can't act now does nothing, as it's dimmed.
    fn can(ok: bool, did: Did) -> Did {
        if ok { did } else { Did::Moved }
    }

    fn toggle_pairs(&mut self, from: bool) -> Did {
        if from {
            self.from_alone = !self.from_alone;
            self.from_sel = 0;
        } else {
            self.to_alone = !self.to_alone;
            self.to_sel = 0;
        }
        Did::Lists
    }

    fn pick_from(&mut self, n: usize, lists: &Lists) -> Did {
        let Some(w) = lists.from.get(n) else { return Did::Moved };
        self.from = Some(w.id.clone());
        self.said.0 = w.label.clone();
        self.stage_if_whole()
    }

    fn pick_to(&mut self, n: usize, lists: &Lists) -> Did {
        let Some(w) = lists.to.get(n) else { return Did::Moved };
        self.to = Some(w.id.clone());
        self.said.1 = w.label.clone();
        self.stage_if_whole()
    }

    fn pick_verb(&mut self, v: usize) -> Did {
        self.verb = v;
        self.stage_if_whole()
    }

    /// FROM and TO both picked: the line is staged, and TO let go. Order is free: until both
    /// are, whichever is picked waits.
    fn stage_if_whole(&mut self) -> Did {
        match (&self.from, self.to.take()) {
            (Some(from), Some(to)) => Did::Stage { from: from.clone(), verb: VERBS[self.verb], to },
            (_, to) => {
                self.to = to;
                Did::Moved
            }
        }
    }

    /// The sheet inside `r`, in the dark skin. Laid out as focusrite-ctrl's, which is 928 across,
    /// scaled across to `r`'s width; rows keep their height, and as many are shown as fit.
    pub fn draw(&self, pen: &mut Pen, hits: &mut Hits, r: Rect, lists: &Lists) {
        let s = |n: i32| n * r.w / 928;
        let top = r.y;
        pen.rule(Rect::new(r.x, top, r.w, 16), lists.title);
        pen.text(
            Rect::new(r.x + s(184), top - 2, r.w - s(184), 18),
            &self.sentence(lists),
            11.0,
            true,
            dark::AMBER,
            Align::Right,
        );
        hits.section(lists.title);
        let row_h = 20;
        let list_top = top + 50;
        let rows = ((r.bottom() - list_top) / row_h).max(1);
        let lw = r.w / 4;
        let d = Drawing { top, list_top, rows, row_h, pairs_w: s(80) };

        let from = Col::From;
        d.list(pen, hits, self, r.x, lw, lists, from);
        // The verbs.
        let vx = r.x + lw + s(14);
        let vw = s(70);
        d.focus(pen, self, Rect::new(vx, list_top, vw, 3 * 30 - 6), Col::Verb);
        for (k, v) in VERBS.iter().enumerate() {
            let kr = Rect::new(vx, list_top + k as i32 * 30, vw, 24);
            let lit = (k == self.verb).then_some(dark::AMBER);
            if k == self.verb_sel && self.col == Col::Verb && lit.is_none() {
                pen.rrect(Rect::new(kr.x - 2, kr.y - 2, kr.w + 4, kr.h + 4), 4.0, dark::BG, dark::AMBER);
            }
            pen.key(
                hits,
                kr,
                &v.to_uppercase(),
                &format!("sheet:verb:{k}"),
                KeyLook { lit, ..KeyLook::default() },
            );
        }
        let tx = vx + vw + s(14);
        d.list(pen, hits, self, tx, lw, lists, Col::To);

        // The lines staged, in amber until APPLY; a click takes one back.
        let sx = tx + lw + s(18);
        let sw = r.right() - sx;
        pen.text(Rect::new(sx, top + 22, sw, 20), "STAGED", 9.0, true, dark::DIM, Align::Left);
        let area = Rect::new(sx, list_top, sw, rows * row_h - 34);
        d.focus(pen, self, area, Col::Staged);
        pen.rrect(area, 3.0, dark::PANEL, dark::RULE);
        if lists.staged.is_empty() {
            pen.text(area.inset(8), "nothing staged", 10.0, false, dark::FAINT, Align::Left);
        }
        let fit = (area.h / row_h).max(0) as usize;
        let first = self.staged_sel.saturating_sub(fit.saturating_sub(1));
        for (k, line) in lists.staged.iter().enumerate().skip(first).take(fit) {
            let lr = Rect::new(sx + 2, list_top + (k - first) as i32 * row_h + 1, sw - 4, row_h - 2);
            if k == self.staged_sel && self.col == Col::Staged {
                pen.fill(lr, dark::BTN_HI);
            }
            let words = if line.warn { format!("! {}", line.words) } else { line.words.clone() };
            let tr = Rect::new(lr.x + 6, lr.y, lr.w - 12, lr.h);
            pen.text(tr, &words, 10.0, line.warn, dark::AMBER, Align::Left);
            hits.add(lr, &format!("sheet:staged:{k}"));
        }
        let ky = area.bottom() + 8;
        let can_apply = !lists.staged.is_empty();
        let bw = (sw - 16) / 3;
        let key = |faint: bool, name: &'static str| KeyLook { faint, name: Some(name), ..KeyLook::default() };
        pen.key(hits, Rect::new(sx, ky, bw, 24), "APPLY  A", "sheet:apply", key(!can_apply, "apply"));
        pen.key(hits, Rect::new(sx + bw + 8, ky, bw, 24), "DROP  D", "sheet:drop", key(!can_apply, "drop"));
        let ur = Rect::new(sx + 2 * (bw + 8), ky, bw, 24);
        pen.key(hits, ur, "UNDO  u", "sheet:undo", key(!lists.can_undo, "undo"));
    }
}

/// Where the parts go, worked out once a draw.
struct Drawing {
    top: i32,
    list_top: i32,
    rows: i32,
    row_h: i32,
    pairs_w: i32,
}

impl Drawing {
    /// The hand's column: an amber edge round it.
    fn focus(&self, pen: &mut Pen, p: &Picker, r: Rect, col: Col) {
        if p.col == col {
            pen.rrect(Rect::new(r.x - 3, r.y - 3, r.w + 6, r.h + 6), 4.0, dark::BG, dark::AMBER);
        }
    }

    /// A list: its kinds as keys, its pairs key, and its rows, scrolled as little as keeps the
    /// hand on one.
    #[allow(clippy::too_many_arguments)]
    fn list(&self, pen: &mut Pen, hits: &mut Hits, p: &Picker, x: i32, w: i32, lists: &Lists, col: Col) {
        let (kinds, kind, kid, alone, pid, words, sel, picked, wid) = match col {
            Col::From => (
                lists.from_kinds,
                p.from_kind,
                "fk",
                p.from_alone,
                "fp",
                lists.from,
                p.from_sel,
                &p.from,
                "from",
            ),
            _ => (lists.to_kinds, p.to_kind, "tk", p.to_alone, "tp", lists.to, p.to_sel, &p.to, "to"),
        };
        let pw = if lists.pairs { self.pairs_w } else { 0 };
        let kw = if kinds.is_empty() { 0 } else { (w - pw - 2) / kinds.len() as i32 };
        for (k, name) in kinds.iter().enumerate() {
            let r = Rect::new(x + k as i32 * kw, self.top + 22, kw - 4, 20);
            // The list shown in plain text, with a line under it; the others dim.
            let color = Some(if k == kind { dark::TEXT } else { dark::DIM });
            if k == kind {
                pen.fill(Rect::new(r.x + 6, r.bottom() + 1, r.w - 12, 2), dark::TEXT);
            }
            pen.key(hits, r, name, &format!("sheet:{kid}:{k}"), KeyLook { color, ..KeyLook::default() });
        }
        if lists.pairs {
            let r = Rect::new(x + w - pw, self.top + 22, pw, 20);
            let said = if alone { "ONE  p" } else { "PAIRS  p" };
            pen.key(
                hits,
                r,
                said,
                &format!("sheet:{pid}"),
                KeyLook { name: Some("pairs"), ..KeyLook::default() },
            );
        }
        let area = Rect::new(x, self.list_top, w, self.rows * self.row_h);
        self.focus(pen, p, area, col);
        pen.rrect(area, 3.0, dark::PANEL, dark::RULE);
        let rows = self.rows as usize;
        let first = sel.saturating_sub(rows.saturating_sub(1));
        let here = p.col == col;
        for (k, word) in words.iter().enumerate().skip(first).take(rows) {
            let r =
                Rect::new(x + 2, self.list_top + (k - first) as i32 * self.row_h + 1, w - 4, self.row_h - 2);
            let is_picked = picked.as_deref() == Some(word.id.as_str());
            if is_picked {
                pen.fill(r, dark::AMBER);
            } else if k == sel && here {
                pen.fill(r, dark::BTN_HI);
            }
            let c = if is_picked { dark::ON_RED } else { dark::TEXT };
            let tr = Rect::new(r.x + 6, r.y, r.w - 12, r.h);
            let mut at = tr.x as f32;
            if word.fed || word.staged {
                let mc = if word.staged && !is_picked { dark::AMBER } else { c };
                pen.run(at, tr, "●", 10.0, false, mc);
            }
            at += pen.width("● ", 10.0, false);
            // The label, then `small` if there's room for both; else the label, cut short.
            let gap = pen.width(" ", 10.0, false);
            let both = pen.width(&word.label, 10.0, is_picked) + gap + pen.width(&word.small, 9.0, false);
            if word.small.is_empty() || at + both > tr.right() as f32 {
                let lr = Rect::new(at as i32, tr.y, tr.right() - at as i32, tr.h);
                pen.text(lr, &word.label, 10.0, is_picked, c, Align::Left);
            } else {
                at = pen.run(at, tr, &word.label, 10.0, is_picked, c) + gap;
                pen.run(at, tr, &word.small, 9.0, false, if is_picked { dark::ON_RED } else { dark::DIM });
            }
            hits.add(r, &format!("sheet:{wid}:{k}"));
        }
    }
}

#[cfg(test)]
mod tests;
