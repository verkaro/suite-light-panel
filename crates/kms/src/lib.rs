//! A whole screen through the kernel's mode setting (DRM/KMS), with no display server: the first
//! connected output at its preferred mode, two dumb buffers in XRGB8888, and page flips on the
//! vertical blank. Nothing between the program and the card but the kernel.
//!
//! Only `libc`: the few ioctls this needs are declared here, from `include/uapi/drm/drm.h` and
//! `drm_mode.h`, for 64-bit Linux.
//!
//! The process must be the card's master: the first to open it when nothing else holds it (no
//! compositor running), as a member of `video`. The console it runs on is put in graphics mode
//! while the screen is held ([`Console`]), so the kernel's text doesn't draw over it.

use std::fs::{File, OpenOptions};
use std::io;
use std::os::fd::{AsRawFd, RawFd};
use std::path::Path;

// ---------- ioctl numbers ----------

const fn ioc(dir: u64, nr: u64, size: usize) -> u64 {
    (dir << 30) | ((size as u64) << 16) | ((b'd' as u64) << 8) | nr
}
const fn iowr<T>(nr: u64) -> u64 {
    ioc(3, nr, std::mem::size_of::<T>())
}
const fn io(nr: u64) -> u64 {
    ioc(0, nr, 0)
}

const SET_MASTER: u64 = io(0x1e);
const DROP_MASTER: u64 = io(0x1f);
const GET_CAP: u64 = iowr::<GetCap>(0x0c);
const GETRESOURCES: u64 = iowr::<CardRes>(0xA0);
const GETCRTC: u64 = iowr::<Crtc>(0xA1);
const SETCRTC: u64 = iowr::<Crtc>(0xA2);
const GETENCODER: u64 = iowr::<Encoder>(0xA6);
const GETCONNECTOR: u64 = iowr::<Connector>(0xA7);
const ADDFB: u64 = iowr::<FbCmd>(0xAE);
const RMFB: u64 = iowr::<u32>(0xAF);
const PAGE_FLIP: u64 = iowr::<PageFlip>(0xB0);
const CREATE_DUMB: u64 = iowr::<CreateDumb>(0xB2);
const MAP_DUMB: u64 = iowr::<MapDumb>(0xB3);
const DESTROY_DUMB: u64 = iowr::<u32>(0xB4);

const CAP_DUMB_BUFFER: u64 = 0x1;
const PAGE_FLIP_EVENT: u32 = 0x1;
const EVENT_FLIP_COMPLETE: u32 = 0x02;
const CONNECTED: u32 = 1;
const MODE_TYPE_PREFERRED: u32 = 1 << 3;

#[repr(C)]
#[derive(Default)]
struct GetCap {
    capability: u64,
    value: u64,
}

#[repr(C)]
#[derive(Default)]
struct CardRes {
    fb_id_ptr: u64,
    crtc_id_ptr: u64,
    connector_id_ptr: u64,
    encoder_id_ptr: u64,
    count_fbs: u32,
    count_crtcs: u32,
    count_connectors: u32,
    count_encoders: u32,
    min_width: u32,
    max_width: u32,
    min_height: u32,
    max_height: u32,
}

/// A display mode, as the kernel describes it.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct ModeInfo {
    pub clock: u32,
    pub hdisplay: u16,
    pub hsync_start: u16,
    pub hsync_end: u16,
    pub htotal: u16,
    pub hskew: u16,
    pub vdisplay: u16,
    pub vsync_start: u16,
    pub vsync_end: u16,
    pub vtotal: u16,
    pub vscan: u16,
    pub vrefresh: u32,
    pub flags: u32,
    pub kind: u32,
    pub name: [u8; 32],
}

impl ModeInfo {
    pub fn name(&self) -> String {
        let n = self.name.iter().position(|&b| b == 0).unwrap_or(32);
        String::from_utf8_lossy(&self.name[..n]).into_owned()
    }
}

#[repr(C)]
#[derive(Default)]
struct Crtc {
    set_connectors_ptr: u64,
    count_connectors: u32,
    crtc_id: u32,
    fb_id: u32,
    x: u32,
    y: u32,
    gamma_size: u32,
    mode_valid: u32,
    mode: ModeInfo,
}

#[repr(C)]
#[derive(Default)]
struct Encoder {
    encoder_id: u32,
    encoder_type: u32,
    crtc_id: u32,
    possible_crtcs: u32,
    possible_clones: u32,
}

#[repr(C)]
#[derive(Default)]
struct Connector {
    encoders_ptr: u64,
    modes_ptr: u64,
    props_ptr: u64,
    prop_values_ptr: u64,
    count_modes: u32,
    count_props: u32,
    count_encoders: u32,
    encoder_id: u32,
    connector_id: u32,
    connector_type: u32,
    connector_type_id: u32,
    connection: u32,
    mm_width: u32,
    mm_height: u32,
    subpixel: u32,
    pad: u32,
}

#[repr(C)]
#[derive(Default)]
struct FbCmd {
    fb_id: u32,
    width: u32,
    height: u32,
    pitch: u32,
    bpp: u32,
    depth: u32,
    handle: u32,
}

#[repr(C)]
#[derive(Default)]
struct PageFlip {
    crtc_id: u32,
    fb_id: u32,
    flags: u32,
    reserved: u32,
    user_data: u64,
}

#[repr(C)]
#[derive(Default)]
struct CreateDumb {
    height: u32,
    width: u32,
    bpp: u32,
    flags: u32,
    handle: u32,
    pitch: u32,
    size: u64,
}

#[repr(C)]
#[derive(Default)]
struct MapDumb {
    handle: u32,
    pad: u32,
    offset: u64,
}

fn ioctl<T>(fd: RawFd, req: u64, arg: &mut T) -> io::Result<()> {
    loop {
        // SAFETY: `arg` is a #[repr(C)] struct of the size encoded in `req`.
        let r = unsafe { libc::ioctl(fd, req as _, arg as *mut T) };
        if r == 0 {
            return Ok(());
        }
        let e = io::Error::last_os_error();
        if !matches!(e.raw_os_error(), Some(libc::EINTR) | Some(libc::EAGAIN)) {
            return Err(e);
        }
    }
}

fn ptr<T>(v: &mut [T]) -> u64 {
    v.as_mut_ptr() as u64
}

// ---------- what's on the card ----------

/// The kind of output, for a person to read (`DRM_MODE_CONNECTOR_*`).
pub fn connector_kind(t: u32) -> &'static str {
    match t {
        1 => "VGA",
        2..=4 => "DVI",
        5 => "Composite",
        6 => "SVIDEO",
        7 => "LVDS",
        8 => "Component",
        9 => "DIN",
        10 => "DP",
        11 => "HDMI-A",
        12 => "HDMI-B",
        13 => "TV",
        14 => "eDP",
        15 => "Virtual",
        16 => "DSI",
        17 => "DPI",
        18 => "Writeback",
        19 => "SPI",
        20 => "USB",
        _ => "Unknown",
    }
}

/// One output of the card, as probed.
#[derive(Clone, Debug)]
pub struct Output {
    pub id: u32,
    /// `HDMI-A-1`, as `/sys/class/drm` names it.
    pub name: String,
    pub connected: bool,
    pub modes: Vec<ModeInfo>,
    pub encoder: u32,
    pub encoders: Vec<u32>,
    pub mm: (u32, u32),
}

impl Output {
    /// The mode the screen prefers, else its first (the largest, by the kernel's order).
    pub fn preferred(&self) -> Option<ModeInfo> {
        self.modes.iter().find(|m| m.kind & MODE_TYPE_PREFERRED != 0).or(self.modes.first()).copied()
    }
}

struct Resources {
    crtcs: Vec<u32>,
    connectors: Vec<u32>,
}

fn resources(fd: RawFd) -> io::Result<Resources> {
    let mut r = CardRes::default();
    ioctl(fd, GETRESOURCES, &mut r)?;
    let mut fbs = vec![0u32; r.count_fbs as usize];
    let mut crtcs = vec![0u32; r.count_crtcs as usize];
    let mut connectors = vec![0u32; r.count_connectors as usize];
    let mut encoders = vec![0u32; r.count_encoders as usize];
    r.fb_id_ptr = ptr(&mut fbs);
    r.crtc_id_ptr = ptr(&mut crtcs);
    r.connector_id_ptr = ptr(&mut connectors);
    r.encoder_id_ptr = ptr(&mut encoders);
    ioctl(fd, GETRESOURCES, &mut r)?;
    crtcs.truncate(r.count_crtcs as usize);
    connectors.truncate(r.count_connectors as usize);
    Ok(Resources { crtcs, connectors })
}

fn output(fd: RawFd, id: u32) -> io::Result<Output> {
    // The counts can grow between the two calls (a screen plugged in): ask until they hold.
    loop {
        let mut c = Connector { connector_id: id, ..Default::default() };
        ioctl(fd, GETCONNECTOR, &mut c)?;
        let (nm, ne) = (c.count_modes, c.count_encoders);
        let mut modes = vec![ModeInfo::default(); nm as usize];
        let mut encoders = vec![0u32; ne as usize];
        let mut c2 = Connector {
            connector_id: id,
            count_modes: nm,
            count_encoders: ne,
            modes_ptr: ptr(&mut modes),
            encoders_ptr: ptr(&mut encoders),
            ..Default::default()
        };
        ioctl(fd, GETCONNECTOR, &mut c2)?;
        if c2.count_modes > nm || c2.count_encoders > ne {
            continue;
        }
        modes.truncate(c2.count_modes as usize);
        encoders.truncate(c2.count_encoders as usize);
        return Ok(Output {
            id,
            name: format!("{}-{}", connector_kind(c2.connector_type), c2.connector_type_id),
            connected: c2.connection == CONNECTED,
            modes,
            encoder: c2.encoder_id,
            encoders,
            mm: (c2.mm_width, c2.mm_height),
        });
    }
}

/// Every output of the card at `path`, probed. Needs no master: it's safe beside a compositor.
pub fn outputs(path: &Path) -> io::Result<Vec<Output>> {
    let f = OpenOptions::new().read(true).write(true).open(path)?;
    let res = resources(f.as_raw_fd())?;
    res.connectors.iter().map(|&id| output(f.as_raw_fd(), id)).collect()
}

/// The first card under `/dev/dri` that has a connected output.
pub fn find_card() -> Option<std::path::PathBuf> {
    let mut cards: Vec<_> = std::fs::read_dir("/dev/dri")
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("card")))
        .collect();
    cards.sort();
    cards.into_iter().find(|p| outputs(p).is_ok_and(|o| o.iter().any(|o| o.connected)))
}

// ---------- the screen ----------

struct Buffer {
    handle: u32,
    fb: u32,
    pitch: u32,
    map: *mut u8,
    len: usize,
}

/// The screen held: one output at one mode, drawn by [`Screen::back`] and shown by
/// [`Screen::present`]. Dropping it puts back what the output showed before.
pub struct Screen {
    file: File,
    crtc: u32,
    connector: u32,
    mode: ModeInfo,
    output: String,
    bufs: [Buffer; 2],
    front: usize,
    flip_pending: bool,
    saved: Crtc,
}

/// The back buffer: `px` is `height` rows of `stride` pixels, XRGB8888, 0x00RRGGBB.
pub struct Frame<'a> {
    pub width: usize,
    pub height: usize,
    pub stride: usize,
    pub px: &'a mut [u32],
}

/// What to hold: the card (else the first with a screen) and the output (by its name, else the
/// first connected one).
#[derive(Default)]
pub struct Options<'a> {
    pub card: Option<&'a Path>,
    pub output: Option<&'a str>,
}

impl Screen {
    pub fn open(o: &Options) -> io::Result<Screen> {
        let path = match o.card {
            Some(p) => p.to_path_buf(),
            None => find_card().ok_or_else(|| other("no card under /dev/dri has a screen connected"))?,
        };
        let file = OpenOptions::new().read(true).write(true).open(&path).map_err(|e| {
            io::Error::new(e.kind(), format!("{}: {e} (is this user in the video group?)", path.display()))
        })?;
        let fd = file.as_raw_fd();
        // Opening the card makes this process its master if no one else is; ask anyway, in case
        // it was dropped (another VT gave it back). A compositor holding it refuses us here.
        if ioctl(fd, SET_MASTER, &mut 0u8).is_err() {
            return Err(other(&format!(
                "{}: can't be the card's master: is a display server running on it?",
                path.display()
            )));
        }
        let mut cap = GetCap { capability: CAP_DUMB_BUFFER, value: 0 };
        if ioctl(fd, GET_CAP, &mut cap).is_err() || cap.value == 0 {
            return Err(other("the card has no dumb buffers"));
        }
        let res = resources(fd)?;
        let outs: Vec<Output> = res.connectors.iter().map(|&id| output(fd, id)).collect::<io::Result<_>>()?;
        let out = match o.output {
            Some(n) => outs.iter().find(|x| x.name == n).ok_or_else(|| other(&format!("no output {n}")))?,
            None => outs.iter().find(|x| x.connected).ok_or_else(|| {
                let names: Vec<_> = outs.iter().map(|x| x.name.as_str()).collect();
                other(&format!("no screen connected ({} disconnected)", names.join(", ")))
            })?,
        };
        let mode = out.preferred().ok_or_else(|| other(&format!("{} has no modes", out.name)))?;
        let crtc = pick_crtc(fd, out, &res)?;
        let mut saved = Crtc { crtc_id: crtc, ..Default::default() };
        ioctl(fd, GETCRTC, &mut saved)?;

        let (w, h) = (mode.hdisplay as u32, mode.vdisplay as u32);
        let b0 = buffer(fd, w, h)?;
        let b1 = buffer(fd, w, h)?;
        let mut s = Screen {
            file,
            crtc,
            connector: out.id,
            mode,
            output: out.name.clone(),
            bufs: [b0, b1],
            front: 0,
            flip_pending: false,
            saved,
        };
        s.set_crtc(0)?;
        Ok(s)
    }

    fn set_crtc(&mut self, which: usize) -> io::Result<()> {
        let mut conn = [self.connector];
        let mut c = Crtc {
            set_connectors_ptr: ptr(&mut conn),
            count_connectors: 1,
            crtc_id: self.crtc,
            fb_id: self.bufs[which].fb,
            mode_valid: 1,
            mode: self.mode,
            ..Default::default()
        };
        ioctl(self.file.as_raw_fd(), SETCRTC, &mut c)?;
        self.front = which;
        Ok(())
    }

    pub fn size(&self) -> (usize, usize) {
        (self.mode.hdisplay as usize, self.mode.vdisplay as usize)
    }
    pub fn mode(&self) -> &ModeInfo {
        &self.mode
    }
    pub fn output(&self) -> &str {
        &self.output
    }
    /// For poll: readable when a flip has completed.
    pub fn fd(&self) -> RawFd {
        self.file.as_raw_fd()
    }

    /// The buffer not on screen, to draw the next frame in. Waits for a flip still pending, so
    /// the buffer drawn is never the one being scanned out.
    pub fn back(&mut self) -> io::Result<Frame<'_>> {
        self.wait_flip()?;
        let (w, h) = self.size();
        let b = &self.bufs[1 - self.front];
        let stride = b.pitch as usize / 4;
        // SAFETY: the map is `len` bytes, page-aligned, for as long as `self` lives.
        let px = unsafe { std::slice::from_raw_parts_mut(b.map as *mut u32, b.len / 4) };
        Ok(Frame { width: w, height: h, stride, px })
    }

    /// Shows the back buffer from the next vertical blank. Returns at once; the next
    /// [`Screen::back`] waits for it.
    pub fn present(&mut self) -> io::Result<()> {
        self.wait_flip()?;
        let next = 1 - self.front;
        let mut f = PageFlip {
            crtc_id: self.crtc,
            fb_id: self.bufs[next].fb,
            flags: PAGE_FLIP_EVENT,
            ..Default::default()
        };
        match ioctl(self.file.as_raw_fd(), PAGE_FLIP, &mut f) {
            Ok(()) => {
                self.flip_pending = true;
                self.front = next;
                Ok(())
            }
            // Some drivers refuse a flip after the VT came back: set the mode again.
            Err(_) => self.set_crtc(next),
        }
    }

    fn wait_flip(&mut self) -> io::Result<()> {
        let fd = self.file.as_raw_fd();
        while self.flip_pending {
            let mut p = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
            // SAFETY: one pollfd. A flip comes within a frame; a second is long enough to say
            // the card has stopped answering (the VT switched away) rather than hang.
            let r = unsafe { libc::poll(&mut p, 1, 1000) };
            if r == 0 {
                self.flip_pending = false;
                break;
            }
            if r < 0 {
                let e = io::Error::last_os_error();
                if e.raw_os_error() == Some(libc::EINTR) {
                    continue;
                }
                return Err(e);
            }
            self.read_events()?;
        }
        Ok(())
    }

    /// Reads the card's events; call when [`Screen::fd`] is readable.
    pub fn read_events(&mut self) -> io::Result<()> {
        let mut buf = [0u8; 1024];
        // SAFETY: reading into a stack buffer of its length.
        let n = unsafe { libc::read(self.file.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
        if n < 0 {
            let e = io::Error::last_os_error();
            return if e.raw_os_error() == Some(libc::EAGAIN) { Ok(()) } else { Err(e) };
        }
        let mut at = 0usize;
        while at + 8 <= n as usize {
            let kind = u32::from_ne_bytes(buf[at..at + 4].try_into().unwrap());
            let len = u32::from_ne_bytes(buf[at + 4..at + 8].try_into().unwrap()) as usize;
            if kind == EVENT_FLIP_COMPLETE {
                self.flip_pending = false;
            }
            at += len.max(8);
        }
        Ok(())
    }

    /// Lets go of the card for another: the VT switched away, or a frame handing the screen to a
    /// panel, which becomes master on opening the card. [`Screen::resume`] takes it back (master
    /// again, as this fd was before, and its last frame shown).
    pub fn suspend(&mut self) -> io::Result<()> {
        self.flip_pending = false;
        ioctl(self.file.as_raw_fd(), DROP_MASTER, &mut 0u8)
    }
    pub fn resume(&mut self) -> io::Result<()> {
        ioctl(self.file.as_raw_fd(), SET_MASTER, &mut 0u8)?;
        let f = self.front;
        self.set_crtc(f)
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        let fd = self.file.as_raw_fd();
        let _ = self.wait_flip();
        // Put back what was showing (the console's framebuffer, most often).
        if self.saved.mode_valid != 0 && self.saved.fb_id != 0 {
            let mut conn = [self.connector];
            let mut c = Crtc {
                set_connectors_ptr: ptr(&mut conn),
                count_connectors: 1,
                crtc_id: self.saved.crtc_id,
                fb_id: self.saved.fb_id,
                x: self.saved.x,
                y: self.saved.y,
                mode_valid: 1,
                mode: self.saved.mode,
                ..Default::default()
            };
            let _ = ioctl(fd, SETCRTC, &mut c);
        }
        for b in &self.bufs {
            // SAFETY: unmapping what `buffer` mapped.
            unsafe { libc::munmap(b.map.cast(), b.len) };
            let mut id = b.fb;
            let _ = ioctl(fd, RMFB, &mut id);
            let mut h = b.handle;
            let _ = ioctl(fd, DESTROY_DUMB, &mut h);
        }
    }
}

fn pick_crtc(fd: RawFd, out: &Output, res: &Resources) -> io::Result<u32> {
    // The CRTC already driving the output, else the first an encoder of it can use.
    if out.encoder != 0 {
        let mut e = Encoder { encoder_id: out.encoder, ..Default::default() };
        if ioctl(fd, GETENCODER, &mut e).is_ok() && e.crtc_id != 0 {
            return Ok(e.crtc_id);
        }
    }
    for &enc in &out.encoders {
        let mut e = Encoder { encoder_id: enc, ..Default::default() };
        if ioctl(fd, GETENCODER, &mut e).is_err() {
            continue;
        }
        for (i, &c) in res.crtcs.iter().enumerate() {
            if e.possible_crtcs & (1 << i) != 0 {
                return Ok(c);
            }
        }
    }
    Err(other(&format!("no CRTC can drive {}", out.name)))
}

fn buffer(fd: RawFd, w: u32, h: u32) -> io::Result<Buffer> {
    let mut c = CreateDumb { width: w, height: h, bpp: 32, ..Default::default() };
    ioctl(fd, CREATE_DUMB, &mut c)?;
    let mut f = FbCmd {
        width: w,
        height: h,
        pitch: c.pitch,
        bpp: 32,
        depth: 24,
        handle: c.handle,
        ..Default::default()
    };
    ioctl(fd, ADDFB, &mut f)?;
    let mut m = MapDumb { handle: c.handle, ..Default::default() };
    ioctl(fd, MAP_DUMB, &mut m)?;
    let len = c.size as usize;
    // SAFETY: mapping the dumb buffer at the offset the card gave, shared with it.
    let map = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            len,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED,
            fd,
            m.offset as _,
        )
    };
    if map == libc::MAP_FAILED {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: freshly mapped, `len` bytes.
    unsafe { std::ptr::write_bytes(map as *mut u8, 0, len) };
    Ok(Buffer { handle: c.handle, fb: f.fb_id, pitch: c.pitch, map: map.cast(), len })
}

fn other(msg: &str) -> io::Error {
    io::Error::other(msg.to_string())
}

// ---------- the console ----------

const KDSETMODE: u64 = 0x4B3A;
const KDGETMODE: u64 = 0x4B3B;
const VT_GETSTATE: u64 = 0x5603;
const KD_TEXT: libc::c_int = 0;
const KD_GRAPHICS: libc::c_int = 1;
const VT_SETMODE: u64 = 0x5602;
const VT_RELDISP: u64 = 0x5605;
const VT_ACTIVATE: u64 = 0x5606;
const VT_AUTO: libc::c_char = 0;
const VT_PROCESS: libc::c_char = 1;
const VT_ACKACQ: libc::c_int = 2;

#[repr(C)]
#[derive(Default)]
struct VtStat {
    v_active: libc::c_ushort,
    v_signal: libc::c_ushort,
    v_state: libc::c_ushort,
}

#[repr(C)]
struct VtMode {
    mode: libc::c_char,
    waitv: libc::c_char,
    relsig: libc::c_short,
    acqsig: libc::c_short,
    frsig: libc::c_short,
}

/// The Linux console the program runs on, in graphics mode while this lives: the kernel stops
/// drawing its text and cursor over the screen. Dropped, it puts back the mode it found: text
/// standing alone, graphics when a frame (muntin) ran this program and will draw again. If it
/// found graphics but the process that ran it is gone, it puts back text, so the screen is
/// never left with no one drawing it. On anything but a console (ssh, a terminal emulator) it
/// does nothing.
///
/// With [`Console::switch_by_signal`], a VT switch asks first: the kernel sends `release` when
/// another VT wants the screen, and the program gives up the card and the keys and calls
/// [`Console::released`]; it sends `acquire` on the way back, and the program calls
/// [`Console::acquired`] and takes them again. Keys grabbed never reach the kernel's own
/// Ctrl+Alt+F*n*, so the program switches with [`Console::activate`].
pub struct Console {
    fd: Option<RawFd>,
    process: bool,
    signals: (i32, i32),
    found: libc::c_int,
    parent: libc::pid_t,
}

impl Console {
    pub fn graphics() -> Console {
        let fd = 0;
        let mut found: libc::c_int = KD_TEXT;
        // SAFETY: ioctls on stdin, an int by pointer then by value; they fail harmlessly on
        // anything but a VT.
        let ok = unsafe {
            libc::ioctl(fd, KDGETMODE as _, &mut found);
            libc::ioctl(fd, KDSETMODE as _, KD_GRAPHICS) == 0
        };
        // SAFETY: a plain call.
        let parent = unsafe { libc::getppid() };
        Console { fd: ok.then_some(fd), process: false, signals: (0, 0), found, parent }
    }
    pub fn is_console(&self) -> bool {
        self.fd.is_some()
    }

    /// VT switches through the program: `release` and `acquire` are signal numbers whose
    /// handlers the program has installed (SIGUSR1 and SIGUSR2, say).
    pub fn switch_by_signal(&mut self, release: i32, acquire: i32) -> bool {
        let Some(fd) = self.fd else { return false };
        let m = VtMode { mode: VT_PROCESS, waitv: 0, relsig: release as _, acqsig: acquire as _, frsig: 0 };
        // SAFETY: a vt_mode by pointer.
        self.process = unsafe { libc::ioctl(fd, VT_SETMODE as _, &m) } == 0;
        self.signals = (release, acquire);
        self.process
    }

    /// Takes the console back after another program has had it (a panel run by the frame):
    /// graphics again, VT switches by signal again if they were, and whatever was typed at the
    /// VT meanwhile dropped. The other program's settings die with it, so the frame says its own
    /// again.
    pub fn reclaim(&mut self) {
        let Some(fd) = self.fd else { return };
        // SAFETY: an int argument by value.
        unsafe { libc::ioctl(fd, KDSETMODE as _, KD_GRAPHICS) };
        if self.process {
            let (r, a) = self.signals;
            self.switch_by_signal(r, a);
        }
        // SAFETY: a plain call on the console's fd.
        unsafe { libc::tcflush(fd, libc::TCIFLUSH) };
    }

    /// Whether this VT is the one on screen. True on anything but a console, and when it can't
    /// be told.
    pub fn is_foreground(&self) -> bool {
        let Some(fd) = self.fd else { return true };
        let Some(own) = vt_number(fd) else { return true };
        let mut st = VtStat::default();
        // SAFETY: a vt_stat by pointer.
        if unsafe { libc::ioctl(fd, VT_GETSTATE as _, &mut st) } != 0 {
            return true;
        }
        st.v_active as u32 == own
    }
    /// Lets the switch away go ahead: call once the card and the keys are let go.
    pub fn released(&self) {
        if let Some(fd) = self.fd {
            // SAFETY: an int argument by value.
            unsafe { libc::ioctl(fd, VT_RELDISP as _, 1 as libc::c_int) };
        }
    }
    /// Says the switch back has been taken.
    pub fn acquired(&self) {
        if let Some(fd) = self.fd {
            // SAFETY: as above.
            unsafe { libc::ioctl(fd, VT_RELDISP as _, VT_ACKACQ) };
        }
    }
    /// Switches to VT `n` (from 1).
    pub fn activate(&self, n: i32) {
        if let Some(fd) = self.fd {
            // SAFETY: as above.
            unsafe { libc::ioctl(fd, VT_ACTIVATE as _, n as libc::c_int) };
        }
    }
}

impl Drop for Console {
    fn drop(&mut self) {
        if let Some(fd) = self.fd {
            if self.process {
                let m = VtMode { mode: VT_AUTO, waitv: 0, relsig: 0, acqsig: 0, frsig: 0 };
                // SAFETY: as above.
                unsafe { libc::ioctl(fd, VT_SETMODE as _, &m) };
            }
            // The mode found, but text if the frame that wanted graphics has gone.
            // SAFETY: a plain call.
            let orphaned = unsafe { libc::getppid() } != self.parent;
            let mode = if self.found == KD_GRAPHICS && !orphaned { KD_GRAPHICS } else { KD_TEXT };
            // SAFETY: as above.
            unsafe { libc::ioctl(fd, KDSETMODE as _, mode) };
            // Keys that reached the console while the program ran (before its grab took, say) would
            // otherwise be typed at the shell it returns to.
            // SAFETY: a plain call on the console's fd.
            unsafe { libc::tcflush(fd, libc::TCIFLUSH) };
        }
    }
}

/// The VT a console's fd is, from its device number: /dev/ttyN is major 4, minor N (1 to 63).
fn vt_number(fd: RawFd) -> Option<u32> {
    // SAFETY: fstat into a zeroed struct.
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(fd, &mut st) } != 0 {
        return None;
    }
    let (major, minor) = (libc::major(st.st_rdev), libc::minor(st.st_rdev));
    (major == 4 && (1..=63).contains(&minor)).then_some(minor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structs_are_the_kernels_size() {
        assert_eq!(std::mem::size_of::<CardRes>(), 64);
        assert_eq!(std::mem::size_of::<ModeInfo>(), 68);
        assert_eq!(std::mem::size_of::<Crtc>(), 104);
        assert_eq!(std::mem::size_of::<Connector>(), 80);
        assert_eq!(std::mem::size_of::<FbCmd>(), 28);
        assert_eq!(std::mem::size_of::<PageFlip>(), 24);
        assert_eq!(std::mem::size_of::<CreateDumb>(), 32);
        assert_eq!(std::mem::size_of::<MapDumb>(), 16);
        assert_eq!(std::mem::size_of::<VtStat>(), 6);
        assert_eq!(std::mem::size_of::<VtMode>(), 8);
    }

    #[test]
    fn ioctl_numbers() {
        // From the kernel's headers, as `printf("%lx", DRM_IOCTL_MODE_GETRESOURCES)` gives them.
        assert_eq!(GETRESOURCES, 0xC04064A0);
        assert_eq!(GETCONNECTOR, 0xC05064A7);
        assert_eq!(SETCRTC, 0xC06864A2);
        assert_eq!(PAGE_FLIP, 0xC01864B0);
        assert_eq!(CREATE_DUMB, 0xC02064B2);
        // From linux/kd.h and linux/vt.h.
        assert_eq!(KDGETMODE, 0x4B3B);
        assert_eq!(VT_GETSTATE, 0x5603);
    }

    #[test]
    fn not_a_console() {
        // Under cargo test, stdin is no VT: nothing happens, and it says it's in front.
        let mut c = Console { fd: None, process: false, signals: (0, 0), found: KD_TEXT, parent: 0 };
        c.reclaim();
        assert!(c.is_foreground());
    }
}
