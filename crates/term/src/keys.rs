//! Keys to the bytes a shell reads, as terminfo's `linux` says the console sends them
//! (`infocmp linux`: `kbs`, `kcuu1`, `kf1` and the rest): Backspace is DEL, F1 to F5 are
//! `ESC [[A` to `ESC [[E`, Home and End `ESC [1~` and `ESC [4~`. An xterm's own differ, and come
//! with `TERM=xterm-256color`.
//!
//! Ctrl and a letter is its control byte; Alt sends ESC first. US layout only, as `lp-keys` is.
//! The keypad types its digits and signs, as with Num Lock on: `linux` has no `smkx`, so no
//! program asks for its application keys.

use lp_keys::{Key, State, code, us_char};

use crate::Modes;

/// What `k` sends to the shell, or nothing: a release, a modifier alone, a key the meta key is
/// held with, or one with no sequence. The console's own keys (Ctrl+Alt+F*n*, and the frame's)
/// are the caller's to take first.
pub fn key_bytes(k: &Key, modes: &Modes) -> Option<Vec<u8>> {
    if k.state == State::Release || k.mods.meta {
        return None;
    }
    let mut out = Vec::new();
    if k.mods.alt {
        out.push(0x1B);
    }
    let csi = |s: &str| [b"\x1b[", s.as_bytes()].concat();
    let arrow = |c: u8| if modes.app_cursor { vec![0x1B, b'O', c] } else { vec![0x1B, b'[', c] };
    let shift = k.mods.shift;
    let body: Vec<u8> = match k.code {
        code::ENTER | code::KPENTER => {
            if modes.newline {
                b"\r\n".to_vec()
            } else {
                b"\r".to_vec()
            }
        }
        code::BACKSPACE => vec![0x7F],
        code::TAB if shift => b"\x1b\t".to_vec(),
        code::TAB => b"\t".to_vec(),
        code::ESC => vec![0x1B],
        code::UP => arrow(b'A'),
        code::DOWN => arrow(b'B'),
        code::RIGHT => arrow(b'C'),
        code::LEFT => arrow(b'D'),
        code::HOME => csi("1~"),
        code::INSERT => csi("2~"),
        code::DELETE => csi("3~"),
        code::END => csi("4~"),
        code::PAGEUP => csi("5~"),
        code::PAGEDOWN => csi("6~"),
        // F1 to F8 with Shift are terminfo's F13 to F20.
        c @ code::F1..=code::F5 if !shift => csi(&format!("[{}", (b'A' + (c - code::F1) as u8) as char)),
        c @ code::F1..=code::F8 if shift => {
            const F13: [u8; 8] = [25, 26, 28, 29, 31, 32, 33, 34];
            csi(&format!("{}~", F13[usize::from(c - code::F1)]))
        }
        c @ code::F6..=code::F10 => csi(&format!("{}~", 17 + c - code::F6)),
        code::F11 => csi("23~"),
        code::F12 => csi("24~"),
        c => {
            let ch = keypad(c).or_else(|| us_char(c, shift))?;
            if k.mods.ctrl { vec![control(ch)?] } else { ch.to_string().into_bytes() }
        }
    };
    out.extend(body);
    Some(out)
}

/// The keypad's keys, as with Num Lock on.
fn keypad(c: u16) -> Option<char> {
    Some(match c {
        code::KP0 => '0',
        code::KP1 => '1',
        code::KP2 => '2',
        code::KP3 => '3',
        code::KP4 => '4',
        code::KP5 => '5',
        code::KP6 => '6',
        code::KP7 => '7',
        code::KP8 => '8',
        code::KP9 => '9',
        code::KPDOT => '.',
        code::KPPLUS => '+',
        code::KPMINUS => '-',
        code::KPASTERISK => '*',
        code::KPSLASH => '/',
        _ => return None,
    })
}

/// Ctrl with a character: the control byte the console sends for it.
fn control(c: char) -> Option<u8> {
    Some(match c {
        'a'..='z' => c as u8 - b'a' + 1,
        'A'..='Z' => c as u8 - b'A' + 1,
        ' ' | '@' | '2' => 0x00,
        '[' | '3' => 0x1B,
        '\\' | '4' => 0x1C,
        ']' | '5' => 0x1D,
        '^' | '6' => 0x1E,
        '_' | '-' | '/' | '7' => 0x1F,
        '?' | '8' => 0x7F,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_keys::Mods;

    fn press(code: u16, shift: bool, ctrl: bool, alt: bool) -> Key {
        let mods = Mods { shift, ctrl, alt, meta: false };
        let ch = if ctrl || alt { None } else { us_char(code, shift) };
        Key { code, state: State::Press, mods, ch }
    }

    fn bytes(code: u16, shift: bool, ctrl: bool, alt: bool) -> Option<Vec<u8>> {
        key_bytes(&press(code, shift, ctrl, alt), &Modes::default())
    }

    const A: u16 = 30;
    const C: u16 = 46;
    const TWO: u16 = 3;
    const LEFTBRACE: u16 = 26;

    #[test]
    fn typing() {
        assert_eq!(bytes(A, false, false, false).unwrap(), b"a");
        assert_eq!(bytes(A, true, false, false).unwrap(), b"A");
        assert_eq!(bytes(TWO, true, false, false).unwrap(), b"@");
        assert_eq!(bytes(code::SPACE, false, false, false).unwrap(), b" ");
        assert_eq!(bytes(code::KP7, false, false, false).unwrap(), b"7");
        assert_eq!(bytes(code::KPSLASH, false, false, false).unwrap(), b"/");
    }

    #[test]
    fn ctrl_and_alt() {
        assert_eq!(bytes(C, false, true, false).unwrap(), [0x03], "Ctrl+C");
        assert_eq!(bytes(A, true, true, false).unwrap(), [0x01], "Ctrl+Shift+A");
        assert_eq!(bytes(code::SPACE, false, true, false).unwrap(), [0x00]);
        assert_eq!(bytes(LEFTBRACE, false, true, false).unwrap(), [0x1B]);
        assert_eq!(bytes(TWO, false, true, false).unwrap(), [0x00]);
        assert_eq!(bytes(A, false, false, true).unwrap(), b"\x1ba", "Alt sends ESC first");
        assert_eq!(bytes(C, false, true, true).unwrap(), b"\x1b\x03");
        assert_eq!(bytes(code::BACKSPACE, false, false, true).unwrap(), b"\x1b\x7f");
    }

    #[test]
    fn linux_terminfo_keys() {
        for (code, want, cap) in [
            (code::BACKSPACE, &b"\x7f"[..], "kbs"),
            (code::UP, b"\x1b[A", "kcuu1"),
            (code::DOWN, b"\x1b[B", "kcud1"),
            (code::RIGHT, b"\x1b[C", "kcuf1"),
            (code::LEFT, b"\x1b[D", "kcub1"),
            (code::HOME, b"\x1b[1~", "khome"),
            (code::INSERT, b"\x1b[2~", "kich1"),
            (code::DELETE, b"\x1b[3~", "kdch1"),
            (code::END, b"\x1b[4~", "kend"),
            (code::PAGEUP, b"\x1b[5~", "kpp"),
            (code::PAGEDOWN, b"\x1b[6~", "knp"),
            (code::F1, b"\x1b[[A", "kf1"),
            (code::F5, b"\x1b[[E", "kf5"),
            (code::F6, b"\x1b[17~", "kf6"),
            (code::F9, b"\x1b[20~", "kf9"),
            (code::F10, b"\x1b[21~", "kf10"),
            (code::F11, b"\x1b[23~", "kf11"),
            (code::F12, b"\x1b[24~", "kf12"),
            (code::ENTER, b"\r", "cr"),
            (code::TAB, b"\t", "ht"),
            (code::ESC, b"\x1b", "esc"),
        ] {
            assert_eq!(bytes(code, false, false, false).unwrap(), want, "{cap}");
        }
        assert_eq!(bytes(code::TAB, true, false, false).unwrap(), b"\x1b\t", "kcbt");
        assert_eq!(bytes(code::F1, true, false, false).unwrap(), b"\x1b[25~", "kf13");
        assert_eq!(bytes(code::F3, true, false, false).unwrap(), b"\x1b[28~", "kf15");
        assert_eq!(bytes(code::F8, true, false, false).unwrap(), b"\x1b[34~", "kf20");
    }

    #[test]
    fn modes() {
        let app = Modes { app_cursor: true, ..Modes::default() };
        assert_eq!(key_bytes(&press(code::UP, false, false, false), &app).unwrap(), b"\x1bOA");
        let lnm = Modes { newline: true, ..Modes::default() };
        assert_eq!(key_bytes(&press(code::ENTER, false, false, false), &lnm).unwrap(), b"\r\n");
    }

    #[test]
    fn nothing_sent() {
        assert_eq!(bytes(code::LEFTSHIFT, false, false, false), None, "a modifier alone");
        assert_eq!(bytes(C, false, true, false).map(|_| ()), Some(()));
        let mut k = press(A, false, false, false);
        k.state = State::Release;
        assert_eq!(key_bytes(&k, &Modes::default()), None, "a release");
        k.state = State::Repeat;
        assert_eq!(key_bytes(&k, &Modes::default()).unwrap(), b"a", "a repeat types again");
        k.mods.meta = true;
        assert_eq!(key_bytes(&k, &Modes::default()), None, "the meta key held");
    }
}
