//! A panel's life on a whole screen: the loop every tool's panel runs, around what the panel
//! itself shows and does.
//!
//! The loop waits in one `poll` for the keys, the pointers, a keyboard or pointer plugged in,
//! the screen's flips, and the [`Wake`] pipe a panel's own threads write to (its server's
//! messages, say). It gives the card and the keys up when another VT is switched to, and takes
//! them back after (`SIGUSR1` and `SIGUSR2`); it quits on `SIGINT`, `SIGTERM` and `SIGHUP`, or
//! on [`quit`]. A frame is drawn in memory only when the panel says something changed, then
//! copied to the screen with the pointer's arrow on top.
//!
//! What's shown, what the keys do and what's said to the server are the panel's: a
//! [`Panel`]. Moved here from dexter-kms's `main.rs` (2026-11-07), for Tape's panel and the
//! ones after it.

use lp_keys::{Hotplug, Key, Keyboards, PointerKind, Pointers};
use lp_paint::{Image, Surface};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub use lp_keys::Pointer;

/// What a tool's panel does, called by [`run`] (and by the snapshot harness).
pub trait Panel {
    /// The loop has come round, woken or not: take what the panel's threads have sent. Whether
    /// anything changed that shows.
    fn take(&mut self) -> bool;
    /// A key pressed, repeated or let go. Whether anything changed that shows. `ctl` switches
    /// the VT (Ctrl+Alt+F*n*: [`vt_key`]).
    fn key(&mut self, k: &Key, ctl: &mut Ctl) -> bool;
    /// The pointer, on a screen of `w` × `h`. Whether anything changed that shows.
    fn point(&mut self, e: Pointer, w: usize, h: usize) -> bool;
    /// A keyboard or pointer plugged in while running, said as "keyboard plugged in: NAME".
    fn plugged(&mut self, said: String) -> bool {
        let _ = said;
        false
    }
    /// After the keys and the pointer, each time round: anything waiting on time (a message
    /// that fades, a key held). Whether anything changed that shows.
    fn tick(&mut self) -> bool {
        false
    }
    /// The whole screen, drawn into memory.
    fn draw(&mut self, s: &mut Surface);
}

/// What a key can ask of the loop.
#[derive(Debug, Default)]
pub struct Ctl {
    vt: Option<i32>,
}

impl Ctl {
    /// Switches to VT `n` (from 1) once the key is done with.
    pub fn switch_vt(&mut self, n: i32) {
        self.vt = Some(n);
    }
}

/// The VT Ctrl+Alt+F*n* switches to, as the console's own keys would: grabbed keys never
/// reach the kernel's.
pub fn vt_key(k: &Key) -> Option<i32> {
    use lp_keys::code::{F1, F10, F11, F12};
    if !(k.mods.ctrl && k.mods.alt) || !k.down() {
        return None;
    }
    match k.code {
        F1..=F10 => Some((k.code - F1 + 1) as i32),
        F11 => Some(11),
        F12 => Some(12),
        _ => None,
    }
}

/// How the loop runs.
#[derive(Clone, Debug)]
pub struct Options<'a> {
    /// The program's name, for what it says on stderr.
    pub name: &'a str,
    /// The card (default: the first with a screen connected).
    pub card: Option<&'a Path>,
    /// The output, as `lp_kms::outputs` names it (default: the first connected).
    pub output: Option<&'a str>,
    /// The keyboards and pointers grabbed, so keys typed reach the panel alone.
    pub grab: bool,
    /// The mice, touchpads and touchscreens read; else keys only.
    pub pointer: bool,
    /// Under muntin, [`lp_keys::FRAME_KEY`] quits before the panel sees it.
    pub frame_key: bool,
    /// The panel's own way out, for what's said when it isn't on a console ("q or Ctrl+C").
    pub quit_keys: &'a str,
}

impl Default for Options<'_> {
    fn default() -> Self {
        Options {
            name: "panel",
            card: None,
            output: None,
            grab: true,
            pointer: true,
            frame_key: true,
            quit_keys: "Ctrl+C",
        }
    }
}

static QUIT: AtomicBool = AtomicBool::new(false);
static RELEASE: AtomicBool = AtomicBool::new(false);
static ACQUIRE: AtomicBool = AtomicBool::new(false);

/// Ends [`run`] after this time round, as SIGTERM does.
pub fn quit() {
    QUIT.store(true, Ordering::SeqCst);
}

/// Whether [`quit`] has been called, or a signal asked.
pub fn quitting() -> bool {
    QUIT.load(Ordering::SeqCst)
}

extern "C" fn on_signal(sig: libc::c_int) {
    match sig {
        libc::SIGUSR1 => RELEASE.store(true, Ordering::SeqCst),
        libc::SIGUSR2 => ACQUIRE.store(true, Ordering::SeqCst),
        _ => QUIT.store(true, Ordering::SeqCst),
    }
}

fn install_signals() {
    for s in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP, libc::SIGUSR1, libc::SIGUSR2] {
        // SAFETY: the handler only stores to atomics.
        unsafe {
            let mut sa: libc::sigaction = std::mem::zeroed();
            sa.sa_sigaction = on_signal as *const () as usize;
            libc::sigemptyset(&mut sa.sa_mask);
            libc::sigaction(s, &sa, std::ptr::null_mut());
        }
    }
}

/// A pipe that wakes the loop's `poll`: [`Waker::wake`] from any thread, after sending the
/// panel something, and the panel's [`Panel::take`] is called.
pub struct Wake {
    r: OwnedFd,
    w: Arc<OwnedFd>,
}

/// The writing end of a [`Wake`], for a panel's threads.
#[derive(Clone)]
pub struct Waker(Arc<OwnedFd>);

impl Wake {
    pub fn new() -> io::Result<Wake> {
        let mut fds = [0; 2];
        // SAFETY: two ints for the two ends.
        if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_NONBLOCK | libc::O_CLOEXEC) } != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: both ends are new and ours alone.
        let (r, w) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
        Ok(Wake { r, w: Arc::new(w) })
    }

    pub fn waker(&self) -> Waker {
        Waker(self.w.clone())
    }

    /// The end to poll.
    pub fn fd(&self) -> RawFd {
        self.r.as_raw_fd()
    }

    /// Everything written, read and forgotten.
    pub fn drain(&self) {
        let mut b = [0u8; 256];
        // SAFETY: reading into a stack buffer of its length, non-blocking.
        while unsafe { libc::read(self.fd(), b.as_mut_ptr().cast(), b.len()) } > 0 {}
    }
}

impl Waker {
    pub fn wake(&self) {
        // SAFETY: one byte into the pipe's write end; a full pipe already wakes the loop.
        unsafe { libc::write(self.0.as_raw_fd(), b"x".as_ptr().cast(), 1) };
    }
}

/// Whether the loop takes `k` itself, to quit for the frame.
fn frame_quits(o: &Options, under: bool, k: &Key) -> bool {
    o.frame_key && under && lp_keys::is_frame_key(k)
}

/// How long `poll` waits with nothing happening, so [`Panel::tick`] still comes round.
const TICK: i32 = 250;

/// Runs `panel` on the screen until [`quit`] or a signal: the console in graphics mode, the
/// screen opened, the keys (and pointers) taken. Everything is put back on the way out, the
/// screen before the console's mode. An error if the screen or the keys can't be had.
pub fn run(o: &Options, wake: &Wake, panel: &mut impl Panel) -> Result<(), String> {
    install_signals();
    let name = o.name;
    // Declared before the screen, so dropped after it: the screen is put back before the VT's
    // mode is.
    let mut console = lp_kms::Console::graphics();
    let vt_ok = console.switch_by_signal(libc::SIGUSR1, libc::SIGUSR2);
    let opts = lp_kms::Options { card: o.card, output: o.output };
    let mut scr = lp_kms::Screen::open(&opts).map_err(|e| e.to_string())?;
    let mut keys = Keyboards::open(o.grab).map_err(|e| e.to_string())?;
    if o.grab && !vt_ok {
        // Grabbed keys never reach the kernel, and with no console to switch through, Ctrl+Alt+F*n*
        // can't work: say how out.
        eprintln!("{name}: not on a console: {} quits, VT switching is off", o.quit_keys);
    }
    let (w, h) = scr.size();
    eprintln!("{name}: {} at {w}×{h} @ {} Hz", scr.output(), scr.mode().vrefresh);
    let mut ptr = if !o.pointer {
        None
    } else {
        match Pointers::open(o.grab, w, h) {
            Ok(p) => {
                for (path, kind, _) in p.devices() {
                    eprintln!("{name}: {kind:?} {}", path.display());
                }
                Some(p)
            }
            Err(e) => {
                eprintln!("{name}: no pointer: {e}");
                None
            }
        }
    };
    let cursor_px = (h as i32 / 700).max(1);
    if keys.is_empty() {
        eprintln!("{name}: no keyboard yet: plug one in");
    }
    // Keyboards and pointers plugged in later.
    let mut plug = Hotplug::new().map_err(|e| eprintln!("{name}: no hotplug: {e}")).ok();
    let under = lp_keys::under_frame();

    let mut canvas = Image::new(w, h);
    let mut away = false;
    let mut dirty = true;
    let mut shown_cursor = None;

    while !quitting() {
        if RELEASE.swap(false, Ordering::SeqCst) {
            keys.set_grab(false);
            if let Some(p) = &mut ptr {
                p.set_grab(false);
            }
            let _ = scr.suspend();
            console.released();
            away = true;
        }
        if ACQUIRE.swap(false, Ordering::SeqCst) {
            console.acquired();
            let _ = scr.resume();
            keys.set_grab(o.grab);
            if let Some(p) = &mut ptr {
                p.set_grab(o.grab);
            }
            away = false;
            dirty = true;
        }

        let mut fds: Vec<libc::pollfd> =
            vec![libc::pollfd { fd: wake.fd(), events: libc::POLLIN, revents: 0 }];
        let mut inputs = keys.fds();
        inputs.extend(ptr.as_ref().map(|p| p.fds()).unwrap_or_default());
        inputs.extend(plug.as_ref().map(|h| h.fd()));
        fds.extend(inputs.into_iter().map(|fd| libc::pollfd { fd, events: libc::POLLIN, revents: 0 }));
        fds.push(libc::pollfd { fd: scr.fd(), events: libc::POLLIN, revents: 0 });
        // SAFETY: a vector of pollfds of its own length.
        unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as _, TICK) };
        if fds.last().is_some_and(|p| p.revents & libc::POLLIN != 0) {
            let _ = scr.read_events();
        }
        wake.drain();
        dirty |= panel.take();
        let pressed = keys.read();
        let pointed = ptr.as_mut().map(|p| p.read()).unwrap_or_default();
        // After the reads, so a device unplugged is gone before one plugged in takes its name.
        for path in plug.as_mut().map(|h| h.read()).unwrap_or_default() {
            let said = if let Some(n) = keys.add(&path) {
                format!("keyboard plugged in: {n}")
            } else if let Some((kind, n)) = ptr.as_mut().and_then(|p| p.add(&path)) {
                let k = match kind {
                    PointerKind::Mouse => "mouse",
                    PointerKind::Touchpad => "touchpad",
                    PointerKind::Touchscreen => "touchscreen",
                    PointerKind::Absolute => "pointer",
                };
                format!("{k} plugged in: {n}")
            } else {
                continue;
            };
            panel.plugged(said);
            dirty = true;
        }
        if away {
            continue; // read only to be let go of: another VT has them
        }
        if !pressed.is_empty()
            && let Some(p) = &mut ptr
        {
            p.hide();
        }
        for k in pressed {
            if frame_quits(o, under, &k) {
                quit();
                break;
            }
            let mut ctl = Ctl::default();
            dirty |= panel.key(&k, &mut ctl);
            if let Some(n) = ctl.vt {
                console.activate(n);
            }
        }
        for e in pointed {
            dirty |= panel.point(e, w, h);
        }
        dirty |= panel.tick();

        let cursor = ptr.as_ref().and_then(|p| p.cursor());
        if dirty || cursor != shown_cursor {
            // Drawn in memory, then copied: a KMS buffer is slow to read, and blending reads. The
            // pointer goes on the copy, so moving it doesn't draw the panel again.
            if dirty {
                panel.draw(&mut canvas.surface());
            }
            let fr = scr.back().map_err(|e| e.to_string())?;
            let mut sf = Surface { w: fr.width, h: fr.height, stride: fr.stride, px: fr.px };
            sf.copy_from(&canvas.surface());
            if let Some((x, y)) = cursor {
                lp_paint::cursor(&mut sf, x, y, cursor_px);
            }
            scr.present().map_err(|e| e.to_string())?;
            dirty = false;
            shown_cursor = cursor;
        }
    }
    drop(keys);
    drop(ptr);
    drop(scr);
    drop(console);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_keys::{Mods, State, code};

    fn key(code: u16, state: State, ctrl: bool, alt: bool) -> Key {
        Key { code, state, mods: Mods { ctrl, alt, ..Default::default() }, ch: None }
    }

    #[test]
    fn a_wake_wakes_a_poll_until_drained() {
        let w = Wake::new().unwrap();
        let poll = |w: &Wake| {
            let mut p = libc::pollfd { fd: w.fd(), events: libc::POLLIN, revents: 0 };
            // SAFETY: one pollfd.
            unsafe { libc::poll(&mut p, 1, 0) };
            p.revents & libc::POLLIN != 0
        };
        assert!(!poll(&w));
        let k = w.waker();
        std::thread::spawn(move || {
            for _ in 0..1000 {
                k.wake();
            }
        })
        .join()
        .unwrap();
        assert!(poll(&w));
        w.drain();
        assert!(!poll(&w));
    }

    #[test]
    fn ctrl_alt_f_switches_vt() {
        assert_eq!(vt_key(&key(code::F1, State::Press, true, true)), Some(1));
        assert_eq!(vt_key(&key(code::F10, State::Press, true, true)), Some(10));
        assert_eq!(vt_key(&key(code::F12, State::Press, true, true)), Some(12));
        assert_eq!(vt_key(&key(code::F1 + 2, State::Repeat, true, true)), Some(3));
        assert_eq!(vt_key(&key(code::F1 + 2, State::Release, true, true)), None);
        assert_eq!(vt_key(&key(code::F1 + 2, State::Press, true, false)), None);
        assert_eq!(vt_key(&key(code::ESC, State::Press, true, true)), None);
    }

    #[test]
    fn the_frame_key_quits_only_under_the_frame_and_when_asked() {
        let fk = key(code::ESC, State::Press, true, true);
        let o = Options::default();
        assert!(frame_quits(&o, true, &fk));
        assert!(!frame_quits(&o, false, &fk));
        assert!(!frame_quits(&Options { frame_key: false, ..o.clone() }, true, &fk));
        assert!(!frame_quits(&o, true, &key(code::ESC, State::Press, false, false)));
    }
}
