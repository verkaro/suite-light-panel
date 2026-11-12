//! The picker by what it gives back, and by what's drawn into memory.

use super::*;
use lp_paint::{Fonts, Image, View};

fn words(prefix: &str, n: usize) -> Vec<Word> {
    (0..n)
        .map(|i| Word { id: format!("{prefix}{i}"), label: format!("{prefix} {i}"), ..Word::default() })
        .collect()
}

struct Fix {
    from: Vec<Word>,
    to: Vec<Word>,
    staged: Vec<Line>,
    can_undo: bool,
}

const FK: [&str; 2] = ["PROGRAMS", "INPUTS"];
const TK: [&str; 2] = ["OUTPUTS", "PROGRAMS"];

impl Fix {
    fn new() -> Fix {
        Fix { from: words("f", 3), to: words("t", 4), staged: vec![], can_undo: false }
    }
    fn lists(&self) -> Lists<'_> {
        Lists {
            title: "PATCH",
            from_kinds: &FK,
            to_kinds: &TK,
            from: &self.from,
            to: &self.to,
            staged: &self.staged,
            pairs: true,
            can_undo: self.can_undo,
        }
    }
}

fn keys(p: &mut Picker, f: &Fix, ks: &[&str]) -> Vec<Option<Did>> {
    ks.iter().map(|k| p.key(k, &f.lists())).collect()
}

fn stage(from: &str, verb: &'static str, to: &str) -> Option<Did> {
    Some(Did::Stage { from: from.into(), verb, to: to.into() })
}

#[test]
fn the_columns_and_their_ends() {
    let f = Fix::new();
    let mut p = Picker::new(false, false);
    assert_eq!(p.key("[", &f.lists()), None, "[ at FROM is the panel's");
    assert_eq!(keys(&mut p, &f, &["]", "]", "]"]), vec![Some(Did::Moved); 3]);
    assert_eq!(p.col, Col::Staged);
    assert_eq!(p.key("]", &f.lists()), None, "] at staged is the panel's");
    p.key("[", &f.lists());
    assert_eq!(p.col, Col::To);
}

#[test]
fn up_and_down_a_column_stop_at_its_ends() {
    let f = Fix::new();
    let mut p = Picker::new(false, false);
    keys(&mut p, &f, &["j", "j", "j", "j"]);
    assert_eq!(p.from_sel, 2);
    keys(&mut p, &f, &["k", "ArrowUp", "ArrowUp"]);
    assert_eq!(p.from_sel, 0);
    keys(&mut p, &f, &["L"]);
    assert_eq!(p.from_sel, 2);
    keys(&mut p, &f, &["H"]);
    assert_eq!(p.from_sel, 0);
    // In the verbs, and TO.
    keys(&mut p, &f, &["]", "j", "j", "j", "]", "L"]);
    assert_eq!((p.verb_sel, p.to_sel), (2, 3));
    // An empty list: nothing moves, nothing breaks.
    let e = Fix { from: vec![], to: vec![], ..Fix::new() };
    let mut p = Picker::new(false, false);
    for k in ["j", "k", "H", "L", "Enter", "x"] {
        assert_eq!(p.key(k, &e.lists()), Some(Did::Moved), "{k}");
    }
    assert_eq!(p.from_sel, 0);
    keys(&mut p, &e, &["]", "]", "]"]);
    assert_eq!(p.key("x", &e.lists()), Some(Did::Moved), "nothing staged to take back");
}

#[test]
fn kinds_turn_round_and_the_verbs_move() {
    let f = Fix::new();
    let mut p = Picker::new(false, false);
    p.from_sel = 2;
    assert_eq!(p.key("l", &f.lists()), Some(Did::Lists));
    assert_eq!((p.from_kind, p.from_sel), (1, 0));
    assert_eq!(p.key("ArrowRight", &f.lists()), Some(Did::Lists));
    assert_eq!(p.from_kind, 0, "round the end");
    assert_eq!(p.key("h", &f.lists()), Some(Did::Lists));
    assert_eq!(p.from_kind, 1);
    keys(&mut p, &f, &["]"]);
    assert_eq!(p.key("h", &f.lists()), Some(Did::Moved));
    assert_eq!(p.verb_sel, 2);
    keys(&mut p, &f, &["]", "l"]);
    assert_eq!(p.to_kind, 1);
    keys(&mut p, &f, &["]"]);
    assert_eq!(p.key("l", &f.lists()), Some(Did::Moved), "nothing in staged");
}

#[test]
fn pairs_turn_where_offered() {
    let f = Fix::new();
    let mut p = Picker::new(false, false);
    p.from_sel = 1;
    assert_eq!(p.key("p", &f.lists()), Some(Did::Lists));
    assert!(p.from_alone && !p.to_alone && p.from_sel == 0);
    keys(&mut p, &f, &["]"]);
    assert_eq!(p.key("p", &f.lists()), Some(Did::Moved), "not in the verbs");
    keys(&mut p, &f, &["]", "p"]);
    assert!(p.to_alone);
    let no = Lists { pairs: false, ..f.lists() };
    assert_eq!(p.key("p", &no), None, "not offered: the panel's");
}

#[test]
fn order_is_free() {
    let f = Fix::new();
    // FROM, then TO.
    let mut p = Picker::new(false, false);
    assert_eq!(p.key("Enter", &f.lists()), Some(Did::Moved));
    assert_eq!(p.sentence(&f.lists()), "f 0 TO …");
    keys(&mut p, &f, &["]", "]", "j"]);
    assert_eq!(p.key("Enter", &f.lists()), stage("f0", "to", "t1"));
    // FROM and the verb stay picked, TO is let go, so another destination can follow.
    assert_eq!((p.from.as_deref(), p.verb, p.to.as_deref()), (Some("f0"), 0, None));
    keys(&mut p, &f, &["j"]);
    assert_eq!(p.key("Enter", &f.lists()), stage("f0", "to", "t2"));

    // TO, then FROM.
    let mut p = Picker::new(false, false);
    keys(&mut p, &f, &["]", "]", "Enter"]);
    assert_eq!(p.sentence(&f.lists()), "… TO t 0");
    keys(&mut p, &f, &["[", "[", "j"]);
    assert_eq!(p.key("Enter", &f.lists()), stage("f1", "to", "t0"));

    // The verb picked last, with both picked: only reachable by a verb picked between.
    let mut p = Picker::new(false, false);
    keys(&mut p, &f, &["]", "j", "Enter"]);
    assert_eq!(p.verb, 1);
    keys(&mut p, &f, &["[", "Enter", "]", "]", "Enter"]);
    assert_eq!(p.from.as_deref(), Some("f0"));
    // A verb picked when FROM and TO are both held stages: hold TO by hand to show it.
    p.to = Some("t3".into());
    keys(&mut p, &f, &["["]);
    p.verb_sel = 2;
    assert_eq!(p.key("Enter", &f.lists()), stage("f0", "only", "t3"));
}

#[test]
fn x_and_esc_let_go() {
    let f = Fix::new();
    let mut p = Picker::new(false, false);
    keys(&mut p, &f, &["Enter"]);
    assert_eq!(p.key("x", &f.lists()), Some(Did::Moved));
    assert_eq!(p.from, None);
    assert_eq!(p.key("Escape", &f.lists()), None, "nothing picked: Esc backs out of the next thing");
    keys(&mut p, &f, &["Enter", "]", "]"]);
    p.to = Some("t1".into());
    assert_eq!(p.key("x", &f.lists()), Some(Did::Moved));
    assert!(p.to.is_none() && p.from.is_some());
    assert_eq!(p.key("Escape", &f.lists()), Some(Did::Moved));
    assert!(p.from.is_none() && p.to.is_none());
}

#[test]
fn staged_lines_apply_drop_and_undo() {
    let mut f = Fix::new();
    let mut p = Picker::new(false, false);
    // Nothing to act on: the keys do nothing, as they're dimmed.
    for k in ["A", "D", "u"] {
        assert_eq!(p.key(k, &f.lists()), Some(Did::Moved), "{k}");
    }
    f.staged =
        vec![Line { words: "a TO b".into(), warn: false }, Line { words: "c TO d".into(), warn: true }];
    f.can_undo = true;
    assert_eq!(p.key("A", &f.lists()), Some(Did::Apply));
    assert_eq!(p.key("D", &f.lists()), Some(Did::Drop));
    assert_eq!(p.key("u", &f.lists()), Some(Did::Undo));
    keys(&mut p, &f, &["]", "]", "]", "j", "j"]);
    assert_eq!(p.key("x", &f.lists()), Some(Did::Unstage(1)));
    assert_eq!(p.key("Enter", &f.lists()), Some(Did::Moved));
    assert_eq!(p.key("q", &f.lists()), None, "not the sheet's");
}

#[test]
fn every_kind_of_click() {
    let mut f = Fix::new();
    let mut p = Picker::new(false, false);
    assert_eq!(p.tap("sheet:from:2", &f.lists()), Some(Did::Moved));
    assert_eq!((p.col, p.from_sel, p.from.as_deref()), (Col::From, 2, Some("f2")));
    assert_eq!(p.tap("sheet:verb:1", &f.lists()), Some(Did::Moved));
    assert_eq!((p.col, p.verb), (Col::Verb, 1));
    assert_eq!(p.tap("sheet:to:3", &f.lists()), stage("f2", "cut", "t3"));
    assert_eq!(p.col, Col::To);
    assert_eq!(p.tap("sheet:fk:1", &f.lists()), Some(Did::Lists));
    assert_eq!((p.col, p.from_kind), (Col::From, 1));
    assert_eq!(p.tap("sheet:tk:1", &f.lists()), Some(Did::Lists));
    assert_eq!(p.to_kind, 1);
    assert_eq!(p.tap("sheet:fp", &f.lists()), Some(Did::Lists));
    assert_eq!(p.tap("sheet:tp", &f.lists()), Some(Did::Lists));
    assert!(p.from_alone && p.to_alone);
    assert_eq!(p.tap("sheet:apply", &f.lists()), Some(Did::Moved), "nothing staged");
    assert_eq!(p.tap("sheet:undo", &f.lists()), Some(Did::Moved));
    f.staged = vec![Line { words: "a TO b".into(), warn: false }];
    f.can_undo = true;
    assert_eq!(p.tap("sheet:staged:0", &f.lists()), Some(Did::Unstage(0)));
    assert_eq!(p.tap("sheet:apply", &f.lists()), Some(Did::Apply));
    assert_eq!(p.tap("sheet:drop", &f.lists()), Some(Did::Drop));
    assert_eq!(p.tap("sheet:undo", &f.lists()), Some(Did::Undo));
    for id in ["bar:yes", "sheet:staged:5", "sheet:fk:9", "sheet:from:x", "sheet:nothing"] {
        assert_eq!(p.tap(id, &f.lists()), None, "{id}");
    }
}

#[test]
fn the_sentence_keeps_a_pick_its_list_no_longer_shows() {
    let mut f = Fix::new();
    let mut p = Picker::new(false, false);
    p.tap("sheet:from:1", &f.lists());
    f.from = words("g", 2);
    assert_eq!(p.sentence(&f.lists()), "f 1 TO …");
}

#[test]
fn settle_keeps_the_hand_inside_a_list_that_shrank() {
    let mut f = Fix::new();
    let mut p = Picker::new(false, false);
    p.from_sel = 2;
    p.to_sel = 3;
    p.staged_sel = 4;
    f.from = words("f", 1);
    f.to = vec![];
    p.settle(&f.lists());
    assert_eq!((p.from_sel, p.to_sel, p.staged_sel), (0, 0, 0));
}

fn drawn(p: &Picker, f: &Fix, w: i32, h: i32, hits: &mut Hits) -> Option<Image> {
    let fs = Fonts::terminus(None).ok()?;
    let mut img = Image::new(w as usize + 40, h as usize + 40);
    let mut sf = img.surface();
    let mut pen =
        Pen { sf: &mut sf, fonts: &fs, v: View::fit(w + 40, h + 40, w as usize + 40, h as usize + 40) };
    p.draw(&mut pen, hits, Rect::new(20, 20, w, h), &f.lists());
    Some(img)
}

#[test]
fn drawn_inside_its_box_at_two_sizes() {
    let mut f = Fix::new();
    f.from = words("f", 40);
    f.staged = vec![Line { words: "f 0 TO t 1".into(), warn: true }];
    f.from[1].fed = true;
    f.from[2].staged = true;
    f.from[3].small = "scape".into();
    let mut p = Picker::new(false, false);
    p.tap("sheet:from:0", &f.lists());
    p.from_sel = 30;
    for (w, h) in [(928, 456), (464, 228)] {
        let mut hits = Hits::new();
        let Some(img) = drawn(&p, &f, w, h, &mut hits) else { return };
        let r = Rect::new(20, 20, w, h);
        for hit in hits.iter() {
            assert!(r.contains(hit.r), "{} at {:?} outside {r:?} ({w}×{h})", hit.id, hit.r);
        }
        assert!(hits.find("sheet:from:30").is_some(), "scrolled to keep the hand in view");
        assert!(hits.find("sheet:staged:0").is_some() && hits.find("sheet:apply").is_some());
        assert_eq!(hits.find("sheet:apply").map(|h| h.faint), Some(false));
        assert_eq!(hits.find("sheet:undo").map(|h| h.faint), Some(true));
        assert!(img.px.contains(&dark::AMBER));
    }
    // A picked word's row is filled amber.
    p.from_sel = 0;
    let mut hits = Hits::new();
    let Some(img) = drawn(&p, &f, 928, 456, &mut hits) else { return };
    let row = hits.find("sheet:from:0").unwrap().r;
    let (x, y) = (row.right() - 3, row.y + row.h / 2);
    assert_eq!(img.px[y as usize * img.w + x as usize], dark::AMBER);
    let other = hits.find("sheet:from:5").unwrap().r;
    assert_ne!(img.px[(other.y + other.h / 2) as usize * img.w + (other.right() - 3) as usize], dark::AMBER);
    // Bar names for the keys.
    let names: Vec<&str> = hits.iter().filter(|h| !h.name.is_empty()).map(|h| h.name.as_str()).collect();
    for n in ["programs", "inputs", "pairs", "to", "cut", "only", "outputs", "apply", "drop", "undo"] {
        assert!(names.contains(&n), "{n} in {names:?}");
    }
}
