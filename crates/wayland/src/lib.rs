//! A panel in a Wayland window, for running under a compositor (sway) as well as on a whole
//! screen (Warren, 2026-11-25: a compatibility, KMS staying the first target). What `lp-kms`
//! and `lp-keys` are on the console, this is in a window: two shared-memory buffers to copy a
//! frame into, and the keys and pointer as `lp-keys` gives them. `lp-panel`'s `run` uses it
//! through its `wayland` feature; a panel never calls it itself.
//!
//! By `wayland-client` alone: no toolkit, no xkb, no libwayland linked. `wl_keyboard` sends the
//! kernel's key codes, the ones `lp-keys` reads from evdev, so a [`Key`] is made by
//! [`lp_keys::Held`] as on the console, on the US layout as there. The compositor sends no
//! repeats: they're made here, from its `repeat_info`. The compositor's cursor is hidden and the
//! caller draws `lp-paint`'s, as on the console. Wheel clicks come by `axis_discrete`; a
//! touchpad's smooth scrolling is left out. Buffer scale 1: not tried on HiDPI.
//!
//! Tried headless, on a sway of its own with a virtual pointer and keyboard, by `../sway-try`'s
//! `headless.sh`: a window's pixels are the panel's in memory exactly.

use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};
use std::time::{Duration, Instant};

use lp_keys::{Button, Held, Key, Pointer, State};
use lp_paint::Surface;
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{
    wl_buffer::{self, WlBuffer},
    wl_callback::{self, WlCallback},
    wl_compositor::WlCompositor,
    wl_keyboard::{self, WlKeyboard},
    wl_pointer::{self, WlPointer},
    wl_registry::WlRegistry,
    wl_seat::{self, WlSeat},
    wl_shm::{self, WlShm},
    wl_shm_pool::WlShmPool,
    wl_surface::WlSurface,
};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle, WEnum, delegate_noop};
use wayland_protocols::xdg::shell::client::{
    xdg_surface::{self, XdgSurface},
    xdg_toplevel::{self, XdgToplevel},
    xdg_wm_base::{self, XdgWmBase},
};

/// Whether a compositor is there to open a window in: `WAYLAND_DISPLAY` set.
pub fn available() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty())
}

/// What happened, in the order it came.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Key(Key),
    /// In the window's pixels, as `lp-keys`' pointers give them on a screen.
    Pointer(Pointer),
    /// "keyboard plugged in", as the console's loop says it.
    Plugged(String),
    /// A new size, or the first: draw again.
    Resized,
    /// The compositor asked the window to close.
    Closed,
}

/// The window a panel draws in.
pub struct Window {
    _conn: Connection,
    queue: EventQueue<Inner>,
    st: Inner,
}

/// Two buffers of one size in one shared mapping: one shown, one drawn.
struct Bufs {
    map: *mut u8,
    len: usize,
    w: usize,
    h: usize,
    buf: [WlBuffer; 2],
    busy: [bool; 2],
}

impl Bufs {
    fn new(shm: &WlShm, qh: &QueueHandle<Inner>, w: usize, h: usize) -> Result<Bufs, String> {
        let one = w * h * 4;
        let len = one * 2;
        // SAFETY: a name and flags; a new fd or -1.
        let fd = unsafe { libc::memfd_create(c"lp-wayland".as_ptr(), libc::MFD_CLOEXEC) };
        if fd < 0 {
            return Err(format!("memfd: {}", std::io::Error::last_os_error()));
        }
        // SAFETY: the fd was just made and is ours alone.
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        // SAFETY: an fd we own and a length.
        if unsafe { libc::ftruncate(fd.as_raw_fd(), len as _) } < 0 {
            return Err(format!("ftruncate: {}", std::io::Error::last_os_error()));
        }
        // SAFETY: a shared mapping of the whole of a file of `len` bytes.
        let map = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd.as_raw_fd(),
                0,
            )
        };
        if map == libc::MAP_FAILED {
            return Err(format!("mmap: {}", std::io::Error::last_os_error()));
        }
        let pool = shm.create_pool(fd.as_fd(), len as i32, qh, ());
        let mk = |i: usize| {
            let (off, stride) = ((i * one) as i32, (w * 4) as i32);
            pool.create_buffer(off, w as i32, h as i32, stride, wl_shm::Format::Xrgb8888, qh, i)
        };
        let buf = [mk(0), mk(1)];
        pool.destroy(); // the buffers keep the memory
        Ok(Bufs { map: map.cast(), len, w, h, buf, busy: [false; 2] })
    }

    fn free(&self) -> Option<usize> {
        (0..2).find(|&i| !self.busy[i])
    }

    /// Buffer `i`, the same layout as `lp-kms`'s: XRGB8888, `w` to a row.
    fn surface(&mut self, i: usize) -> Surface<'_> {
        let n = self.w * self.h;
        // SAFETY: buffer i is n u32s at i*n inside the mapping, and the compositor isn't
        // reading it (not busy).
        let px = unsafe { std::slice::from_raw_parts_mut(self.map.cast::<u32>().add(i * n), n) };
        Surface { w: self.w, h: self.h, stride: self.w, px }
    }
}

impl Drop for Bufs {
    fn drop(&mut self) {
        for b in &self.buf {
            b.destroy();
        }
        // SAFETY: the mapping made in `new`, of `len` bytes.
        unsafe { libc::munmap(self.map.cast(), self.len) };
    }
}

struct Repeat {
    code: u16,
    next: Instant,
}

// The dispatch state: wayland-client hands it to each object's events.
struct Inner {
    qh: QueueHandle<Inner>,
    shm: WlShm,
    surface: WlSurface,
    want_pointer: bool,
    keyboard: Option<WlKeyboard>,
    pointer: Option<WlPointer>,
    asked: (usize, usize),
    size: Option<(usize, usize)>,
    bufs: Option<Bufs>,
    frame_pending: bool,
    held: Held,
    repeat_rate: i32,
    repeat_delay: Duration,
    repeat: Option<Repeat>,
    cursor: Option<(i32, i32)>,
    events: Vec<Event>,
}

impl Inner {
    fn key(&mut self, c: u16, state: State) {
        let k = self.held.key(c, state);
        self.events.push(Event::Key(k));
        match state {
            State::Press if !lp_keys::is_modifier(c) && self.repeat_rate > 0 => {
                self.repeat = Some(Repeat { code: c, next: Instant::now() + self.repeat_delay });
            }
            State::Release if self.repeat.as_ref().is_some_and(|r| r.code == c) => self.repeat = None,
            _ => {}
        }
    }

    fn repeats(&mut self) {
        let now = Instant::now();
        if let Some(r) = &mut self.repeat
            && now >= r.next
        {
            let c = r.code;
            r.next = now + Duration::from_secs(1) / self.repeat_rate.max(1) as u32;
            self.key(c, State::Repeat);
        }
    }
}

impl Window {
    /// Opens a window called `title` (its app id too). `pointer`: take the pointer, else keys
    /// only, as `lp-panel`'s option. An error when there's no compositor, or it lacks what's
    /// needed (`wl_compositor`, `wl_shm`, `xdg_wm_base`).
    pub fn open(title: &str, pointer: bool) -> Result<Window, String> {
        let conn = Connection::connect_to_env().map_err(|e| format!("no Wayland: {e}"))?;
        let (globals, queue) = registry_queue_init::<Inner>(&conn).map_err(|e| e.to_string())?;
        let qh = queue.handle();
        let compositor: WlCompositor =
            globals.bind(&qh, 4..=6, ()).map_err(|e| format!("wl_compositor: {e}"))?;
        let shm: WlShm = globals.bind(&qh, 1..=1, ()).map_err(|e| format!("wl_shm: {e}"))?;
        let wm: XdgWmBase = globals.bind(&qh, 1..=5, ()).map_err(|e| format!("xdg_wm_base: {e}"))?;
        // Up to 7: axis_discrete, which 8 replaces.
        if globals.bind::<WlSeat, _, _>(&qh, 5..=7, ()).is_err() {
            eprintln!("{title}: no seat: no keys or pointer");
        }
        let surface = compositor.create_surface(&qh, ());
        let xs = wm.get_xdg_surface(&surface, &qh, ());
        let top = xs.get_toplevel(&qh, ());
        top.set_title(title.into());
        top.set_app_id(title.into());
        surface.commit();
        let st = Inner {
            qh,
            shm,
            surface,
            want_pointer: pointer,
            keyboard: None,
            pointer: None,
            asked: (0, 0),
            size: None,
            bufs: None,
            frame_pending: false,
            held: Held::default(),
            repeat_rate: 0,
            repeat_delay: Duration::from_millis(600),
            repeat: None,
            cursor: None,
            events: Vec::new(),
        };
        Ok(Window { _conn: conn, queue, st })
    }

    /// Waits up to `timeout` (less if a key's repeat is due) for the compositor or any of
    /// `others`, whose `revents` it fills in as `poll` does, and gives what happened.
    pub fn poll(&mut self, others: &mut [libc::pollfd], timeout: Duration) -> Result<Vec<Event>, String> {
        let err = |e: &dyn std::fmt::Display| e.to_string();
        self.queue.dispatch_pending(&mut self.st).map_err(|e| err(&e))?;
        self.queue.flush().map_err(|e| err(&e))?;
        let mut wait = timeout;
        if let Some(r) = &self.st.repeat {
            wait = wait.min(r.next.saturating_duration_since(Instant::now()));
        }
        // Something already waiting is given at once.
        if !self.st.events.is_empty() {
            wait = Duration::ZERO;
        }
        if let Some(guard) = self.queue.prepare_read() {
            let mut fds = Vec::with_capacity(others.len() + 1);
            fds.push(libc::pollfd {
                fd: guard.connection_fd().as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            });
            fds.extend(others.iter().map(|p| libc::pollfd { revents: 0, ..*p }));
            // SAFETY: a vector of pollfds of its own length.
            unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as _, wait.as_millis() as i32) };
            for (o, p) in others.iter_mut().zip(&fds[1..]) {
                o.revents = p.revents;
            }
            if fds[0].revents & libc::POLLIN != 0 {
                guard.read().map_err(|e| err(&e))?;
            }
        }
        self.queue.dispatch_pending(&mut self.st).map_err(|e| err(&e))?;
        self.st.repeats();
        Ok(std::mem::take(&mut self.st.events))
    }

    /// The window's size in pixels, once the compositor has said it.
    pub fn size(&self) -> Option<(usize, usize)> {
        self.st.size
    }

    /// Where the pointer is in the window, while it's over it and not hidden.
    pub fn cursor(&self) -> Option<(i32, i32)> {
        self.st.cursor
    }

    /// Hides the cursor until the pointer next moves, as typing does on the console.
    pub fn hide_cursor(&mut self) {
        self.st.cursor = None;
    }

    /// Whether a frame can be shown now: the size known, the last frame taken, a buffer free.
    pub fn ready(&self) -> bool {
        let st = &self.st;
        let Some(size) = st.size else { return false };
        !st.frame_pending && st.bufs.as_ref().is_none_or(|b| (b.w, b.h) != size || b.free().is_some())
    }

    /// Shows `frame`, the window's size, with the cursor's arrow on top at `cursor_px` a pixel
    /// (as the console's loop draws it). False when it isn't [`ready`](Self::ready) or `frame`
    /// is another size, and nothing was shown.
    pub fn present(&mut self, frame: &Surface, cursor_px: i32) -> Result<bool, String> {
        if !self.ready() {
            return Ok(false);
        }
        let st = &mut self.st;
        let (w, h) = st.size.unwrap();
        if (frame.w, frame.h) != (w, h) {
            return Ok(false);
        }
        if st.bufs.as_ref().is_none_or(|b| (b.w, b.h) != (w, h)) {
            st.bufs = None;
            st.bufs = Some(Bufs::new(&st.shm, &st.qh, w, h)?);
        }
        let b = st.bufs.as_mut().unwrap();
        let i = b.free().unwrap();
        let mut sf = b.surface(i);
        sf.copy_from(frame);
        if let Some((x, y)) = st.cursor {
            lp_paint::cursor(&mut sf, x, y, cursor_px);
        }
        b.busy[i] = true;
        st.surface.attach(Some(&b.buf[i]), 0, 0);
        st.surface.damage_buffer(0, 0, w as i32, h as i32);
        st.surface.frame(&st.qh, ());
        st.surface.commit();
        st.frame_pending = true;
        Ok(true)
    }
}

impl Dispatch<WlRegistry, GlobalListContents> for Inner {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as wayland_client::Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(Inner: ignore WlCompositor);
delegate_noop!(Inner: ignore WlShm);
delegate_noop!(Inner: ignore WlShmPool);
delegate_noop!(Inner: ignore WlSurface);

impl Dispatch<WlBuffer, usize> for Inner {
    fn event(
        st: &mut Self,
        _: &WlBuffer,
        e: wl_buffer::Event,
        i: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_buffer::Event::Release = e
            && let Some(b) = &mut st.bufs
        {
            b.busy[*i] = false;
        }
    }
}

impl Dispatch<WlCallback, ()> for Inner {
    fn event(
        st: &mut Self,
        _: &WlCallback,
        e: wl_callback::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { .. } = e {
            st.frame_pending = false;
        }
    }
}

impl Dispatch<XdgWmBase, ()> for Inner {
    fn event(
        _: &mut Self,
        wm: &XdgWmBase,
        e: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = e {
            wm.pong(serial);
        }
    }
}

impl Dispatch<XdgSurface, ()> for Inner {
    fn event(
        st: &mut Self,
        xs: &XdgSurface,
        e: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = e {
            xs.ack_configure(serial);
            let (w, h) = st.asked;
            // No size asked: the window's own choice, a 16:9 that fits most screens.
            let size = if w == 0 || h == 0 { (1280, 720) } else { (w, h) };
            if st.size != Some(size) {
                st.size = Some(size);
            }
            // A configure wants a buffer committed, frame callback or not.
            st.frame_pending = false;
            st.events.push(Event::Resized);
        }
    }
}

impl Dispatch<XdgToplevel, ()> for Inner {
    fn event(
        st: &mut Self,
        _: &XdgToplevel,
        e: xdg_toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match e {
            xdg_toplevel::Event::Configure { width, height, .. } => {
                st.asked = (width.max(0) as usize, height.max(0) as usize);
            }
            xdg_toplevel::Event::Close => st.events.push(Event::Closed),
            _ => {}
        }
    }
}

impl Dispatch<WlSeat, ()> for Inner {
    fn event(
        st: &mut Self,
        seat: &WlSeat,
        e: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        // The last keyboard or pointer unplugged takes the capability away, and the objects with
        // it: let go of them, and ask again when one's plugged in. Without this a keyboard
        // plugged in later is never heard (found by sway-try's test).
        let wl_seat::Event::Capabilities { capabilities: WEnum::Value(c) } = e else { return };
        let has_kb = c.contains(wl_seat::Capability::Keyboard);
        if has_kb && st.keyboard.is_none() {
            st.keyboard = Some(seat.get_keyboard(qh, ()));
            st.events.push(Event::Plugged("keyboard plugged in".into()));
        } else if !has_kb && let Some(k) = st.keyboard.take() {
            k.release();
            st.held.clear();
            st.repeat = None;
        }
        let has_ptr = c.contains(wl_seat::Capability::Pointer) && st.want_pointer;
        if has_ptr && st.pointer.is_none() {
            st.pointer = Some(seat.get_pointer(qh, ()));
        } else if !has_ptr && let Some(p) = st.pointer.take() {
            p.release();
            st.cursor = None;
        }
    }
}

impl Dispatch<WlKeyboard, ()> for Inner {
    fn event(
        st: &mut Self,
        _: &WlKeyboard,
        e: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match e {
            // The keymap is xkb's; the codes are evdev's, which is what lp-keys reads. Dropped.
            wl_keyboard::Event::Keymap { fd, .. } => drop(fd),
            // Keys already down count as held, as on the console.
            wl_keyboard::Event::Enter { keys, .. } => {
                st.held.clear();
                for c in keys.as_chunks::<4>().0.iter().map(|b| u32::from_ne_bytes(*b)) {
                    st.held.set(c.min(u16::MAX as u32) as u16, true);
                }
            }
            wl_keyboard::Event::Leave { .. } => {
                st.held.clear();
                st.repeat = None;
            }
            wl_keyboard::Event::Key { key, state, .. } => {
                let s = match state {
                    WEnum::Value(wl_keyboard::KeyState::Pressed) => State::Press,
                    _ => State::Release,
                };
                if let Ok(c) = u16::try_from(key) {
                    st.key(c, s);
                }
            }
            wl_keyboard::Event::RepeatInfo { rate, delay } => {
                st.repeat_rate = rate;
                st.repeat_delay = Duration::from_millis(delay.max(0) as u64);
            }
            _ => {}
        }
    }
}

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;

impl Dispatch<WlPointer, ()> for Inner {
    fn event(
        st: &mut Self,
        ptr: &WlPointer,
        e: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let at = |sx: f64, sy: f64| (sx.floor() as i32, sy.floor() as i32);
        let moved = |st: &mut Self, x: i32, y: i32| {
            st.cursor = Some((x, y));
            st.events.push(Event::Pointer(Pointer::Move { x, y }));
        };
        match e {
            wl_pointer::Event::Enter { serial, surface_x, surface_y, .. } => {
                // No cursor of the compositor's: the caller draws lp-paint's, as on the console.
                ptr.set_cursor(serial, None, 0, 0);
                let (x, y) = at(surface_x, surface_y);
                moved(st, x, y);
            }
            wl_pointer::Event::Leave { .. } => st.cursor = None,
            wl_pointer::Event::Motion { surface_x, surface_y, .. } => {
                let (x, y) = at(surface_x, surface_y);
                moved(st, x, y);
            }
            wl_pointer::Event::Button { button, state, .. } => {
                let Some((x, y)) = st.cursor else { return };
                let button = match button {
                    BTN_LEFT => Button::Left,
                    BTN_RIGHT => Button::Right,
                    BTN_MIDDLE => Button::Middle,
                    _ => return,
                };
                let p = match state {
                    WEnum::Value(wl_pointer::ButtonState::Pressed) => Pointer::Press { x, y, button },
                    _ => Pointer::Release { x, y, button },
                };
                st.events.push(Event::Pointer(p));
            }
            // A wheel's clicks; Wayland's down is positive, lp-keys' up.
            wl_pointer::Event::AxisDiscrete {
                axis: WEnum::Value(wl_pointer::Axis::VerticalScroll),
                discrete,
            } => {
                let Some((x, y)) = st.cursor else { return };
                st.events.push(Event::Pointer(Pointer::Wheel { x, y, notches: -discrete }));
            }
            _ => {}
        }
    }
}
