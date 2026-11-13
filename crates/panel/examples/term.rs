//! A terminal standing alone on a screen: the shell in the panel, no server. transom's step 2
//! (`../Transom.md`): the in-panel shape of a `--tty` in a screen, and the emulator's proof on
//! a real screen. When the panel ends, so does the shell; transom's server is what keeps it.
//!
//! ```sh
//! cargo run -p lp-panel --example term                  # $SHELL, else /bin/sh
//! cargo run -p lp-panel --example term -- --font 24     # Terminus 24 rather than the largest that fits 80 × 24
//! cargo run -p lp-panel --example term -- --run htop    # a program through /bin/sh -c
//! cargo run -p lp-panel --example term -- --snapshot out.png --size 800x480 --run 'ls -l /'
//! ```
//!
//! From a console (Ctrl+Alt+F*n*), in the `video` and `input` groups. Every key goes to the
//! shell but three: Ctrl+Alt+Esc leaves, Alt+Tab does nothing (under muntin it's the frame's),
//! Ctrl+Alt+F*n* switches the console. Shift+PgUp and Shift+PgDn scroll back. `TERM=linux`.

use lp_keys::{Key, State, code};
use lp_paint::Fonts;
use lp_paint::grid::{Colours, GridFonts, RowLook, draw_row};
use lp_panel::snapshot::{Headless, Script, Settle};
use lp_panel::{Ctl, Options, Panel, Pointer, Wake, Waker};
use lp_term::{Pty, Term, key_bytes};
use std::ffi::{OsStr, OsString};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// How many rows the scrollback keeps.
const SCROLLBACK: usize = 5000;

/// What the shell is run as, and where.
struct Program {
    path: PathBuf,
    args: Vec<OsString>,
    env: Vec<(OsString, OsString)>,
    dir: PathBuf,
}

impl Program {
    /// `$SHELL` (else `/bin/sh`), or `/bin/sh -c run`, in `$HOME`, with this environment but
    /// muntin's, `TERM=linux`, and no `COLORTERM`: only what's built is claimed.
    fn new(run: Option<&str>) -> Program {
        let var = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty());
        let (path, args) = match run {
            Some(cmd) => (PathBuf::from("/bin/sh"), vec!["-c".into(), cmd.into()]),
            None => (var("SHELL").map(PathBuf::from).unwrap_or_else(|| "/bin/sh".into()), vec![]),
        };
        let mut env: Vec<(OsString, OsString)> = std::env::vars_os()
            .filter(|(k, _)| {
                let k = k.to_string_lossy();
                !k.starts_with("MUNTIN_") && k != "TERM" && k != "COLORTERM"
            })
            .collect();
        env.push(("TERM".into(), "linux".into()));
        let dir = var("HOME").map(PathBuf::from).unwrap_or_else(|| "/".into());
        Program { path, args, env, dir }
    }

    fn spawn(&self, cols: usize, rows: usize) -> std::io::Result<Pty> {
        let args: Vec<&OsStr> = self.args.iter().map(|a| a.as_os_str()).collect();
        let env: Vec<(&OsStr, &OsStr)> =
            self.env.iter().map(|(k, v)| (k.as_os_str(), v.as_os_str())).collect();
        Pty::spawn(&self.path, &args, &env, &self.dir, cols, rows)
    }
}

/// From the reader's thread: what the program wrote, or its end.
enum Got {
    Bytes(Vec<u8>),
    End,
}

/// Reads the program's output on a thread of its own, waking the loop for each piece.
fn reader(pty: &Pty, waker: Option<Waker>) -> mpsc::Receiver<Got> {
    // SAFETY: a new fd onto the same terminal, the thread's own.
    let fd = unsafe { OwnedFd::from_raw_fd(libc::dup(pty.fd())) };
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let mut pfd = libc::pollfd { fd: fd.as_raw_fd(), events: libc::POLLIN, revents: 0 };
            // SAFETY: one pollfd; a buffer of its length.
            unsafe { libc::poll(&mut pfd, 1, -1) };
            let n = unsafe { libc::read(fd.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
            let got = match n {
                n if n > 0 => Got::Bytes(buf[..n as usize].to_vec()),
                n if n < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::WouldBlock => {
                    continue;
                }
                _ => Got::End,
            };
            let end = matches!(got, Got::End);
            if tx.send(got).is_err() {
                return;
            }
            if let Some(w) = &waker {
                w.wake();
            }
            if end {
                return;
            }
        }
    });
    rx
}

struct TermPanel {
    program: Program,
    term: Term,
    pty: Option<Pty>,
    rx: Option<mpsc::Receiver<Got>>,
    waker: Option<Waker>,
    /// Typed, waiting for room in the terminal.
    out: Vec<u8>,
    fonts: Fonts,
    font: Option<usize>,
    /// The cell size and the screen's, once known.
    cell: (usize, usize),
    screen: (usize, usize),
    colours: Colours,
    /// Rows back into the scrollback; 0 is the live screen.
    scroll: usize,
    /// The whole screen to be drawn, not only the rows changed.
    full: bool,
    /// The cursor where it was last drawn, to draw over it when it moves.
    drawn_cursor: Option<usize>,
    ended: Option<i32>,
}

impl TermPanel {
    /// The font for a screen of `w` × `h`, as asked for or the largest that gives 80 × 24.
    fn grid_fonts(&self, w: usize, h: usize) -> GridFonts<'_> {
        let (regular, bold) = self
            .font
            .and_then(|px| self.fonts.sized(px))
            .unwrap_or_else(|| self.fonts.for_grid(w, h, 80, 24));
        GridFonts { regular, bold }
    }

    /// Sizes the grid to the screen and starts the program on it, the first time.
    fn start(&mut self, w: usize, h: usize) -> Result<(), String> {
        let fonts = self.grid_fonts(w, h);
        let (cols, rows) = fonts.fit(w, h);
        self.cell = fonts.cell();
        self.screen = (w, h);
        self.term.resize(cols, rows);
        let pty =
            self.program.spawn(cols, rows).map_err(|e| format!("{}: {e}", self.program.path.display()))?;
        self.rx = Some(reader(&pty, self.waker.clone()));
        self.pty = Some(pty);
        self.full = true;
        Ok(())
    }

    /// As much of what's typed as the terminal takes now.
    fn flush(&mut self) {
        let Some(pty) = &self.pty else { return };
        while !self.out.is_empty() {
            match pty.write(&self.out) {
                Ok(n) => {
                    self.out.drain(..n);
                }
                Err(_) => break,
            }
        }
    }

    fn scroll_by(&mut self, back: isize) -> bool {
        let to = (self.scroll as isize + back).clamp(0, self.term.scrollback_len() as isize) as usize;
        let moved = to != self.scroll;
        self.scroll = to;
        self.full |= moved;
        moved
    }
}

impl Panel for TermPanel {
    fn take(&mut self) -> bool {
        let mut changed = false;
        let mut ended = false;
        if let Some(rx) = &self.rx {
            while let Ok(got) = rx.try_recv() {
                match got {
                    Got::Bytes(b) => {
                        self.term.feed(&b);
                        changed = true;
                    }
                    Got::End => ended = true,
                }
            }
        }
        let replies = self.term.take_replies();
        self.out.extend(replies);
        self.term.take_bells();
        self.flush();
        if ended && let Some(pty) = &mut self.pty {
            // The terminal closed: the program has gone, or is going.
            let start = Instant::now();
            while self.ended.is_none() && start.elapsed() < Duration::from_secs(1) {
                self.ended = pty.try_wait().ok().flatten();
                std::thread::sleep(Duration::from_millis(5));
            }
            eprintln!("term: {} ended, status {}", self.program.path.display(), self.ended.unwrap_or(-1));
            lp_panel::quit();
        }
        changed
    }

    fn key(&mut self, k: &Key, ctl: &mut Ctl) -> bool {
        if let Some(n) = lp_panel::vt_key(k) {
            ctl.switch_vt(n);
            return false;
        }
        if lp_keys::is_frame_key(k) {
            // Standing alone there's no other way out (under muntin, the loop took it first).
            lp_panel::quit();
            return false;
        }
        if k.code == code::TAB && k.mods.alt && !k.mods.ctrl && !k.mods.meta {
            return false; // Alt+Tab is the frame's, never the shell's
        }
        if k.mods.shift && !k.mods.ctrl && !k.mods.alt && k.state != State::Release {
            let page = (self.term.rows() / 2).max(1) as isize;
            match k.code {
                code::PAGEUP => return self.scroll_by(page),
                code::PAGEDOWN => return self.scroll_by(-page),
                _ => {}
            }
        }
        let Some(bytes) = key_bytes(k, &self.term.modes()) else { return false };
        self.out.extend(bytes);
        self.flush();
        // Any other key goes back to the bottom.
        self.scroll_by(-(self.scroll as isize))
    }

    fn point(&mut self, _: Pointer, _: usize, _: usize) -> bool {
        false
    }

    fn tick(&mut self) -> bool {
        self.flush();
        false
    }

    fn draw(&mut self, sf: &mut lp_paint::Surface) {
        if self.pty.is_none()
            && self.ended.is_none()
            && let Err(e) = self.start(sf.w, sf.h)
        {
            eprintln!("term: {e}");
            self.ended = Some(127);
            lp_panel::quit();
        }
        // The scrollback may have been cleared (`CSI 3 J`) while scrolled back.
        self.scroll = self.scroll.min(self.term.scrollback_len());
        let colours = self.colours.with_palette(self.term.palette());
        let modes = self.term.modes();
        let cursor = self.term.cursor();
        let (cols, rows) = (self.term.cols(), self.term.rows());
        let mut todo = self.term.take_dirty();
        let full = std::mem::take(&mut self.full) || self.scroll > 0;
        if full {
            sf.clear(if modes.reverse { colours.fg } else { colours.bg });
            todo = (0..rows).collect();
        }
        // Where the cursor was, to take it off, and where it is.
        let shown = (self.scroll == 0 && modes.cursor_visible).then_some(cursor.row);
        todo.extend(self.drawn_cursor);
        todo.extend(shown);
        todo.sort_unstable();
        todo.dedup();
        self.drawn_cursor = shown;

        let (w, h) = self.screen;
        let (cw, ch) = self.cell;
        let x0 = (w.saturating_sub(cols * cw) / 2) as i32;
        let y0 = (h.saturating_sub(rows * ch) / 2) as i32;
        let fonts = self.grid_fonts(w, h);
        let back = self.term.scrollback_len();
        for r in todo {
            // Scrolled back, the screen shows the scrollback's tail above the screen's head.
            let i = back - self.scroll + r;
            let row = if i < back { self.term.scrollback(i) } else { self.term.row(i - back) };
            let look =
                RowLook { cols, cursor: (shown == Some(r)).then_some(cursor.col), reverse: modes.reverse };
            draw_row(sf, x0, y0 + (r * ch) as i32, fonts, &colours, row.cells(), look);
        }
    }
}

impl Headless for TermPanel {
    /// Waits until the program has been quiet a moment (or has ended), taking what it wrote.
    fn settle(&mut self, s: Settle) -> Result<(), String> {
        let quiet = Duration::from_millis(if s == Settle::Key { 100 } else { 300 });
        let start = Instant::now();
        let mut last = Instant::now();
        while last.elapsed() < quiet && start.elapsed() < Duration::from_secs(5) {
            let got = self.rx.as_ref().map(|rx| rx.recv_timeout(Duration::from_millis(20)));
            match got {
                Some(Ok(Got::Bytes(b))) => {
                    self.term.feed(&b);
                    last = Instant::now();
                }
                Some(Ok(Got::End)) | None => break,
                Some(Err(_)) => {}
            }
            let replies = self.term.take_replies();
            self.out.extend(replies);
            self.flush();
        }
        Ok(())
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "term [--font PX] [--run COMMAND] [--snapshot PNG --size WxH --keys KEYS]: a terminal on a whole screen"
        );
        return;
    }
    let fonts = match Fonts::terminus(None) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("term: {e}");
            std::process::exit(1);
        }
    };
    let font = opt("--font").map(|f| f.parse().unwrap_or_else(|_| panic!("--font: not a size: {f}")));
    let snapshot = opt("--snapshot");
    let wake = Wake::new().expect("a wake pipe");
    let mut panel = TermPanel {
        program: Program::new(opt("--run").as_deref()),
        term: Term::new(80, 24, SCROLLBACK),
        pty: None,
        rx: None,
        waker: snapshot.is_none().then(|| wake.waker()),
        out: Vec::new(),
        fonts,
        font,
        cell: (1, 1),
        screen: (1, 1),
        colours: Colours::dark(),
        scroll: 0,
        full: true,
        drawn_cursor: None,
        ended: None,
    };
    if let Some(png) = snapshot {
        let size = opt("--size").unwrap_or_else(|| "1920x1080".into());
        let script = Script::parse(&size, &[], &[], opt("--keys").as_deref()).unwrap_or_else(|e| {
            eprintln!("term: {e}");
            std::process::exit(2);
        });
        if let Err(e) = panel.start(script.size.0, script.size.1) {
            eprintln!("term: {e}");
            std::process::exit(1);
        }
        match lp_panel::snapshot::snapshot(&mut panel, &script, Path::new(&png)) {
            Ok(said) => println!("{said}"),
            Err(e) => {
                eprintln!("term: {e}");
                std::process::exit(1);
            }
        }
        return;
    }
    let o = Options { name: "term", pointer: false, quit_keys: "Ctrl+Alt+Esc", ..Options::default() };
    if let Err(e) = lp_panel::run(&o, &wake, &mut panel) {
        eprintln!("term: {e}");
        std::process::exit(1);
    }
    std::process::exit(panel.ended.unwrap_or(0));
}
