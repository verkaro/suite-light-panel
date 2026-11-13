//! Bytes in, grid out. Each screen is written row by row, a wide character once.

use super::*;

fn run(cols: usize, rows: usize, s: &str) -> Term {
    let mut t = Term::new(cols, rows, 100);
    t.feed(s.as_bytes());
    t
}

fn screen(t: &Term) -> Vec<String> {
    (0..t.rows()).map(|r| t.row(r).text()).collect()
}

fn at(t: &Term) -> (usize, usize) {
    let c = t.cursor();
    (c.row, c.col)
}

fn cell(t: &Term, row: usize, col: usize) -> Cell {
    t.row(row).cells()[col]
}

/// Every row whole, every wide character in its two halves, the cursor on screen.
fn check(t: &Term) {
    let s = &t.s;
    assert_eq!(s.lines.len(), s.rows);
    assert_eq!(s.other.len(), s.rows);
    assert_eq!(s.dirty.len(), s.rows);
    assert_eq!(s.tabs.len(), s.cols);
    assert!(s.row < s.rows && s.col < s.cols, "cursor {:?} in {}x{}", (s.row, s.col), s.cols, s.rows);
    assert!(s.top <= s.bottom && s.bottom < s.rows);
    assert!(s.scrollback.len() <= s.max_scrollback);
    for line in s.lines.iter().chain(&s.other) {
        let c = &line.cells;
        assert_eq!(c.len(), s.cols);
        for i in 0..c.len() {
            match c[i].width {
                Width::One => {}
                Width::Lead => assert!(i + 1 < c.len() && c[i + 1].width == Width::Tail, "{line:?}"),
                Width::Tail => assert!(i > 0 && c[i - 1].width == Width::Lead, "{line:?}"),
            }
        }
    }
}

/// 6 by 4, full, the cursor at row 1, column 2.
const FULL: &str = "abcdef\r\nghijkl\r\nmnopqr\r\nstuvwx\x1b[2;3H";

/// A capability's name, its bytes, the screen after them and the cursor.
type Case = (&'static str, &'static str, [&'static str; 4], (usize, usize));

/// What each of `infocmp linux`'s output capabilities does, the claim `TERM=linux` makes.
#[test]
fn linux_terminfo() {
    let cases: &[Case] = &[
        ("clear", "\x1b[H\x1b[J", ["      "; 4], (0, 0)),
        ("csr", "\x1b[2;3r", ["abcdef", "ghijkl", "mnopqr", "stuvwx"], (0, 0)),
        ("cub", "\x1b[2D", ["abcdef", "ghijkl", "mnopqr", "stuvwx"], (1, 0)),
        ("cub1", "\x08", ["abcdef", "ghijkl", "mnopqr", "stuvwx"], (1, 1)),
        ("cud", "\x1b[2B", ["abcdef", "ghijkl", "mnopqr", "stuvwx"], (3, 2)),
        ("cud1", "\n", ["abcdef", "ghijkl", "mnopqr", "stuvwx"], (2, 2)),
        ("cuf", "\x1b[2C", ["abcdef", "ghijkl", "mnopqr", "stuvwx"], (1, 4)),
        ("cuf1", "\x1b[C", ["abcdef", "ghijkl", "mnopqr", "stuvwx"], (1, 3)),
        ("cup", "\x1b[4;6H", ["abcdef", "ghijkl", "mnopqr", "stuvwx"], (3, 5)),
        ("cuu", "\x1b[1A", ["abcdef", "ghijkl", "mnopqr", "stuvwx"], (0, 2)),
        ("cuu1", "\x1b[A", ["abcdef", "ghijkl", "mnopqr", "stuvwx"], (0, 2)),
        ("dch", "\x1b[2P", ["abcdef", "ghkl  ", "mnopqr", "stuvwx"], (1, 2)),
        ("dch1", "\x1b[P", ["abcdef", "ghjkl ", "mnopqr", "stuvwx"], (1, 2)),
        ("dl", "\x1b[2M", ["abcdef", "stuvwx", "      ", "      "], (1, 0)),
        ("dl1", "\x1b[M", ["abcdef", "mnopqr", "stuvwx", "      "], (1, 0)),
        ("ech", "\x1b[2X", ["abcdef", "gh  kl", "mnopqr", "stuvwx"], (1, 2)),
        ("ed", "\x1b[J", ["abcdef", "gh    ", "      ", "      "], (1, 2)),
        ("el", "\x1b[K", ["abcdef", "gh    ", "mnopqr", "stuvwx"], (1, 2)),
        ("el1", "\x1b[1K", ["abcdef", "   jkl", "mnopqr", "stuvwx"], (1, 2)),
        ("home", "\x1b[H", ["abcdef", "ghijkl", "mnopqr", "stuvwx"], (0, 0)),
        ("hpa", "\x1b[5G", ["abcdef", "ghijkl", "mnopqr", "stuvwx"], (1, 4)),
        ("ht", "\t", ["abcdef", "ghijkl", "mnopqr", "stuvwx"], (1, 5)),
        ("hts", "\x1bH\r\t", ["abcdef", "ghijkl", "mnopqr", "stuvwx"], (1, 2)),
        ("tbc", "\x1b[3g\r\t", ["abcdef", "ghijkl", "mnopqr", "stuvwx"], (1, 5)),
        ("ich", "\x1b[2@", ["abcdef", "gh  ij", "mnopqr", "stuvwx"], (1, 2)),
        ("ich1", "\x1b[@", ["abcdef", "gh ijk", "mnopqr", "stuvwx"], (1, 2)),
        ("il", "\x1b[2L", ["abcdef", "      ", "      ", "ghijkl"], (1, 0)),
        ("il1", "\x1b[L", ["abcdef", "      ", "ghijkl", "mnopqr"], (1, 0)),
        ("ind", "\n", ["abcdef", "ghijkl", "mnopqr", "stuvwx"], (2, 2)),
        ("nel", "\r\n", ["abcdef", "ghijkl", "mnopqr", "stuvwx"], (2, 0)),
        ("ri", "\x1bM", ["abcdef", "ghijkl", "mnopqr", "stuvwx"], (0, 2)),
        ("cr", "\r", ["abcdef", "ghijkl", "mnopqr", "stuvwx"], (1, 0)),
        ("sc, rc", "\x1b7\x1b[H\x1b8", ["abcdef", "ghijkl", "mnopqr", "stuvwx"], (1, 2)),
        ("vpa", "\x1b[4d", ["abcdef", "ghijkl", "mnopqr", "stuvwx"], (3, 2)),
        ("smir", "\x1b[4hXY\x1b[4l", ["abcdef", "ghXYij", "mnopqr", "stuvwx"], (1, 4)),
        ("rmam", "\x1b[?7lXYZW", ["abcdef", "ghXYZW", "mnopqr", "stuvwx"], (1, 5)),
        ("am, xenl", "XYZW", ["abcdef", "ghXYZW", "mnopqr", "stuvwx"], (1, 5)),
        ("am", "XYZWV", ["abcdef", "ghXYZW", "Vnopqr", "stuvwx"], (2, 1)),
        ("smpch, rmpch", "\x1b[11mX\x1b[10m", ["abcdef", "ghXjkl", "mnopqr", "stuvwx"], (1, 3)),
        ("smacs, rmacs", "\x0eqx\x0fq", ["abcdef", "gh─│ql", "mnopqr", "stuvwx"], (1, 5)),
        ("acsc", "\x0e+,-.0`a", ["abcdef", "gh→←↑↓", "█◆▒pqr", "stuvwx"], (2, 3)),
    ];
    for (cap, bytes, rows, cursor) in cases {
        // `enacs` first, as ncurses sends it.
        let mut t = run(6, 4, &format!("\x1b)0{FULL}"));
        t.take_dirty();
        t.feed(bytes.as_bytes());
        assert_eq!(screen(&t), rows, "{cap}");
        assert_eq!(at(&t), *cursor, "{cap}");
        assert!(t.take_replies().is_empty(), "{cap} answered");
        check(&t);
    }

    // The attributes, and `sgr` as ncurses sends it for all of them, with the line drawing.
    let attrs = |s: &str| {
        let t = run(6, 1, &format!("\x1b)0{s}q"));
        let c = cell(&t, 0, 0);
        (c.attrs, c.fg, c.bg, c.ch)
    };
    let (u, b) = (Attrs::UNDERLINE, Attrs::BOLD);
    assert_eq!(attrs("\x1b[1m").0, b, "bold");
    assert_eq!(attrs("\x1b[2m").0, Attrs::DIM, "dim");
    assert_eq!(attrs("\x1b[5m").0, Attrs::BLINK, "blink");
    assert_eq!(attrs("\x1b[7m").0, Attrs::REVERSE, "rev, smso");
    assert_eq!(attrs("\x1b[7m\x1b[27m").0, Attrs::empty(), "rmso");
    assert_eq!(attrs("\x1b[4m").0, u, "smul");
    assert_eq!(attrs("\x1b[4m\x1b[24m").0, Attrs::empty(), "rmul");
    assert_eq!(attrs("\x1b[31m").1, Color::Index(1), "setaf");
    assert_eq!(attrs("\x1b[42m").2, Color::Index(2), "setab");
    assert_eq!(attrs("\x1b[31;42m\x1b[39;49m").1, Color::Default, "op");
    assert_eq!(attrs("\x1b[31;42m\x1b[39;49m").2, Color::Default, "op");
    assert_eq!(
        attrs("\x1b[1;4m\x0e\x1b[m\x0f"),
        (Attrs::empty(), Color::Default, Color::Default, 'q'),
        "sgr0"
    );
    let mut all = Attrs::empty();
    for a in [Attrs::REVERSE, u, Attrs::BLINK, Attrs::DIM, b] {
        all.insert(a);
    }
    assert_eq!(attrs("\x1b[0;10;7;4;7;5;2;1m\x0e"), (all, Color::Default, Color::Default, '─'), "sgr");
    assert_eq!(attrs("\x1b[0;10;4m\x0f"), (u, Color::Default, Color::Default, 'q'), "sgr");

    // The rest, which change what isn't a cell.
    let t = run(6, 4, "\x07\x07");
    assert_eq!(t.clone().take_bells(), 2, "bel");
    let mut t = run(6, 4, "\x1b[?25l\x1b[?1c");
    assert!(!t.modes().cursor_visible, "civis");
    t.feed(b"\x1b[?25h\x1b[?0c");
    assert!(t.modes().cursor_visible, "cnorm");
    t.feed(b"\x1b[?25l\x1b[?25h\x1b[?8c");
    assert!(t.modes().cursor_visible, "cvvis");
    assert!(t.take_replies().is_empty(), "the cursor's shape is never a question");
    t.feed(b"\x1b[?5h");
    assert!(t.modes().reverse, "flash");
    t.feed(b"\x1b[?5l");
    assert!(!t.modes().reverse, "flash");
    t.feed(b"\x1b[?7l\x1b[?7h\x1b[4h");
    assert!(t.modes().wrap && t.modes().insert, "smam, smir");
    t.feed(b"\x1b[4l");
    assert!(!t.modes().insert, "rmir");
    t.feed(b"\x1b[3;5H\x1b[6n");
    assert_eq!(t.take_replies(), b"\x1b[3;5R", "u7, answered as u6");
    t.feed(b"\x1b[c");
    assert_eq!(t.take_replies(), b"\x1b[?6c", "u9, answered as u8");
    t.feed(b"\x1b]P1ff8000");
    assert_eq!(t.palette()[1], Some([255, 128, 0]), "initc");
    t.feed(b"\x1b]R");
    assert_eq!(t.palette()[1], None, "oc");
    t.feed(b"\x1b]P2010203abc\x1bc\x1b]Rok");
    assert_eq!(screen(&t)[0], "ok    ", "rs1: what follows still prints");
    assert_eq!(t.palette()[2], None, "rs1");
}

#[test]
fn cursor_kept_on_screen() {
    let mut t = Term::new(5, 5, 0);
    for (bytes, want) in [
        ("\x1b[99;99H", (4, 4)),
        ("\x1b[0;0H", (0, 0)),
        ("\x1b[;3H", (0, 2)),
        ("\x1b[9A", (0, 2)),
        ("\x1b[9B", (4, 2)),
        ("\x1b[9C", (4, 4)),
        ("\x1b[9D", (4, 0)),
        ("\x08", (4, 0)),
        ("\x1b[2;3H\x1b[2E", (3, 0)),
        ("\x1b[2;3H\x1b[F", (0, 0)),
        ("\x1b[3;3H\x1b[2a", (2, 4)),
        ("\x1b[2e", (4, 4)),
        ("\x1b[2`", (4, 1)),
        ("\x1b[3;3f", (2, 2)),
        ("\x1b[0d", (0, 2)),
    ] {
        t.feed(bytes.as_bytes());
        assert_eq!(at(&t), want, "{bytes:?}");
    }
}

#[test]
fn origin_mode() {
    let mut t = run(5, 5, "\x1b[2;4r");
    assert_eq!(at(&t), (0, 0), "a region homes the cursor");
    t.feed(b"\x1b[?6h");
    assert_eq!(at(&t), (1, 0), "to the region's top");
    t.feed(b"\x1b[9;9H");
    assert_eq!(at(&t), (3, 4), "kept in the region");
    t.feed(b"\x1b[6n");
    assert_eq!(t.take_replies(), b"\x1b[3;5R", "counted from the region's top");
    t.feed(b"\x1b[9A");
    assert_eq!(at(&t), (1, 4));
    t.feed(b"\x1b[?6l");
    assert_eq!(at(&t), (0, 0));
    t.feed(b"\x1b[5;1H\x1b[9A");
    assert_eq!(at(&t), (1, 0), "up from below the region stops at its top");
    t.feed(b"\x1b[1;1H\x1b[9B");
    assert_eq!(at(&t), (3, 0), "down from above stops at its foot");
}

#[test]
fn erasing() {
    let t = run(6, 4, &format!("{FULL}\x1b[2J"));
    assert_eq!(screen(&t), ["      "; 4]);
    assert_eq!(at(&t), (1, 2), "ED doesn't move the cursor");
    let t = run(6, 4, &format!("{FULL}\x1b[1J"));
    assert_eq!(screen(&t), ["      ", "   jkl", "mnopqr", "stuvwx"]);
    let t = run(6, 4, &format!("{FULL}\x1b[2K"));
    assert_eq!(screen(&t), ["abcdef", "      ", "mnopqr", "stuvwx"]);
    let t = run(6, 4, &format!("{FULL}\x1b[9X"));
    assert_eq!(screen(&t)[1], "gh    ", "ECH stops at the edge");

    // `bce`: erasing, and the rows scrolled in, take the background.
    let t = run(4, 2, "\x1b[44m\x1b[2J");
    assert_eq!(cell(&t, 1, 3).bg, Color::Index(4));
    let t = run(4, 2, "\x1b[2;1H\x1b[41m\n");
    assert_eq!(cell(&t, 1, 0).bg, Color::Index(1));
    assert_eq!(cell(&t, 0, 0).bg, Color::Default);

    let mut t = run(4, 2, "1\r\n2\r\n3\r\n4");
    assert_eq!(t.scrollback_len(), 2);
    t.feed(b"\x1b[3J");
    assert_eq!(t.scrollback_len(), 0, "E3 clears the scrollback");
    assert_eq!(screen(&t), ["3   ", "4   "], "and only that");
}

#[test]
fn scroll_region() {
    let mut t = run(4, 4, "1\r\n2\r\n3\r\n4\x1b[2;3r\x1b[3;1H\nX");
    assert_eq!(screen(&t), ["1   ", "3   ", "X   ", "4   "], "LF at the region's foot");
    assert_eq!(t.scrollback_len(), 0, "a region below the top keeps no scrollback");
    t.feed(b"\x1b[2;1H\x1bMY");
    assert_eq!(screen(&t), ["1   ", "Y   ", "3   ", "4   "], "RI at its head");
    t.feed(b"\x1b[S");
    assert_eq!(screen(&t), ["1   ", "3   ", "    ", "4   "], "SU");
    t.feed(b"\x1b[2T");
    assert_eq!(screen(&t), ["1   ", "    ", "    ", "4   "], "SD");
    t.feed(b"\x1b[4;1H\nZ");
    assert_eq!(
        screen(&t),
        ["1   ", "    ", "    ", "Z   "],
        "below the region, the bottom row doesn't scroll"
    );
    t.feed(b"\x1b[3;2r");
    assert_eq!((t.s.top, t.s.bottom), (1, 2), "a region upside down is refused");
    t.feed(b"\x1b[r\x1b[S");
    assert_eq!(screen(&t), ["    ", "    ", "Z   ", "    "], "the whole screen again");
    assert_eq!(t.scrollback(t.scrollback_len() - 1).text(), "1   ", "into the scrollback");
}

#[test]
fn wrapping() {
    let mut t = run(4, 3, "abcd");
    assert_eq!(at(&t), (0, 3), "waits at the edge (xenl)");
    assert!(!t.row(0).wrapped());
    t.feed(b"e");
    assert_eq!(screen(&t), ["abcd", "e   ", "    "]);
    assert!(t.row(0).wrapped());
    assert_eq!(at(&t), (1, 1));

    let t = run(4, 3, "abcd\re");
    assert_eq!(screen(&t)[..2], ["ebcd", "    "], "CR ends the wait");
    let t = run(4, 3, "abcd\x08x");
    assert_eq!(screen(&t)[0], "abxd", "so does BS");
    let t = run(4, 3, "abcd\x1b[Ke");
    assert_eq!(screen(&t)[..2], ["abce", "    "], "and EL");
    let t = run(4, 3, "abcd\x1b7\x1b[H\x1b8e");
    assert_eq!(screen(&t)[1], "e   ", "DECRC brings the wait back");

    let mut t = run(4, 2, "abcdefgh");
    assert_eq!(t.scrollback_len(), 0);
    t.feed(b"i");
    assert_eq!(screen(&t), ["efgh", "i   "], "wrapping off the foot scrolls");
    assert_eq!(t.scrollback(0).text(), "abcd");
    assert!(t.scrollback(0).wrapped());
}

#[test]
fn alt_screen() {
    let mut t = run(4, 2, "ab\x1b[?1049h");
    assert!(t.modes().alt_screen);
    assert_eq!(screen(&t), ["    ", "    "]);
    assert_eq!(at(&t), (0, 2));
    t.feed(b"\x1b[31mxy\r\n1\r\n2\r\n3");
    assert_eq!(t.scrollback_len(), 0, "the alternate screen keeps none");
    t.feed(b"\x1b[H\x1b[?1049l");
    assert_eq!(screen(&t), ["ab  ", "    "]);
    assert_eq!(at(&t), (0, 2), "the cursor as it was");
    t.feed(b"c");
    assert_eq!(cell(&t, 0, 2).fg, Color::Default, "and the pen");
    t.feed(b"\x1b[?1049h");
    assert_eq!(screen(&t), ["    ", "    "], "cleared on the way in");

    let mut t = run(4, 1, "\x1b[?47hz\x1b[?47l");
    assert_eq!(screen(&t), ["    "]);
    t.feed(b"\x1b[?47h");
    assert_eq!(screen(&t), ["z   "], "47 doesn't clear");
    t.feed(b"\x1b[?1047l\x1b[?1047h");
    assert_eq!(screen(&t), ["    "], "1047 clears on the way out");
}

#[test]
fn colours() {
    let one = |s: &str| {
        let t = run(4, 1, &format!("{s}X"));
        let c = cell(&t, 0, 0);
        (c.fg, c.bg, c.attrs)
    };
    let i = Color::Index;
    let d = Color::Default;
    let mut bu = Attrs::BOLD;
    bu.insert(Attrs::UNDERLINE);
    assert_eq!(one("\x1b[1;4;31;42m"), (i(1), i(2), bu));
    assert_eq!(one("\x1b[91;102m"), (i(9), i(10), Attrs::empty()));
    assert_eq!(one("\x1b[38;5;200;48;5;17m"), (i(200), i(17), Attrs::empty()));
    assert_eq!(one("\x1b[38;2;1;2;3m"), (Color::Rgb(1, 2, 3), d, Attrs::empty()));
    assert_eq!(one("\x1b[38:2::4:5:6m"), (Color::Rgb(4, 5, 6), d, Attrs::empty()));
    assert_eq!(one("\x1b[48:2:7:8:9m"), (d, Color::Rgb(7, 8, 9), Attrs::empty()));
    assert_eq!(one("\x1b[38:5:9m"), (i(9), d, Attrs::empty()));
    assert_eq!(one("\x1b[38;5;1;1m"), (i(1), d, Attrs::BOLD), "on after a colour");
    assert_eq!(one("\x1b[38;5;999m"), (i(255), d, Attrs::empty()));
    assert_eq!(one("\x1b[38;5m"), (d, d, Attrs::empty()), "cut short");
    assert_eq!(one("\x1b[1;99;4m"), (d, d, bu), "an unknown one passed over alone");
    assert_eq!(one("\x1b[1;2;3;4;5;7;8;9m\x1b[22;23;24;25;27;28;29m"), (d, d, Attrs::empty()));
    assert_eq!(one("\x1b[4m\x1b[4:0m"), (d, d, Attrs::empty()));
    assert_eq!(one("\x1b[4:3m"), (d, d, Attrs::UNDERLINE));
    assert_eq!(one("\x1b[31;42m\x1b[m"), (d, d, Attrs::empty()));
    let t = run(4, 1, "\x1b[31mX\x1b[0mY");
    assert_eq!(cell(&t, 0, 1).fg, d);
}

#[test]
fn wide_characters() {
    let t = run(5, 2, "中a");
    assert_eq!(screen(&t), ["中a  ", "     "]);
    assert_eq!((cell(&t, 0, 0).width, cell(&t, 0, 1).width), (Width::Lead, Width::Tail));
    assert_eq!(at(&t), (0, 3));

    let t = run(5, 2, "abcd中");
    assert_eq!(screen(&t), ["abcd ", "中   "], "one cell left: on to the next row");
    assert!(t.row(0).wrapped());
    let mut t = run(4, 2, "ab中");
    assert_eq!(at(&t), (0, 3));
    t.feed(b"c");
    assert_eq!(screen(&t), ["ab中", "c   "]);
    let t = run(4, 1, "\x1b[?7labc中");
    assert_eq!(screen(&t), ["ab中"], "wrap off: the last two cells");
    let t = run(1, 1, "中");
    assert_eq!(screen(&t), [" "], "no room at all");

    for (bytes, want, why) in [
        ("中\rx", "x   ", "over the left half"),
        ("中\x1b[1;2Hx", " x  ", "over the right half"),
        ("中\x1b[1;2H\x1b[X", "    ", "half erased"),
        ("a中b\x1b[1;3H\x1b[P", "a b ", "half deleted"),
        ("中\x1b[1;2H\x1b[@", "    ", "split by an insert"),
        ("ab中\x1b[1;2H\x1b[@", "a b ", "pushed off the edge"),
    ] {
        let t = run(4, 1, bytes);
        assert_eq!(screen(&t), [want], "{why}");
        check(&t);
    }
}

#[test]
fn marks_join_the_cell_before() {
    let t = run(4, 1, "e\u{301}x");
    assert_eq!(screen(&t), ["e\u{301}x  "]);
    assert_eq!(cell(&t, 0, 0).marks, [Some('\u{301}'), None]);
    assert_eq!(at(&t), (0, 2));
    let t = run(4, 1, "中\u{301}");
    assert_eq!(cell(&t, 0, 0).marks[0], Some('\u{301}'), "the wide one's left half");
    let t = run(2, 1, "ab\u{301}");
    assert_eq!(cell(&t, 0, 1).marks[0], Some('\u{301}'), "waiting at the edge");
    let t = run(2, 1, "\u{301}");
    assert_eq!(screen(&t), ["  "], "nothing before it");
    let t = run(2, 1, "a\u{301}\u{302}\u{303}");
    assert_eq!(cell(&t, 0, 0).marks, [Some('\u{301}'), Some('\u{302}')], "two kept");
}

#[test]
fn charsets() {
    assert_eq!(screen(&run(3, 1, "\x1b(0lqk\x1b(B")), ["┌─┐"]);
    assert_eq!(screen(&run(3, 1, "\x1b)0\x0e\x1b7\x0f\x1b8q")), ["─  "], "DECSC keeps them");
    assert_eq!(screen(&run(3, 1, "\x1b)0\x0e\x1bcq")), ["q  "], "RIS resets them");
    assert_eq!(screen(&run(3, 1, "\x1b(0é")), ["é  "], "only ASCII is mapped");
}

#[test]
fn tabs() {
    for (bytes, col) in [
        ("\t", 8),
        ("\t\t", 16),
        ("\t\t\t", 19),
        ("\x1b[2I", 16),
        ("\x1b[1;4H\x1bH\r\t", 3),
        ("\x1b[1;9H\x1b[g\r\t", 16),
        ("\x1b[3g\t", 19),
        ("\x1b[1;12H\x1b[Z", 8),
        ("\x1b[1;12H\x1b[9Z", 0),
    ] {
        assert_eq!(at(&run(20, 1, bytes)).1, col, "{bytes:?}");
    }
}

#[test]
fn replies() {
    let mut t = run(6, 4, "\x1b[5n\x1b[0c\x1bZ\x1b[>c\x1b[?6n");
    assert_eq!(t.take_replies(), b"\x1b[0n\x1b[?6c\x1b[?6c", "and no answer to `>c` or `?6n`, as Linux");
    assert!(t.take_replies().is_empty(), "taken once");
}

#[test]
fn saving_the_cursor() {
    let t = run(4, 3, "\x1b[2;3H\x1b[1;31m\x1b7\x1b[H\x1b[0m\x1b8X");
    assert_eq!(at(&t), (1, 3));
    assert_eq!((cell(&t, 1, 2).attrs, cell(&t, 1, 2).fg), (Attrs::BOLD, Color::Index(1)));
    let t = run(4, 3, "\x1b[2;3H\x1b[s\x1b[H\x1b[uX");
    assert_eq!(screen(&t)[1], "  X ");
    let t = run(4, 3, "\x1b[2;3H\x1b8X");
    assert_eq!(screen(&t)[0], "X   ", "nothing saved: home");
}

#[test]
fn escapes() {
    let t = run(3, 2, "\x1b#8");
    assert_eq!(screen(&t), ["EEE", "EEE"], "DECALN");
    let t = run(3, 2, "a\x1b[2;1H\x1bDb");
    assert_eq!(screen(&t), ["   ", "b  "], "IND at the foot scrolls");
    let t = run(3, 2, "a\x1bEb");
    assert_eq!(screen(&t), ["a  ", "b  "], "NEL");
    let t = run(3, 2, "a\x1bMb");
    assert_eq!(screen(&t), [" b ", "a  "], "RI at the top scrolls down");
    let t = run(3, 2, "\x1b[20ha\nb");
    assert_eq!(screen(&t), ["a  ", "b  "], "LNM");
    let t = run(3, 2, "\x1b=");
    assert!(t.modes().app_keypad);
    let t = run(3, 2, "\x1b[?1h\x1b[?2004h");
    assert!(t.modes().app_cursor && t.modes().bracketed_paste);
    let t = run(3, 2, "\x1b[9;5]\x1b[3 qok");
    assert_eq!(screen(&t)[0], "ok ", "Linux's setterm and DECSCUSR pass quietly");
}

#[test]
fn title() {
    let mut t = run(4, 1, "\x1b]0;one\x07");
    assert_eq!(t.title(), "one");
    t.feed(b"\x1b]2;two\x1b\\");
    assert_eq!(t.title(), "two");
    t.feed(b"\x1b]1;icon\x07\x1b]2\x07");
    assert_eq!(t.title(), "two");
}

#[test]
fn dirty_rows() {
    let mut t = Term::new(4, 4, 0);
    assert_eq!(t.take_dirty(), [0, 1, 2, 3], "new: all of it");
    assert_eq!(t.take_dirty(), [] as [usize; 0], "taken");
    t.feed(b"\x1b[2;1Hx\x1b[4;1H");
    assert_eq!(t.take_dirty(), [1], "a cursor move dirties nothing");
    t.feed(b"\x1b[2;3r\x1b[3;1H\n");
    assert_eq!(t.take_dirty(), [1, 2], "a scroll, its region");
}

#[test]
fn scrollback_keeps_its_length() {
    let mut t = Term::new(4, 2, 3);
    t.feed(b"1\r\n2\r\n3\r\n4\r\n5\r\n6");
    assert_eq!(screen(&t), ["5   ", "6   "]);
    let kept: Vec<_> = (0..t.scrollback_len()).map(|i| t.scrollback(i).text()).collect();
    assert_eq!(kept, ["2   ", "3   ", "4   "]);
    let mut t = Term::new(4, 2, 0);
    t.feed(b"1\r\n2\r\n3");
    assert_eq!(t.scrollback_len(), 0);
}

#[test]
fn resizing() {
    let fill = "abcd\r\nefgh\r\nij";
    let mut t = run(4, 3, fill);
    t.resize(2, 3);
    assert_eq!(screen(&t), ["ab", "ef", "ij"]);
    assert_eq!(at(&t), (2, 1));
    check(&t);

    let mut t = run(4, 3, fill);
    t.resize(4, 2);
    assert_eq!(screen(&t), ["efgh", "ij  "], "from the top, keeping the cursor");
    assert_eq!(t.scrollback(0).text(), "abcd");
    assert_eq!(at(&t), (1, 2));

    let mut t = run(4, 3, &format!("{fill}\x1b[H"));
    t.resize(4, 2);
    assert_eq!(screen(&t), ["abcd", "efgh"], "from the bottom");
    t.resize(6, 3);
    assert_eq!(screen(&t), ["abcd  ", "efgh  ", "      "]);
    check(&t);

    let mut t = run(3, 1, "a中");
    t.resize(2, 1);
    assert_eq!(screen(&t), ["a "], "a wide one cut");

    let mut t = run(8, 4, "\x1b[2;3r\x1b[?1049h");
    t.resize(20, 2);
    assert_eq!((t.s.top, t.s.bottom), (0, 1), "the region whole again");
    t.feed(b"\x1b[?1049l\t\tx");
    assert_eq!(at(&t), (0, 17), "tab stops for the new columns");
    check(&t);
}

/// Mixed sequences, split anywhere, read the same: the family's "the same at any block size".
#[test]
fn any_split_reads_the_same() {
    let bytes = "ab\x1b[1;31m中\u{301}\r\n\x1b]2;tït\x07\x1b[2;3r\x1b)0\x0eqx\x0f\x1b[?1049hzz\x1b[?1049l\
                 \x1b]P1ff8000\x1b[38:2::1:2:3m😀é\x1b[6n\x1b[3;1H\x1b[2L\x1bM\x1b#8\x1b[4hXY\x1b[c"
        .as_bytes();
    let whole = {
        let mut t = Term::new(7, 5, 10);
        t.feed(bytes);
        format!("{t:?}")
    };
    let mut t = Term::new(7, 5, 10);
    for b in bytes {
        t.feed(std::slice::from_ref(b));
    }
    assert_eq!(format!("{t:?}"), whole, "a byte at a time");
    for at in 0..bytes.len() {
        let mut t = Term::new(7, 5, 10);
        t.feed(&bytes[..at]);
        t.feed(&bytes[at..]);
        assert_eq!(format!("{t:?}"), whole, "split at {at}");
    }
}

/// A seeded generator, so a failure repeats.
struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Random bytes, most of them pieces of sequences, at sizes down to a single cell: nothing
/// panics, and every row, wide character and cursor stays whole.
#[test]
fn random_bytes_break_nothing() {
    let pieces: &[&[u8]] = &[
        b"\x1b",
        b"[",
        b"?",
        b";",
        b":",
        b"0",
        b"1",
        b"2",
        b"5",
        b"9",
        b"38;5;",
        b"48:2::",
        b"1049",
        b"47",
        b"h",
        b"l",
        b"m",
        b"H",
        b"A",
        b"B",
        b"C",
        b"D",
        b"J",
        b"K",
        b"L",
        b"M",
        b"P",
        b"@",
        b"X",
        b"S",
        b"T",
        b"r",
        b"g",
        b"Z",
        b"I",
        b"s",
        b"u",
        b"7",
        b"8",
        b"c",
        b"n",
        b"\r",
        b"\n",
        b"\x08",
        b"\t",
        b"\x0e",
        b"\x0f",
        b"\x07",
        b"\x18",
        b"(0",
        b"#8",
        b"]",
        b"]P",
        b"]R",
        b"\x1b\\",
        b"a",
        b"xyz",
        "中".as_bytes(),
        "😀".as_bytes(),
        "\u{301}".as_bytes(),
        "é".as_bytes(),
        b"\xe2",
        b"\x9b",
        b"\xff",
    ];
    for (cols, rows) in [(1, 1), (2, 1), (1, 3), (2, 3), (3, 2), (7, 5), (80, 24)] {
        for seed in 1..=20u64 {
            let mut rng = XorShift(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (cols * 100 + rows) as u64);
            let mut bytes = Vec::new();
            for _ in 0..3000 {
                if rng.below(20) == 0 {
                    bytes.push(rng.next() as u8);
                } else {
                    bytes.extend_from_slice(pieces[rng.below(pieces.len())]);
                }
            }
            let mut t = Term::new(cols, rows, 5);
            let mut whole = t.clone();
            whole.feed(&bytes);
            let mut i = 0;
            while i < bytes.len() {
                let n = 1 + rng.below(64);
                let end = (i + n).min(bytes.len());
                t.feed(&bytes[i..end]);
                check(&t);
                i = end;
            }
            assert_eq!(format!("{t:?}"), format!("{whole:?}"), "{cols}x{rows} seed {seed}: in pieces");
            // And again, the size changing between pieces.
            let mut t = Term::new(cols, rows, 5);
            for chunk in bytes.chunks(97) {
                t.feed(chunk);
                t.resize(1 + rng.below(cols + 2), 1 + rng.below(rows + 2));
                check(&t);
            }
        }
    }
}
