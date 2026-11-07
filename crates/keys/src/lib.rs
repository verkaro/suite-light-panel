//! The keyboards, read straight from the kernel's input devices (`/dev/input/event*`, evdev),
//! with no X, no Wayland and no terminal: each press, repeat and release, its key code, the
//! modifiers held, and the character it types on a US layout.
//!
//! Only `libc`. Reading the devices needs the `input` group (or a seat's logind handing them
//! over). Grabbed ([`Keyboards::open`] with `grab`), the keys reach this program alone: not the
//! console under it, nor the kernel's VT switching, so the program must offer its own way out.

use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

/// Key codes (`linux/input-event-codes.h`), the ones a panel tends to want by name.
pub mod code {
    pub const ESC: u16 = 1;
    pub const BACKSPACE: u16 = 14;
    pub const TAB: u16 = 15;
    pub const ENTER: u16 = 28;
    pub const LEFTCTRL: u16 = 29;
    pub const LEFTSHIFT: u16 = 42;
    pub const RIGHTSHIFT: u16 = 54;
    pub const LEFTALT: u16 = 56;
    pub const SPACE: u16 = 57;
    pub const F1: u16 = 59;
    pub const F2: u16 = 60;
    pub const F10: u16 = 68;
    pub const F11: u16 = 87;
    pub const F12: u16 = 88;
    pub const KPENTER: u16 = 96;
    pub const RIGHTCTRL: u16 = 97;
    pub const RIGHTALT: u16 = 100;
    pub const HOME: u16 = 102;
    pub const UP: u16 = 103;
    pub const PAGEUP: u16 = 104;
    pub const LEFT: u16 = 105;
    pub const RIGHT: u16 = 106;
    pub const END: u16 = 107;
    pub const DOWN: u16 = 108;
    pub const PAGEDOWN: u16 = 109;
    pub const INSERT: u16 = 110;
    pub const DELETE: u16 = 111;
    pub const LEFTMETA: u16 = 125;
    pub const RIGHTMETA: u16 = 126;
}

const EV_KEY: u16 = 0x01;
const KEY_A: usize = 30;
const KEY_Z: usize = 44;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Press,
    Repeat,
    Release,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key {
    pub code: u16,
    pub state: State,
    /// The modifiers held as it happened (a modifier's own press included).
    pub mods: Mods,
    /// What it types on a US layout, Shift applied; `None` for keys that type nothing, and
    /// with Ctrl, Alt or the meta key held.
    pub ch: Option<char>,
}

impl Key {
    pub fn down(&self) -> bool {
        self.state != State::Release
    }
}

/// The US layout: a code's character unshifted and shifted.
fn us(code: u16) -> Option<(char, char)> {
    const ROW1: &[u8] = b"1234567890-=";
    const ROW1S: &[u8] = b"!@#$%^&*()_+";
    const ROW2: &[u8] = b"qwertyuiop[]";
    const ROW3: &[u8] = b"asdfghjkl;'`";
    const ROW4: &[u8] = b"\\zxcvbnm,./";
    const ROW4S: &[u8] = b"|ZXCVBNM<>?";
    let c = code as usize;
    let pick = |row: &[u8], shifted: &[u8], i: usize| Some((row[i] as char, shifted[i] as char));
    match c {
        2..=13 => pick(ROW1, ROW1S, c - 2),
        16..=27 => pick(ROW2, b"QWERTYUIOP{}", c - 16),
        30..=41 => pick(ROW3, b"ASDFGHJKL:\"~", c - 30),
        43..=53 => pick(ROW4, ROW4S, c - 43),
        57 => Some((' ', ' ')),
        _ => None,
    }
}

/// One keyboard's device.
struct Device {
    file: File,
    path: PathBuf,
    grabbed: bool,
}

/// Every keyboard plugged in when opened.
pub struct Keyboards {
    devs: Vec<Device>,
    held: [bool; 768],
}

fn ior(nr: u64, size: usize) -> u64 {
    (2 << 30) | ((size as u64) << 16) | ((b'E' as u64) << 8) | nr
}
fn iow(nr: u64, size: usize) -> u64 {
    (1 << 30) | ((size as u64) << 16) | ((b'E' as u64) << 8) | nr
}

/// Whether the device has letter keys: a keyboard, not a power button or a mouse's side keys.
fn is_keyboard(fd: RawFd) -> bool {
    let mut bits = [0u8; 96];
    // EVIOCGBIT(EV_KEY, len)
    // SAFETY: the kernel writes at most `len` bytes.
    let r = unsafe { libc::ioctl(fd, ior(0x20 + EV_KEY as u64, bits.len()) as _, bits.as_mut_ptr()) };
    let has = |k: usize| bits[k / 8] & (1 << (k % 8)) != 0;
    r >= 0 && (KEY_A..=KEY_Z).all(has)
}

fn grab(fd: RawFd, on: bool) -> bool {
    // EVIOCGRAB
    // SAFETY: an int argument by value.
    unsafe { libc::ioctl(fd, iow(0x90, 4) as _, on as libc::c_int) == 0 }
}

impl Keyboards {
    /// Opens every keyboard; `grab` takes them for this program alone. Fails only when none can
    /// be read, saying why.
    pub fn open(grab_them: bool) -> io::Result<Keyboards> {
        let mut paths: Vec<PathBuf> = std::fs::read_dir("/dev/input")?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("event")))
            .collect();
        paths.sort();
        let mut devs = Vec::new();
        let mut denied = 0;
        for path in paths {
            let file = match OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK).open(&path) {
                Ok(f) => f,
                Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
                    denied += 1;
                    continue;
                }
                Err(_) => continue,
            };
            if !is_keyboard(file.as_raw_fd()) {
                continue;
            }
            let grabbed = grab_them && grab(file.as_raw_fd(), true);
            devs.push(Device { file, path, grabbed });
        }
        if devs.is_empty() {
            return Err(io::Error::other(if denied > 0 {
                "no keyboard can be read: /dev/input needs the input group (log in again after joining it)"
            } else {
                "no keyboard found under /dev/input"
            }));
        }
        Ok(Keyboards { devs, held: [false; 768] })
    }

    /// The devices, for poll.
    pub fn fds(&self) -> Vec<RawFd> {
        self.devs.iter().map(|d| d.file.as_raw_fd()).collect()
    }

    /// The devices read, for a person to check.
    pub fn paths(&self) -> Vec<(PathBuf, bool)> {
        self.devs.iter().map(|d| (d.path.clone(), d.grabbed)).collect()
    }

    /// Lets the keys go to the console again for a while (the VT switched away), or takes them
    /// back.
    pub fn set_grab(&mut self, on: bool) {
        for d in &mut self.devs {
            d.grabbed = grab(d.file.as_raw_fd(), on) && on;
        }
    }

    pub fn mods(&self) -> Mods {
        let h = |c: u16| self.held[c as usize];
        Mods {
            shift: h(code::LEFTSHIFT) || h(code::RIGHTSHIFT),
            ctrl: h(code::LEFTCTRL) || h(code::RIGHTCTRL),
            alt: h(code::LEFTALT) || h(code::RIGHTALT),
            meta: h(code::LEFTMETA) || h(code::RIGHTMETA),
        }
    }

    /// Every key event waiting, without blocking. A device unplugged is dropped.
    pub fn read(&mut self) -> Vec<Key> {
        let mut out = Vec::new();
        let mut gone = Vec::new();
        for (i, d) in self.devs.iter_mut().enumerate() {
            // struct input_event on 64-bit: a timeval (16 bytes), type, code (u16), value (i32).
            let mut buf = [0u8; 24 * 64];
            loop {
                match d.file.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        for ev in buf[..n].as_chunks::<24>().0 {
                            let kind = u16::from_ne_bytes([ev[16], ev[17]]);
                            let c = u16::from_ne_bytes([ev[18], ev[19]]);
                            let v = i32::from_ne_bytes([ev[20], ev[21], ev[22], ev[23]]);
                            if kind == EV_KEY && (c as usize) < 768 {
                                out.push((c, v));
                            }
                        }
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(_) => {
                        gone.push(i);
                        break;
                    }
                }
            }
        }
        for i in gone.into_iter().rev() {
            self.devs.remove(i);
        }
        out.into_iter()
            .map(|(c, v)| {
                let state = match v {
                    0 => State::Release,
                    2 => State::Repeat,
                    _ => State::Press,
                };
                self.held[c as usize] = state != State::Release;
                let mods = self.mods();
                let ch = if mods.ctrl || mods.alt || mods.meta {
                    None
                } else {
                    us(c).map(|(lo, hi)| if mods.shift { hi } else { lo })
                };
                Key { code: c, state, mods, ch }
            })
            .collect()
    }
}

impl Drop for Keyboards {
    fn drop(&mut self) {
        for d in &self.devs {
            if d.grabbed {
                grab(d.file.as_raw_fd(), false);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn us_layout() {
        assert_eq!(us(30), Some(('a', 'A')));
        assert_eq!(us(2), Some(('1', '!')));
        assert_eq!(us(12), Some(('-', '_')));
        assert_eq!(us(13), Some(('=', '+')));
        assert_eq!(us(39), Some((';', ':')));
        assert_eq!(us(53), Some(('/', '?')));
        assert_eq!(us(57), Some((' ', ' ')));
        assert_eq!(us(code::HOME), None);
    }

    #[test]
    fn ioctl_numbers() {
        // EVIOCGRAB and EVIOCGBIT(EV_KEY, 96), as the headers give them.
        assert_eq!(iow(0x90, 4), 0x40044590);
        assert_eq!(ior(0x21, 96), 0x80604521);
    }
}
