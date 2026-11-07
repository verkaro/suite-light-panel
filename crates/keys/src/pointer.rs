//! The pointers: mice, touchpads, touchscreens, and the absolute pointers of a tablet or a
//! virtual machine, read from evdev like the keys, as one position on the screen.
//!
//! A mouse moves it by its counts, a touchpad by the finger's travel (a tap clicks), a
//! touchscreen puts it where it's touched. The program draws the pointer itself (nothing else
//! will, without a display server): [`Pointers::cursor`] says where, and whether to, which a
//! touchscreen's touch turns off and a mouse turns on again.

use crate::{bits, grab, has, is_keyboard, name, open_all, open_one, read_events};
use std::fs::File;
use std::io;
use std::os::fd::{AsRawFd, RawFd};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const EV_REL: u16 = 0x02;
const EV_ABS: u16 = 0x03;
const SYN_REPORT: u16 = 0;
const REL_X: u16 = 0;
const REL_Y: u16 = 1;
const REL_WHEEL: u16 = 8;
const ABS_X: u16 = 0;
const ABS_Y: u16 = 1;
const BTN_LEFT: u16 = 0x110;
const BTN_RIGHT: u16 = 0x111;
const BTN_MIDDLE: u16 = 0x112;
const BTN_TOOL_FINGER: u16 = 0x145;
const BTN_TOUCH: u16 = 0x14a;
const BTN_TOOL_DOUBLETAP: u16 = 0x14d;
const INPUT_PROP_POINTER: usize = 0;
const INPUT_PROP_DIRECT: usize = 1;

/// A touch shorter than this, that hardly moved, is a tap: a click.
const TAP: Duration = Duration::from_millis(180);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    Middle,
}

/// What the pointer did, at (x, y) in screen pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pointer {
    Move {
        x: i32,
        y: i32,
    },
    Press {
        x: i32,
        y: i32,
        button: Button,
    },
    Release {
        x: i32,
        y: i32,
        button: Button,
    },
    /// A wheel's notches: up positive.
    Wheel {
        x: i32,
        y: i32,
        notches: i32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerKind {
    Mouse,
    Touchpad,
    Touchscreen,
    /// A tablet, or a virtual machine's pointer: absolute, with a cursor.
    Absolute,
}

#[derive(Clone, Copy, Default)]
struct Axis {
    min: i32,
    max: i32,
}

impl Axis {
    fn of(fd: RawFd, abs: u16) -> Axis {
        // struct input_absinfo: value, minimum, maximum, fuzz, flat, resolution.
        let mut a = [0i32; 6];
        // EVIOCGABS(abs)
        // SAFETY: the kernel writes the 24 bytes of an input_absinfo.
        unsafe { libc::ioctl(fd, crate::ior(0x40 + abs as u64, 24) as _, a.as_mut_ptr()) };
        Axis { min: a[1], max: a[2] }
    }
    fn range(&self) -> f32 {
        (self.max - self.min).max(1) as f32
    }
    /// `v` along `len` pixels.
    fn map(&self, v: i32, len: usize) -> f32 {
        (v - self.min) as f32 / self.range() * (len.saturating_sub(1)) as f32
    }
}

struct Device {
    file: File,
    path: PathBuf,
    kind: PointerKind,
    grabbed: bool,
    ax: Axis,
    ay: Axis,
    // Since the last report:
    dx: i32,
    dy: i32,
    wheel: i32,
    abs: (Option<i32>, Option<i32>),
    buttons: Vec<(Button, bool)>,
    touch: Option<bool>,
    // A touch under way: where it was last, when it began, how far it went (pixels), and
    // whether two fingers were down.
    last: Option<(i32, i32)>,
    began: Option<Instant>,
    travel: f32,
    two: bool,
}

/// Every pointer plugged in when opened, and those [`Pointers::add`]ed since, as one position
/// on a screen of `w` × `h`.
pub struct Pointers {
    devs: Vec<Device>,
    w: usize,
    h: usize,
    x: f32,
    y: f32,
    show: bool,
    want: bool,
}

fn classify(fd: RawFd) -> Option<PointerKind> {
    if is_keyboard(fd) {
        return None;
    }
    let ev = bits(fd, 0x20);
    let rel = bits(fd, 0x20 + EV_REL as u64);
    let abs = bits(fd, 0x20 + EV_ABS as u64);
    let key = bits(fd, 0x20 + EV_KEY as u64);
    // EVIOCGPROP(len)
    let prop = bits(fd, 0x09);
    let k = |c: u16| has(&key, c as usize);
    if has(&ev, EV_REL as usize) && has(&rel, REL_X as usize) && has(&rel, REL_Y as usize) && k(BTN_LEFT) {
        return Some(PointerKind::Mouse);
    }
    if !(has(&ev, EV_ABS as usize) && has(&abs, ABS_X as usize) && has(&abs, ABS_Y as usize)) {
        return None;
    }
    if has(&prop, INPUT_PROP_DIRECT) || (k(BTN_TOUCH) && !k(BTN_TOOL_FINGER) && !k(BTN_LEFT)) {
        Some(PointerKind::Touchscreen)
    } else if k(BTN_TOOL_FINGER) || has(&prop, INPUT_PROP_POINTER) {
        Some(PointerKind::Touchpad)
    } else if k(BTN_LEFT) || k(BTN_TOUCH) {
        Some(PointerKind::Absolute)
    } else {
        None // a joystick, an accelerometer
    }
}

impl Device {
    fn new(path: PathBuf, file: File, grab_it: bool) -> Option<Device> {
        let fd = file.as_raw_fd();
        let kind = classify(fd)?;
        let (ax, ay) = if kind == PointerKind::Mouse {
            (Axis::default(), Axis::default())
        } else {
            (Axis::of(fd, ABS_X), Axis::of(fd, ABS_Y))
        };
        let grabbed = grab_it && grab(fd, true);
        Some(Device {
            file,
            path,
            kind,
            grabbed,
            ax,
            ay,
            dx: 0,
            dy: 0,
            wheel: 0,
            abs: (None, None),
            buttons: Vec::new(),
            touch: None,
            last: None,
            began: None,
            travel: 0.0,
            two: false,
        })
    }
}

fn button(c: u16) -> Option<Button> {
    match c {
        BTN_LEFT => Some(Button::Left),
        BTN_RIGHT => Some(Button::Right),
        BTN_MIDDLE => Some(Button::Middle),
        _ => None,
    }
}

impl Pointers {
    /// Opens every pointer, for a screen of `w` × `h`; `grab` takes them for this program alone.
    /// None at all is no error: a panel may have only keys.
    pub fn open(grab_them: bool, w: usize, h: usize) -> io::Result<Pointers> {
        let (all, _) = open_all()?;
        let devs = all.into_iter().filter_map(|(path, file)| Device::new(path, file, grab_them)).collect();
        let (x, y) = (w as f32 / 2.0, h as f32 / 2.0);
        Ok(Pointers { devs, w, h, x, y, show: false, want: grab_them })
    }

    /// Takes the device at `path` if it's a pointer not already held, grabbed as the others
    /// are: what it was taken for, and its name. As [`crate::Keyboards::add`].
    pub fn add(&mut self, path: &std::path::Path) -> Option<(PointerKind, String)> {
        if self.devs.iter().any(|d| d.path == path) {
            return None;
        }
        let d = Device::new(path.to_path_buf(), open_one(path).ok()?, self.want)?;
        let got = (d.kind, name(d.file.as_raw_fd()));
        self.devs.push(d);
        Some(got)
    }

    pub fn is_empty(&self) -> bool {
        self.devs.is_empty()
    }

    /// The devices, for poll.
    pub fn fds(&self) -> Vec<RawFd> {
        self.devs.iter().map(|d| d.file.as_raw_fd()).collect()
    }

    /// The devices read, what each was taken for, and whether it's grabbed.
    pub fn devices(&self) -> Vec<(PathBuf, PointerKind, bool)> {
        self.devs.iter().map(|d| (d.path.clone(), d.kind, d.grabbed)).collect()
    }

    /// As [`crate::Keyboards::set_grab`].
    pub fn set_grab(&mut self, on: bool) {
        self.want = on;
        for d in &mut self.devs {
            if d.grabbed != on {
                d.grabbed = grab(d.file.as_raw_fd(), on) && on;
            }
        }
    }

    /// Where the pointer is, when it should be drawn: after a mouse, a touchpad or a tablet
    /// moved it, not after a touchscreen was touched.
    pub fn cursor(&self) -> Option<(i32, i32)> {
        self.show.then_some((self.x as i32, self.y as i32))
    }

    /// Hides the cursor until the pointer moves again (the keys were used, say).
    pub fn hide(&mut self) {
        self.show = false;
    }

    fn at(&self) -> (i32, i32) {
        (self.x as i32, self.y as i32)
    }

    fn clamp(&mut self) {
        self.x = self.x.clamp(0.0, self.w.saturating_sub(1) as f32);
        self.y = self.y.clamp(0.0, self.h.saturating_sub(1) as f32);
    }

    /// Everything the pointers did since the last call, without blocking. A device unplugged
    /// is dropped.
    pub fn read(&mut self) -> Vec<Pointer> {
        let mut out = Vec::new();
        let mut devs = std::mem::take(&mut self.devs);
        devs.retain_mut(|d| {
            let mut evs = Vec::new();
            let ok = read_events(&mut d.file, &mut evs).is_ok();
            for (kind, c, v) in evs {
                match (kind, c) {
                    (EV_REL, REL_X) => d.dx += v,
                    (EV_REL, REL_Y) => d.dy += v,
                    (EV_REL, REL_WHEEL) => d.wheel += v,
                    (EV_ABS, ABS_X) => d.abs.0 = Some(v),
                    (EV_ABS, ABS_Y) => d.abs.1 = Some(v),
                    (EV_KEY, BTN_TOUCH) => d.touch = Some(v != 0),
                    (EV_KEY, BTN_TOOL_DOUBLETAP) if v != 0 => d.two = true,
                    (EV_KEY, c) => {
                        if let Some(b) = button(c) {
                            d.buttons.push((b, v != 0));
                        }
                    }
                    (EV_SYN, SYN_REPORT) => self.report(d, &mut out),
                    _ => {}
                }
            }
            ok
        });
        self.devs = devs;
        out
    }

    /// One device's report, its changes since the last, as what the pointer did.
    fn report(&mut self, d: &mut Device, out: &mut Vec<Pointer>) {
        let before = self.at();
        match d.kind {
            PointerKind::Mouse => {
                if d.dx != 0 || d.dy != 0 {
                    // A little acceleration, so a small screen and a big one both cross easily.
                    let (dx, dy) = (d.dx as f32, d.dy as f32);
                    let speed = (dx * dx + dy * dy).sqrt();
                    let k =
                        (1.0 + (speed - 4.0).max(0.0) * 0.08).min(3.0) * (self.w as f32 / 1920.0).max(0.6);
                    self.x += dx * k;
                    self.y += dy * k;
                    self.show = true;
                }
            }
            PointerKind::Touchpad => {
                if d.touch == Some(true) {
                    d.began = Some(Instant::now());
                    d.travel = 0.0;
                    d.last = None;
                    d.two = false;
                }
                let nx = d.abs.0.or(d.last.map(|l| l.0));
                let ny = d.abs.1.or(d.last.map(|l| l.1));
                if d.began.is_some()
                    && let (Some(nx), Some(ny)) = (nx, ny)
                {
                    if let Some((lx, ly)) = d.last {
                        // The pad's width across about 1.2 screens, the same scale both ways.
                        let k = self.w as f32 * 1.2 / d.ax.range();
                        let (mx, my) = ((nx - lx) as f32 * k, (ny - ly) as f32 * k);
                        self.x += mx;
                        self.y += my;
                        d.travel += mx.abs() + my.abs();
                    }
                    d.last = Some((nx, ny));
                    self.show = true;
                }
                if d.touch == Some(false) {
                    if let Some(t) = d.began.take()
                        && t.elapsed() < TAP
                        && d.travel < 12.0
                    {
                        let b = if d.two { Button::Right } else { Button::Left };
                        let (x, y) = before;
                        out.push(Pointer::Press { x, y, button: b });
                        out.push(Pointer::Release { x, y, button: b });
                    }
                    d.last = None;
                }
            }
            PointerKind::Touchscreen | PointerKind::Absolute => {
                if let Some(v) = d.abs.0 {
                    self.x = d.ax.map(v, self.w);
                }
                if let Some(v) = d.abs.1 {
                    self.y = d.ay.map(v, self.h);
                }
                self.show = d.kind == PointerKind::Absolute;
                if d.kind == PointerKind::Touchscreen
                    && let Some(t) = d.touch
                {
                    // A touch is the left button, where the finger is (the move first).
                    d.buttons.insert(0, (Button::Left, t));
                }
            }
        }
        self.clamp();
        let (x, y) = self.at();
        if (x, y) != before {
            out.push(Pointer::Move { x, y });
        }
        for (button, down) in d.buttons.drain(..) {
            out.push(if down { Pointer::Press { x, y, button } } else { Pointer::Release { x, y, button } });
        }
        if d.wheel != 0 {
            out.push(Pointer::Wheel { x, y, notches: d.wheel });
        }
        d.dx = 0;
        d.dy = 0;
        d.wheel = 0;
        d.abs = (None, None);
        d.touch = None;
    }
}

impl Drop for Pointers {
    fn drop(&mut self) {
        for d in self.devs.iter().filter(|d| d.grabbed) {
            grab(d.file.as_raw_fd(), false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_axis_maps_onto_the_screen() {
        let a = Axis { min: 0, max: 32767 };
        assert_eq!(a.map(0, 1920), 0.0);
        assert_eq!(a.map(32767, 1920), 1919.0);
        let a = Axis { min: 100, max: 100 };
        assert!(a.map(100, 800).is_finite());
    }

    #[test]
    fn eviocgabs_and_eviocgprop() {
        assert_eq!(crate::ior(0x40, 24), 0x80184540);
        assert_eq!(crate::ior(0x09, 96), 0x80604509);
        assert_eq!(crate::ior(0x18, 96), 0x80604518);
    }
}
