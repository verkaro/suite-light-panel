//! A program on a pseudo-terminal: the master end ours, the program in a session of its own
//! with the other end as its terminal, its size set. Only `libc`.
//!
//! The program gets exactly the environment it's given, nothing inherited, so a caller decides
//! what reaches a shell (transom takes `MUNTIN_*` out; tests give `/bin/sh` a scratch `HOME`).

use std::ffi::{CString, OsStr};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

/// A program running on a pseudo-terminal.
#[derive(Debug)]
pub struct Pty {
    master: OwnedFd,
    pid: libc::pid_t,
    status: Option<i32>,
}

fn cstr(s: &OsStr) -> io::Result<CString> {
    CString::new(s.as_bytes()).map_err(|_| io::Error::other("a NUL in an argument"))
}

fn check(r: libc::c_int) -> io::Result<libc::c_int> {
    if r < 0 { Err(io::Error::last_os_error()) } else { Ok(r) }
}

fn winsize(cols: usize, rows: usize) -> libc::winsize {
    let clamp = |n: usize| n.clamp(1, usize::from(u16::MAX)) as u16;
    libc::winsize { ws_row: clamp(rows), ws_col: clamp(cols), ws_xpixel: 0, ws_ypixel: 0 }
}

impl Pty {
    /// Starts `program` with `args` (not counting its own name) and `env` alone, in `dir`, on a
    /// terminal of `cols` by `rows`. An error if it can't be started, its own errno.
    pub fn spawn(
        program: &Path,
        args: &[&OsStr],
        env: &[(&OsStr, &OsStr)],
        dir: &Path,
        cols: usize,
        rows: usize,
    ) -> io::Result<Pty> {
        // Everything the child needs, made before the fork: after it, nothing may allocate.
        let prog = cstr(program.as_os_str())?;
        let mut argv_s = vec![prog.clone()];
        for a in args {
            argv_s.push(cstr(a)?);
        }
        let mut argv: Vec<*const libc::c_char> = argv_s.iter().map(|a| a.as_ptr()).collect();
        argv.push(std::ptr::null());
        let mut envp_s = Vec::with_capacity(env.len());
        for (k, v) in env {
            let mut kv = k.as_bytes().to_vec();
            kv.push(b'=');
            kv.extend_from_slice(v.as_bytes());
            envp_s.push(CString::new(kv).map_err(|_| io::Error::other("a NUL in the environment"))?);
        }
        let mut envp: Vec<*const libc::c_char> = envp_s.iter().map(|e| e.as_ptr()).collect();
        envp.push(std::ptr::null());
        let dir = cstr(dir.as_os_str())?;

        // SAFETY: the master is new and ours; its name is read into a buffer of its length.
        let (master, slave_name) = unsafe {
            let fd = check(libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC))?;
            let master = OwnedFd::from_raw_fd(fd);
            check(libc::grantpt(fd))?;
            check(libc::unlockpt(fd))?;
            let mut name = [0 as libc::c_char; 128];
            let r = libc::ptsname_r(fd, name.as_mut_ptr(), name.len());
            if r != 0 {
                return Err(io::Error::from_raw_os_error(r));
            }
            (master, std::ffi::CStr::from_ptr(name.as_ptr()).to_owned())
        };
        let ws = winsize(cols, rows);
        // SAFETY: a winsize of the kernel's shape, by pointer.
        check(unsafe { libc::ioctl(master.as_raw_fd(), libc::TIOCSWINSZ, &ws) })?;

        // The child says why exec failed through a pipe closed by a successful exec.
        let mut pipe = [0; 2];
        // SAFETY: two ints for the two ends.
        check(unsafe { libc::pipe2(pipe.as_mut_ptr(), libc::O_CLOEXEC) })?;
        // SAFETY: both ends new and ours.
        let (err_r, err_w) = unsafe { (OwnedFd::from_raw_fd(pipe[0]), OwnedFd::from_raw_fd(pipe[1])) };

        // SAFETY: fork; the child calls only async-signal-safe functions on what was made above,
        // then execs or `_exit`s.
        let pid = check(unsafe { libc::fork() })?;
        if pid == 0 {
            unsafe { child(&slave_name, &dir, &prog, &argv, &envp, err_w.as_raw_fd()) }
        }
        drop(err_w);
        let mut errno = [0u8; 4];
        // SAFETY: reading into a buffer of its length; EOF means exec succeeded.
        let n = loop {
            let n = unsafe { libc::read(err_r.as_raw_fd(), errno.as_mut_ptr().cast(), 4) };
            if n >= 0 || io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
                break n;
            }
        };
        if n == 4 {
            // SAFETY: reaping our own child, which has `_exit`ed.
            unsafe { libc::waitpid(pid, std::ptr::null_mut(), 0) };
            return Err(io::Error::from_raw_os_error(i32::from_ne_bytes(errno)));
        }
        // SAFETY: flags on our own fd.
        unsafe {
            let fl = libc::fcntl(master.as_raw_fd(), libc::F_GETFL);
            libc::fcntl(master.as_raw_fd(), libc::F_SETFL, fl | libc::O_NONBLOCK);
        }
        Ok(Pty { master, pid, status: None })
    }

    /// The master end, to poll for reading.
    pub fn fd(&self) -> RawFd {
        self.master.as_raw_fd()
    }

    pub fn pid(&self) -> i32 {
        self.pid
    }

    /// What the program wrote, without waiting: `WouldBlock` when there's nothing yet, and 0
    /// once the terminal's other end is closed (the kernel's EIO, said as an end).
    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        // SAFETY: reading into a buffer of its length.
        let n = unsafe { libc::read(self.fd(), buf.as_mut_ptr().cast(), buf.len()) };
        if n >= 0 {
            return Ok(n as usize);
        }
        let e = io::Error::last_os_error();
        if e.raw_os_error() == Some(libc::EIO) { Ok(0) } else { Err(e) }
    }

    /// Typed to the program, as much as fits now: what's written, or `WouldBlock`.
    pub fn write(&self, bytes: &[u8]) -> io::Result<usize> {
        // SAFETY: writing from a slice of its length.
        let n = unsafe { libc::write(self.fd(), bytes.as_ptr().cast(), bytes.len()) };
        if n < 0 { Err(io::Error::last_os_error()) } else { Ok(n as usize) }
    }

    /// A new size; the kernel tells the program (SIGWINCH).
    pub fn resize(&self, cols: usize, rows: usize) -> io::Result<()> {
        let ws = winsize(cols, rows);
        // SAFETY: a winsize of the kernel's shape, by pointer.
        check(unsafe { libc::ioctl(self.fd(), libc::TIOCSWINSZ, &ws) }).map(|_| ())
    }

    /// The program's status once it has ended: its exit code, or 128 and the signal that ended
    /// it, as a shell says it. Doesn't wait.
    pub fn try_wait(&mut self) -> io::Result<Option<i32>> {
        if self.status.is_none() {
            let mut st = 0;
            // SAFETY: our own child, not waiting.
            let r = check(unsafe { libc::waitpid(self.pid, &mut st, libc::WNOHANG) })?;
            if r == self.pid {
                self.status = Some(if libc::WIFSIGNALED(st) {
                    128 + libc::WTERMSIG(st)
                } else {
                    libc::WEXITSTATUS(st)
                });
            }
        }
        Ok(self.status)
    }

    /// SIGHUP to the program's process group, so its jobs end with it, as when a terminal's
    /// line drops.
    pub fn hangup(&self) {
        if self.status.is_none() {
            // SAFETY: a signal to the group our child leads.
            unsafe { libc::kill(-self.pid, libc::SIGHUP) };
        }
    }
}

impl Drop for Pty {
    /// Hangs up and closes; the program is reaped if it has gone, never waited for.
    fn drop(&mut self) {
        self.hangup();
        let _ = self.try_wait();
    }
}

/// In the child, after the fork: a session of its own, the terminal as its controlling one and
/// its standard three, the signals as a new program expects them, then the program. Only
/// async-signal-safe calls.
unsafe fn child(
    slave: &std::ffi::CStr,
    dir: &CString,
    prog: &CString,
    argv: &[*const libc::c_char],
    envp: &[*const libc::c_char],
    err: RawFd,
) -> ! {
    let fail = |err: RawFd| -> ! {
        // SAFETY: errno read, written to the pipe, and out.
        unsafe {
            let e = (*libc::__errno_location()).to_ne_bytes();
            libc::write(err, e.as_ptr().cast(), 4);
            libc::_exit(127)
        }
    };
    // SAFETY: each call is async-signal-safe, on what the parent made.
    unsafe {
        if libc::setsid() < 0 {
            fail(err);
        }
        let fd = libc::open(slave.as_ptr(), libc::O_RDWR);
        if fd < 0 || libc::ioctl(fd, libc::TIOCSCTTY, 0) < 0 {
            fail(err);
        }
        // UTF-8 for the line discipline, so Backspace in a cooked read takes a whole character.
        let mut t: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(fd, &mut t) == 0 {
            t.c_iflag |= libc::IUTF8;
            libc::tcsetattr(fd, libc::TCSANOW, &t);
        }
        for std in 0..3 {
            if libc::dup2(fd, std) < 0 {
                fail(err);
            }
        }
        if fd > 2 {
            libc::close(fd);
        }
        // Nothing else of the parent's reaches the program: a panel's screen and keyboards,
        // muntin's pipe. Marked, not closed, so the error pipe still says if exec fails.
        // CLOSE_RANGE_CLOEXEC is Linux 5.11's; before it, one at a time.
        if libc::close_range(3, libc::c_uint::MAX, libc::CLOSE_RANGE_CLOEXEC as libc::c_int) < 0 {
            let max = libc::sysconf(libc::_SC_OPEN_MAX).clamp(256, 65536) as RawFd;
            for fd in 3..max {
                libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
            }
        }
        // What the parent ignores or blocks (Rust ignores SIGPIPE) would be inherited.
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigprocmask(libc::SIG_SETMASK, &set, std::ptr::null_mut());
        for sig in 1..32 {
            if sig != libc::SIGKILL && sig != libc::SIGSTOP {
                libc::signal(sig, libc::SIG_DFL);
            }
        }
        if libc::chdir(dir.as_ptr()) < 0 {
            fail(err);
        }
        libc::execve(prog.as_ptr(), argv.as_ptr(), envp.as_ptr());
        fail(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// A scratch folder for `HOME`, never Warren's, gone when the test is.
    struct Scratch(std::path::PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn scratch() -> Scratch {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("lp-term-pty-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        Scratch(d)
    }

    /// `/bin/sh -c script`, with nothing of this environment but a scratch `HOME`.
    fn sh(script: &str, cols: usize, rows: usize) -> (Pty, Scratch) {
        let home = scratch();
        let env: [(&OsStr, &OsStr); 3] = [
            ("HOME".as_ref(), home.0.as_os_str()),
            ("PATH".as_ref(), "/usr/bin:/bin".as_ref()),
            ("TERM".as_ref(), "linux".as_ref()),
        ];
        let argv: [&OsStr; 2] = ["-c".as_ref(), script.as_ref()];
        (Pty::spawn(Path::new("/bin/sh"), &argv, &env, &home.0, cols, rows).unwrap(), home)
    }

    /// Everything until the program's end, or `until` turns up.
    fn read_until(p: &Pty, until: Option<&str>) -> String {
        let mut out = Vec::new();
        let start = Instant::now();
        let mut buf = [0u8; 4096];
        while start.elapsed() < Duration::from_secs(10) {
            let mut pfd = libc::pollfd { fd: p.fd(), events: libc::POLLIN, revents: 0 };
            // SAFETY: one pollfd.
            unsafe { libc::poll(&mut pfd, 1, 100) };
            match p.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => out.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) => panic!("{e}"),
            }
            if until.is_some_and(|u| String::from_utf8_lossy(&out).contains(u)) {
                break;
            }
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    fn wait(p: &mut Pty) -> i32 {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(10) {
            if let Some(s) = p.try_wait().unwrap() {
                return s;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("never ended");
    }

    #[test]
    fn runs_on_a_terminal_of_its_size() {
        let (mut p, _home) =
            sh("printf 'hi\\n'; stty size; [ -t 0 ] && echo tty; echo \"$TERM $FOO\"", 80, 24);
        let out = read_until(&p, None);
        assert_eq!(out, "hi\r\n24 80\r\ntty\r\nlinux \r\n", "nothing of ours in its environment");
        assert_eq!(wait(&mut p), 0);
    }

    #[test]
    fn resized_and_typed_to() {
        let (p, _home) = sh("read x; stty size; echo \"got $x\"", 80, 24);
        p.resize(100, 30).unwrap();
        p.write(b"abc\r").unwrap();
        let out = read_until(&p, Some("got abc"));
        assert!(out.ends_with("abc\r\n30 100\r\ngot abc\r\n"), "{out:?}");
    }

    #[test]
    fn its_status() {
        let (mut p, _home) = sh("exit 3", 80, 24);
        read_until(&p, None);
        assert_eq!(wait(&mut p), 3);
        let (mut p, _home) = sh("kill -TERM $$", 80, 24);
        read_until(&p, None);
        assert_eq!(wait(&mut p), 128 + libc::SIGTERM);
    }

    #[test]
    fn a_pipe_closed_ends_its_writer() {
        // SIGPIPE back to its default, though this process ignores it.
        let (mut p, _home) = sh("yes | head -n 1", 80, 24);
        assert_eq!(read_until(&p, None), "y\r\n");
        assert_eq!(wait(&mut p), 0);
    }

    #[test]
    fn hangup_ends_its_jobs() {
        let (mut p, _home) = sh("sleep 30 & echo started; wait", 80, 24);
        read_until(&p, Some("started"));
        p.hangup();
        assert_eq!(wait(&mut p), 128 + libc::SIGHUP);
    }

    #[test]
    fn nothing_of_ours_is_left_open_in_it() {
        // An fd of ours not marked close-on-exec, as a screen or a keyboard may be.
        let fd = std::fs::File::open("/dev/null").unwrap();
        // SAFETY: a copy of our own fd at a high number, closed below.
        assert_eq!(unsafe { libc::dup2(fd.as_raw_fd(), 900) }, 900);
        let (mut p, _home) = sh("[ -e /proc/$$/fd/900 ] && echo leaked; echo ok", 80, 24);
        let out = read_until(&p, None);
        unsafe { libc::close(900) };
        assert_eq!(out, "ok\r\n");
        assert_eq!(wait(&mut p), 0);
    }

    #[test]
    fn what_cant_start_says_why() {
        let e = Pty::spawn(Path::new("/nonexistent/program"), &[], &[], Path::new("/"), 80, 24).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::NotFound);
    }
}
