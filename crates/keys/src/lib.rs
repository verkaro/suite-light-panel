//! The keyboards, read straight from the kernel's input devices (`/dev/input/event*`, evdev),
//! with no X, no Wayland and no terminal: each press, repeat and release, its key code, the
//! modifiers held, and the character it types on a US layout.
//!
//! Only `libc`. Reading the devices needs the `input` group (or a seat's logind handing them
//! over). Grabbed ([`Keyboards::open`] with `grab`), the keys reach this program alone: not the
//! console under it, nor the kernel's VT switching, so the program must offer its own way out.
//!
//! A keyboard is grabbed only once no key is down on it. The console's keyboard keeps its own
//! count of the modifiers held, from the presses and releases it sees: grabbed between a press
//! and its release (the Enter that started the program, the Ctrl+Alt of a switch back to its VT),
//! the console never sees the release, and after the program it types as if Ctrl and Alt were
//! still held, which is to say nothing at all. So the keys pressed then reach the console too,
//! until they're let go.
//!
//! [`Pointers`]: the mice, touchpads and touchscreens, read the same way. [`Hotplug`]: the
//! devices plugged in later, for [`Keyboards::add`] and [`Pointers::add`].

pub mod common;
mod hotplug;
mod pointer;
pub use hotplug::{Hotplug, INPUT_DIR};
pub use pointer::{Button, Pointer, PointerKind, Pointers};

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

/// The key every panel shares to leave for muntin, the family's frame: Ctrl+Alt+Esc (Warren,
/// 2026-11-07). A panel acts on it only [`under_frame`], quitting as on its own quit key, with
/// exit status 0; standing alone the key does nothing.
pub const FRAME_KEY: &str = "Ctrl+Alt+Esc";

/// Whether `k` is [`FRAME_KEY`], pressed.
pub fn is_frame_key(k: &Key) -> bool {
    k.state == State::Press && k.mods.ctrl && k.mods.alt && k.code == code::ESC
}

/// Whether the frame started this program: it sets `MUNTIN_TOOL` to the tool's id.
pub fn under_frame() -> bool {
    std::env::var_os("MUNTIN_TOOL").is_some_and(|v| !v.is_empty())
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

/// Every keyboard plugged in when opened, and those [`Keyboards::add`]ed since.
pub struct Keyboards {
    devs: Vec<Device>,
    held: [bool; 768],
    /// Grab them, each as soon as no key is down on it.
    want: bool,
}

pub(crate) fn ior(nr: u64, size: usize) -> u64 {
    (2 << 30) | ((size as u64) << 16) | ((b'E' as u64) << 8) | nr
}
pub(crate) fn iow(nr: u64, size: usize) -> u64 {
    (1 << 30) | ((size as u64) << 16) | ((b'E' as u64) << 8) | nr
}

/// A bitmap the kernel fills by `EVIOCG*(len)` number `nr`; all zero when it fails.
pub(crate) fn bits(fd: RawFd, nr: u64) -> [u8; 96] {
    let mut b = [0u8; 96];
    // SAFETY: the kernel writes at most `len` bytes.
    if unsafe { libc::ioctl(fd, ior(nr, b.len()) as _, b.as_mut_ptr()) } < 0 {
        b = [0; 96];
    }
    b
}
pub(crate) fn has(b: &[u8; 96], k: usize) -> bool {
    b[k / 8] & (1 << (k % 8)) != 0
}

/// Whether the device has letter keys: a keyboard, not a power button or a mouse's side keys.
pub(crate) fn is_keyboard(fd: RawFd) -> bool {
    // EVIOCGBIT(EV_KEY, len)
    let b = bits(fd, 0x20 + EV_KEY as u64);
    (KEY_A..=KEY_Z).all(|k| has(&b, k))
}

/// Whether any key on the device is down now (EVIOCGKEY).
fn any_down(fd: RawFd) -> bool {
    bits(fd, 0x18).iter().any(|&b| b != 0)
}

pub(crate) fn grab(fd: RawFd, on: bool) -> bool {
    // EVIOCGRAB
    // SAFETY: an int argument by value.
    unsafe { libc::ioctl(fd, iow(0x90, 4) as _, on as libc::c_int) == 0 }
}

/// The device's name, as it gives it ("AT Translated Set 2 keyboard").
pub(crate) fn name(fd: RawFd) -> String {
    let mut b = [0u8; 128];
    // EVIOCGNAME(len)
    // SAFETY: the kernel writes at most `len` bytes.
    let n = unsafe { libc::ioctl(fd, ior(0x06, b.len()) as _, b.as_mut_ptr()) };
    let s = &b[..(n.max(0) as usize).min(b.len())];
    String::from_utf8_lossy(&s[..s.iter().position(|&c| c == 0).unwrap_or(s.len())]).into_owned()
}

/// One device, opened to read without blocking.
pub(crate) fn open_one(path: &std::path::Path) -> io::Result<File> {
    OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC).open(path)
}

/// The devices under /dev/input, in order, opened to read without blocking; and how many
/// couldn't be for want of permission.
pub(crate) fn open_all() -> io::Result<(Vec<(PathBuf, File)>, usize)> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir("/dev/input")?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("event")))
        .collect();
    paths.sort();
    let mut out = Vec::new();
    let mut denied = 0;
    for path in paths {
        match open_one(&path) {
            Ok(f) => out.push((path, f)),
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => denied += 1,
            Err(_) => {}
        }
    }
    Ok((out, denied))
}

/// Every event waiting on `file` as (type, code, value), without blocking; `Err` when the
/// device has gone.
pub(crate) fn read_events(file: &mut File, out: &mut Vec<(u16, u16, i32)>) -> io::Result<()> {
    // struct input_event on 64-bit: a timeval (16 bytes), type, code (u16), value (i32).
    let mut buf = [0u8; 24 * 64];
    loop {
        match file.read(&mut buf) {
            Ok(0) => return Ok(()),
            Ok(n) => {
                for ev in buf[..n].as_chunks::<24>().0 {
                    let kind = u16::from_ne_bytes([ev[16], ev[17]]);
                    let c = u16::from_ne_bytes([ev[18], ev[19]]);
                    let v = i32::from_ne_bytes([ev[20], ev[21], ev[22], ev[23]]);
                    out.push((kind, c, v));
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(()),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
}

impl Keyboards {
    /// Opens every keyboard; `grab` takes them for this program alone, each once no key is down
    /// on it. Fails when none can be read for want of permission, saying why; none plugged in is
    /// no error (one may be, later: [`Keyboards::add`]).
    pub fn open(grab_them: bool) -> io::Result<Keyboards> {
        let (all, denied) = open_all()?;
        let devs: Vec<Device> = all
            .into_iter()
            .filter(|(_, f)| is_keyboard(f.as_raw_fd()))
            .map(|(path, file)| Device { file, path, grabbed: false })
            .collect();
        if devs.is_empty() && denied > 0 {
            return Err(io::Error::other(
                "no keyboard can be read: /dev/input needs the input group (log in again after joining it)",
            ));
        }
        let mut k = Keyboards { devs, held: [false; 768], want: grab_them };
        k.settle();
        Ok(k)
    }

    /// Grabs each keyboard wanted and not yet grabbed whose keys are all up.
    fn settle(&mut self) {
        if !self.want {
            return;
        }
        for d in self.devs.iter_mut().filter(|d| !d.grabbed) {
            let fd = d.file.as_raw_fd();
            if !any_down(fd) {
                d.grabbed = grab(fd, true);
            }
        }
    }

    /// Takes the device at `path` if it's a keyboard not already held, grabbed as the others
    /// are: its name when it was. For [`Hotplug::read`]'s paths: one that can't be opened yet
    /// (udev not done with it) is offered again when its permissions change.
    pub fn add(&mut self, path: &std::path::Path) -> Option<String> {
        if self.devs.iter().any(|d| d.path == path) {
            return None;
        }
        let file = open_one(path).ok()?;
        if !is_keyboard(file.as_raw_fd()) {
            return None;
        }
        let n = name(file.as_raw_fd());
        self.devs.push(Device { file, path: path.to_path_buf(), grabbed: false });
        self.settle();
        Some(n)
    }

    pub fn is_empty(&self) -> bool {
        self.devs.is_empty()
    }

    /// Whether every keyboard is grabbed (false while one waits for its keys to be let go).
    pub fn all_grabbed(&self) -> bool {
        self.devs.iter().all(|d| d.grabbed)
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
    /// back, each keyboard once its keys are up.
    ///
    /// Taking them back, which keys are held is read afresh from each keyboard (`EVIOCGKEY`):
    /// while another program had them, a press could come without its release, and the
    /// modifiers would be wrong on every key after.
    pub fn set_grab(&mut self, on: bool) {
        self.want = on;
        if on {
            self.held = [false; 768];
            for d in &self.devs {
                let b = bits(d.file.as_raw_fd(), 0x18);
                for (c, h) in self.held.iter_mut().enumerate() {
                    *h |= has(&b, c);
                }
            }
            self.settle();
            return;
        }
        for d in self.devs.iter_mut().filter(|d| d.grabbed) {
            grab(d.file.as_raw_fd(), false);
            d.grabbed = false;
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
        let mut evs = Vec::new();
        let before = self.devs.len();
        self.devs.retain_mut(|d| read_events(&mut d.file, &mut evs).is_ok());
        if self.devs.len() < before {
            // A keyboard unplugged with keys down: none of them is held now.
            self.held = [false; 768];
        }
        self.settle();
        evs.into_iter()
            .filter(|&(kind, c, _)| kind == EV_KEY && (c as usize) < 768)
            .map(|(_, c, v)| (c, v))
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
    fn the_frame_key() {
        let k = |code, state, ctrl, alt| Key {
            code,
            state,
            mods: Mods { ctrl, alt, ..Mods::default() },
            ch: None,
        };
        assert!(is_frame_key(&k(code::ESC, State::Press, true, true)));
        assert!(!is_frame_key(&k(code::ESC, State::Repeat, true, true)));
        assert!(!is_frame_key(&k(code::ESC, State::Release, true, true)));
        assert!(!is_frame_key(&k(code::ESC, State::Press, true, false)));
        assert!(!is_frame_key(&k(code::ESC, State::Press, false, true)));
        assert!(!is_frame_key(&k(code::ENTER, State::Press, true, true)));
    }

    #[test]
    fn ioctl_numbers() {
        // EVIOCGRAB and EVIOCGBIT(EV_KEY, 96), as the headers give them.
        assert_eq!(iow(0x90, 4), 0x40044590);
        assert_eq!(ior(0x21, 96), 0x80604521);
    }
}
