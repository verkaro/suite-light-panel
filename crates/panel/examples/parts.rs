//! Every part a panel is made of, drawn once to a PNG, for whoever changes one to look at:
//!
//! ```sh
//! cargo run -p lp-panel --example parts -- /tmp/parts.png [1920x1080]
//! ```
//!
//! A picture proves the drawing, not a screen.

use lp_keys::common::{Declared, KEYS};
use lp_paint::{
    Align, Cell, Column, Dial, Disc, Field, FieldState, Fonts, Hits, Image, KeyLook, List, ListLook, Marker,
    Pen, Rect, Scale, View, Wave, dark, knob_pos, write_png,
};
use lp_panel::bar::{Bar, Opt};
use lp_panel::keys::keys_over;
use lp_panel::sheet::{Line, Lists, Picker, Word};
use lp_panel::snapshot::keys_of;

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or("parts PNG [WxH]")?;
    let size = args.next().unwrap_or("1400x1340".into());
    let (w, h) = size.split_once('x').ok_or("the size is WxH")?;
    let (w, h): (usize, usize) = (w.parse().map_err(|_| "W")?, h.parse().map_err(|_| "H")?);
    let fonts = Fonts::terminus(None).map_err(|e| e.to_string())?;
    let mut img = Image::new(w, h);
    let mut sf = img.surface();
    sf.clear(dark::PAGE);
    let mut pen = Pen { sf: &mut sf, fonts: &fonts, v: View::fit(1400, 1340, w, h) };
    let mut hits = Hits::new();
    pen.fill(Rect::new(0, 0, 1400, 1340), dark::BG);

    // A list, the hand on it.
    pen.fill(Rect::new(10, 10, 560, 400), dark::PANEL);
    pen.rule(Rect::new(20, 14, 540, 16), "LIBRARY");
    let cols = [
        Column { head: "NAME", w: 0, align: Align::Left },
        Column { head: "KIND", w: 90, align: Align::Left },
        Column { head: "RT", w: 50, align: Align::Right },
        Column { head: "LEN", w: 50, align: Align::Right },
        Column { head: "", w: 14, align: Align::Centre },
    ];
    let look = ListLook { row_h: 20, size: 12.0, empty: "nothing found" };
    let area = Rect::new(10, 34, 560, 370);
    let mut list = List::default();
    list.keep(9, 60, List::rows(area, &look, &cols));
    let row = |i: usize| {
        vec![
            Cell::new(format!("room {i:02}, the stairwell behind the old hall")),
            Cell::colored("stairwell", dark::DIM),
            Cell::new(format!("{:.2}", 0.2 + i as f32 * 0.07)),
            Cell::colored(format!("{:.1}", 0.5 + i as f32 * 0.1), dark::DIM),
            if i % 4 == 1 { Cell::colored("•", dark::AMBER) } else { Cell::default() },
        ]
    };
    list.draw(&mut pen, area, &look, &cols, 60, Some(9), true, row, Some((&mut hits, "row")));

    // A form: plain, empty, under the cursor, waiting, being typed in.
    pen.fill(Rect::new(580, 10, 400, 190), dark::PANEL);
    pen.rule(Rect::new(590, 14, 380, 16), "WHAT'S SAID");
    let field = |label, value, waiting, state| Field {
        label,
        value,
        hint: "not said",
        waiting,
        state,
        label_w: 80,
        size: 12.0,
    };
    let fields = [
        field("NAME", "oak hall, from the gallery", false, FieldState::Plain),
        field("PLACE", "", false, FieldState::Plain),
        field("KIND", "hall", false, FieldState::Cursor),
        field("SIZE", "large", true, FieldState::Plain),
        field("NOTES", "the doors were open, and it ra", false, FieldState::Typing { caret: 30 }),
    ];
    for (i, f) in fields.iter().enumerate() {
        pen.field(Rect::new(590, 38 + 30 * i as i32, 380, 24), f);
    }

    // Keys: plain, lit, not for now.
    hits.section("ROOM");
    pen.key(&mut hits, Rect::new(990, 14, 90, 26), "KEEP", "keep", KeyLook::default());
    pen.key(
        &mut hits,
        Rect::new(1090, 14, 90, 26),
        "PLAY",
        "play",
        KeyLook { lit: Some(dark::GREEN), ..KeyLook::default() },
    );
    pen.key(
        &mut hits,
        Rect::new(1190, 14, 90, 26),
        "EXPORT",
        "export",
        KeyLook { faint: true, ..KeyLook::default() },
    );
    hits.section("");
    pen.key(
        &mut hits,
        Rect::new(1290, 14, 90, 26),
        "? KEYS",
        "keys",
        KeyLook { name: Some("keys"), ..KeyLook::default() },
    );

    // Dials, a mix's gain (−80 to +12) with 0 dB marked: off and silent, a quiet signal, a full
    // one under the hand, a caution, a clip latched, a line on the interface's knob, and one
    // being typed in. The last two have no meter, so no ring.
    let zero = Some(knob_pos(Some(0.0), -80.0, 12.0));
    let dial = |label, value, db: Option<f64>, ring| Dial {
        label,
        value,
        pos: knob_pos(db, -80.0, 12.0),
        mark: zero,
        ring,
        ..Dial::default()
    };
    let dials = [
        dial("ADAT 3", "OFF", None, Some(Disc::Off)),
        dial("ANALOGUE 1", "-20.0", Some(-20.0), Some(Disc::Signal(0.4))),
        Dial { cursor: true, ..dial("PCM 1", "0.0", Some(0.0), Some(Disc::Signal(1.0))) },
        dial("ANALOGUE 2", "+4.0", Some(4.0), Some(Disc::Caution)),
        dial("S/PDIF 1", "+12.0", Some(12.0), Some(Disc::Clip)),
        Dial { held: true, ..dial("MONITOR L", "-12.0", Some(-12.0), None) },
        Dial { typing: Some("-6.5"), ..dial("MIX A", "-3.0", Some(-3.0), None) },
        Dial { inner: Some(Disc::Caution), ..dial("PCM 2", "+6.0", Some(6.0), Some(Disc::Signal(0.8))) },
    ];
    hits.section("MIX A");
    for (i, d) in dials.iter().enumerate() {
        pen.dial(&mut hits, Rect::new(994 + 50 * i as i32, 52, 48, 100), d, &format!("dial{i}"));
    }
    hits.section("");

    // A waveform, straight and in dB, of a click and a fall from it.
    let n = 1200;
    let hi: Vec<f32> = (0..n)
        .map(|k| {
            let t = k as f32 / n as f32;
            let wobble = 0.6 + 0.4 * ((k * 7919) % 97) as f32 / 97.0;
            if k < 6 { 1.0 } else { wobble * 10f32.powf(-3.2 * t) * if k < 150 { 0.5 } else { 1.0 } }
        })
        .collect();
    let lo: Vec<f32> = hi.iter().map(|v| -v * 0.9).collect();
    let curve: Vec<Option<f32>> =
        (0..n).map(|k| (k >= 6).then(|| -64.0 * (k - 6) as f32 / n as f32)).collect();
    let markers = [
        Marker { at: 3.0, label: "ONSET", color: dark::GREEN },
        Marker { at: 75.0, label: "TAIL", color: dark::AMBER },
    ];
    let wave = Wave {
        lo: &lo,
        hi: &hi,
        from: 0.0,
        to: 600.0,
        unit: "ms",
        scale: Scale::Linear,
        curve: &curve,
        markers: &markers,
    };
    pen.wave(Rect::new(580, 210, 810, 190), &wave);
    pen.wave(Rect::new(10, 420, 1380, 240), &Wave { scale: Scale::Db { floor: -90.0 }, ..wave });

    // The bar, each way it's used, and closed.
    let options = Bar::screen(&hits, vec![Opt::new("quit", "quit")]);
    let mut bars: [Bar; 6] = std::array::from_fn(|_| Bar::new());
    bars[1].ask("discard room 09? it isn't confirmed", "discard");
    bars[2].offer("mark 7", 'i', "note", "note");
    bars[3].type_in("search", "kind:hall rt:>1", 0, "search");
    bars[4].choose(options.clone());
    bars[5].choose(options);
    for k in keys_of("room{enter}ex{enter}").map_err(|e| e.to_string())? {
        bars[5].key(&k);
    }
    for (i, bar) in bars.iter().enumerate() {
        bar.draw(
            &mut pen,
            &mut hits,
            Rect::new(10, 670 + 31 * i as i32, 1380, 26),
            "?  the keys     :  what this screen does",
        );
    }

    // The sheet: a source picked, one destination fed, a line staged to a speaker.
    let word = |id: &str, label: &str, small: &str, fed: bool, staged: bool| Word {
        id: id.into(),
        label: label.into(),
        small: small.into(),
        fed,
        staged,
    };
    let from = [
        word("0", "out_L/R", "scape", false, false),
        word("1", "out_1/2", "brr", false, false),
        word("2", "a very long name for a port, longer than its row", "x", false, false),
    ];
    let to =
        [word("0", "playback_1/2", "system", true, false), word("1", "playback_3/4", "system", false, true)];
    let staged = [
        Line { words: "scape out_L/R TO playback_3/4".into(), warn: true },
        Line { words: "brr out_1/2 CUT playback_1/2".into(), warn: false },
    ];
    let lists = Lists {
        title: "PATCH",
        from_kinds: &["PROGRAMS", "INPUTS"],
        to_kinds: &["OUTPUTS", "PROGRAMS"],
        from: &from,
        to: &to,
        staged: &staged,
        pairs: true,
        can_undo: false,
    };
    let mut picker = Picker::new(false, false);
    picker.tap("sheet:from:0", &lists);
    picker.key("]", &lists);
    picker.key("]", &lists);
    picker.key("j", &lists);
    pen.fill(Rect::new(10, 866, 1380, 470), dark::PANEL);
    picker.draw(&mut pen, &mut hits, Rect::new(236, 876, 928, 456), &lists);

    if std::env::var_os("PARTS_KEYS").is_some() {
        let list: Vec<Declared> = KEYS
            .iter()
            .map(|(k, m)| Declared {
                key: k.to_string(),
                does: m.to_string(),
                screen: "everywhere".into(),
                ..Declared::default()
            })
            .collect();
        keys_over(&mut pen, &list, &["everywhere"], Rect::new(200, 120, 1000, 560));
    }
    write_png(std::path::Path::new(&path), &sf).map_err(|e| e.to_string())?;
    println!("{path} ({w}×{h})");
    Ok(())
}
