//! A panel tested without a screen: connected to its server as ever, given clicks, drags and
//! keys as a hand would, then drawn once to a PNG. A tool's `--snapshot PNG` with `--size`,
//! `--click`, `--drag` and `--keys`, so its tests (and whoever changes it) see what it shows.
//!
//! The panel says how long to wait for its server ([`Headless::settle`]); the script is
//! played through the same [`Panel`] methods the screen's loop calls. Moved here from
//! dexter-kms's `main.rs` (2026-11-07), for Tape's panel and the ones after it.

use crate::{Ctl, Panel};
use lp_keys::{Button, Key, Mods, Pointer, State, code};
use lp_paint::Image;
use std::path::Path;
use std::time::Instant;

/// What a panel waits for between the script's steps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Settle {
    /// Before anything: the server's first state.
    Start,
    /// After a click or a drag, before the next step: time for the server to answer it.
    Answer,
    /// After a key: a moment.
    Key,
}

/// A [`Panel`] that can be run without a screen.
pub trait Headless: Panel {
    /// Waits for the server, taking what it sends, as `s` asks. An error if it doesn't come.
    fn settle(&mut self, s: Settle) -> Result<(), String>;
    /// After each click: a box it opened filled in, say.
    fn clicked(&mut self) {}
    /// After the last key: anything left waiting for another key done.
    fn keys_done(&mut self) {}
}

/// What's done before the snapshot is drawn, in this order: the clicks, the drags, the keys.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Script {
    /// The screen's size, in pixels.
    pub size: (usize, usize),
    /// Each a press and a release where it is, in screen pixels.
    pub clicks: Vec<(i32, i32)>,
    /// Each a press, a move and a release: from (x, y) to (x2, y2).
    pub drags: Vec<[i32; 4]>,
    /// Each pressed and let go; given at all (even none), the end waits for an answer too.
    pub keys: Option<Vec<Key>>,
}

impl Script {
    /// From a tool's options: `--size WxH`, each `--click X,Y`, each `--drag X,Y,X2,Y2`, and
    /// `--keys` (see [`keys_of`]).
    pub fn parse(
        size: &str,
        clicks: &[String],
        drags: &[String],
        keys: Option<&str>,
    ) -> Result<Script, String> {
        let size = size
            .split_once('x')
            .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
            .ok_or("--size is WxH, e.g. 1920x1080")?;
        let clicks = clicks
            .iter()
            .map(|c| {
                c.split_once(',')
                    .and_then(|(x, y)| Some((x.trim().parse().ok()?, y.trim().parse().ok()?)))
                    .ok_or_else(|| "--click is X,Y, e.g. 400,300".to_string())
            })
            .collect::<Result<_, _>>()?;
        let drags = drags
            .iter()
            .map(|d| {
                let v: Vec<i32> = d.split(',').filter_map(|n| n.trim().parse().ok()).collect();
                <[i32; 4]>::try_from(v).map_err(|_| "--drag is X,Y,X2,Y2".to_string())
            })
            .collect::<Result<_, _>>()?;
        let keys = keys.map(keys_of).transpose()?;
        Ok(Script { size, clicks, drags, keys })
    }
}

/// Keys as typed: each character one, with Shift where a US keyboard needs it, or a key
/// named in braces: `{enter}` `{esc}` `{tab}` `{backspace}` `{delete}` `{left}` `{right}`
/// `{up}` `{down}` `{home}` `{end}` `{pgup}` `{pgdn}`. Each a press.
pub fn keys_of(s: &str) -> Result<Vec<Key>, String> {
    let mut out = Vec::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        let (code, shift, ch) = if c == '{' {
            let name: String = it.by_ref().take_while(|&c| c != '}').collect();
            let code = match name.as_str() {
                "enter" => code::ENTER,
                "esc" => code::ESC,
                "tab" => code::TAB,
                "backspace" => code::BACKSPACE,
                "delete" => code::DELETE,
                "home" => code::HOME,
                "end" => code::END,
                "pgup" => code::PAGEUP,
                "pgdn" => code::PAGEDOWN,
                "left" => code::LEFT,
                "right" => code::RIGHT,
                "up" => code::UP,
                "down" => code::DOWN,
                _ => return Err(format!("--keys: no key {{{name}}}")),
            };
            (code, false, None)
        } else {
            let (code, shift) = lp_keys::common::code_of(c).ok_or(format!("--keys: no key for {c:?}"))?;
            (code, shift, Some(c))
        };
        out.push(Key { code, state: State::Press, mods: Mods { shift, ..Default::default() }, ch });
    }
    Ok(out)
}

/// A character's key on a US keyboard, whether or not it takes Shift.
pub fn key_code(c: char) -> Option<u16> {
    lp_keys::common::code_of(c).map(|(code, _)| code)
}

/// Plays `script` on `p`, then draws it to the PNG at `path`, the pointer's arrow where the
/// last click or drag ended. What to say of it: "PATH (W×H, drawn in N ms)".
pub fn snapshot(p: &mut impl Headless, script: &Script, path: &Path) -> Result<String, String> {
    let (w, h) = script.size;
    let press = |x, y| Pointer::Press { x, y, button: Button::Left };
    let release = |x, y| Pointer::Release { x, y, button: Button::Left };
    p.settle(Settle::Start)?;
    let mut last = None;
    for &(x, y) in &script.clicks {
        // Each after the answer to the one before has had time to come.
        if last.is_some() {
            p.settle(Settle::Answer)?;
        }
        p.point(press(x, y), w, h);
        p.point(release(x, y), w, h);
        last = Some((x, y));
        p.clicked();
    }
    for &[x, y, x2, y2] in &script.drags {
        p.settle(Settle::Answer)?;
        p.point(press(x, y), w, h);
        p.point(Pointer::Move { x: x2, y: y2 }, w, h);
        p.point(release(x2, y2), w, h);
        last = Some((x2, y2));
    }
    for k in script.keys.iter().flatten() {
        for state in [State::Press, State::Release] {
            p.key(&Key { state, ..*k }, &mut Ctl::default());
        }
        p.settle(Settle::Key)?;
    }
    p.keys_done();
    if last.is_some() || script.keys.is_some() {
        p.settle(Settle::Answer)?;
    }
    let mut img = Image::new(w, h);
    let t = Instant::now();
    p.draw(&mut img.surface());
    let took = t.elapsed();
    if let Some((x, y)) = last {
        lp_paint::cursor(&mut img.surface(), x, y, (h as i32 / 700).max(1));
    }
    lp_paint::write_png(path, &img.surface()).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(format!("{} ({w}×{h}, drawn in {:.1} ms)", path.display(), took.as_secs_f64() * 1000.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_paint::Surface;

    /// Everything done to it, in order.
    #[derive(Default)]
    struct Fake {
        log: Vec<String>,
        fail_at: Option<usize>,
    }

    impl Panel for Fake {
        fn take(&mut self) -> bool {
            false
        }
        fn key(&mut self, k: &Key, _: &mut Ctl) -> bool {
            self.log.push(format!("key {} {:?} {}", k.code, k.state, k.mods.shift));
            true
        }
        fn point(&mut self, e: Pointer, w: usize, h: usize) -> bool {
            self.log.push(format!("{e:?} {w}x{h}"));
            true
        }
        fn draw(&mut self, s: &mut Surface) {
            self.log.push("draw".into());
            s.clear(0x123456);
        }
    }

    impl Headless for Fake {
        fn settle(&mut self, s: Settle) -> Result<(), String> {
            if self.fail_at == Some(self.log.len()) {
                return Err("nothing came".into());
            }
            self.log.push(format!("{s:?}"));
            Ok(())
        }
        fn clicked(&mut self) {
            self.log.push("clicked".into());
        }
        fn keys_done(&mut self) {
            self.log.push("keys done".into());
        }
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("lp-panel-{}-{name}.png", std::process::id()))
    }

    #[test]
    fn a_script_plays_in_order_and_draws() {
        let s =
            Script::parse("40x30", &["1,2".into(), "3, 4".into()], &["5,6,7,8".into()], Some("aA{enter}"))
                .unwrap();
        let mut f = Fake::default();
        let png = scratch("order");
        let said = snapshot(&mut f, &s, &png).unwrap();
        assert!(said.ends_with(" ms)") && said.contains("(40×30, drawn in"), "{said}");
        let png_bytes = std::fs::read(&png).unwrap();
        std::fs::remove_file(&png).unwrap();
        assert!(png_bytes.starts_with(b"\x89PNG"));
        let want = [
            "Start",
            "Press { x: 1, y: 2, button: Left } 40x30",
            "Release { x: 1, y: 2, button: Left } 40x30",
            "clicked",
            "Answer",
            "Press { x: 3, y: 4, button: Left } 40x30",
            "Release { x: 3, y: 4, button: Left } 40x30",
            "clicked",
            "Answer",
            "Press { x: 5, y: 6, button: Left } 40x30",
            "Move { x: 7, y: 8 } 40x30",
            "Release { x: 7, y: 8, button: Left } 40x30",
            "key 30 Press false",
            "key 30 Release false",
            "Key",
            "key 30 Press true",
            "key 30 Release true",
            "Key",
            "key 28 Press false",
            "key 28 Release false",
            "Key",
            "keys done",
            "Answer",
            "draw",
        ];
        assert_eq!(f.log, want);
    }

    #[test]
    fn nothing_to_do_waits_once_and_a_server_that_never_comes_fails() {
        let s = Script::parse("4x4", &[], &[], None).unwrap();
        let mut f = Fake::default();
        let png = scratch("none");
        snapshot(&mut f, &s, &png).unwrap();
        std::fs::remove_file(&png).unwrap();
        assert_eq!(f.log, ["Start", "keys done", "draw"]);
        let mut f = Fake { fail_at: Some(0), ..Default::default() };
        assert_eq!(snapshot(&mut f, &s, &png), Err("nothing came".into()));
        assert!(!png.exists());
    }

    #[test]
    fn keys_as_typed() {
        let k = keys_of("q1+?\\ {left}").unwrap();
        let codes: Vec<_> = k.iter().map(|k| (k.code, k.ch, k.mods.shift)).collect();
        assert_eq!(
            codes,
            [
                (16, Some('q'), false),
                (2, Some('1'), false),
                (13, Some('+'), true),
                (53, Some('?'), true),
                (43, Some('\\'), false),
                (code::SPACE, Some(' '), false),
                (code::LEFT, None, false),
            ]
        );
        let named: Vec<_> = keys_of("{tab}{backspace}{pgup}{pgdn}").unwrap().iter().map(|k| k.code).collect();
        assert_eq!(named, [code::TAB, code::BACKSPACE, code::PAGEUP, code::PAGEDOWN]);
        assert_eq!(keys_of("{f13}"), Err("--keys: no key {f13}".into()));
        assert_eq!(keys_of("é"), Err("--keys: no key for 'é'".into()));
        // Everything a US keyboard types, Shift held where it's needed.
        let typed: Vec<_> = keys_of(":]~").unwrap().iter().map(|k| (k.code, k.ch, k.mods.shift)).collect();
        assert_eq!(typed, [(39, Some(':'), true), (27, Some(']'), false), (41, Some('~'), true)]);
        let named: Vec<_> = keys_of("{home}{end}{delete}").unwrap().iter().map(|k| k.code).collect();
        assert_eq!(named, [code::HOME, code::END, code::DELETE]);
    }

    #[test]
    fn bad_options_say_their_shape() {
        assert_eq!(Script::parse("1920", &[], &[], None), Err("--size is WxH, e.g. 1920x1080".into()));
        assert_eq!(
            Script::parse("4x4", &["1".into()], &[], None),
            Err("--click is X,Y, e.g. 400,300".into())
        );
        assert_eq!(Script::parse("4x4", &[], &["1,2,3".into()], None), Err("--drag is X,Y,X2,Y2".into()));
    }
}
