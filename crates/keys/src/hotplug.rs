//! Devices plugged in while the program runs: `/dev/input` watched with inotify, no udev.
//!
//! The kernel makes a new `eventN` as root's alone; udev then gives it to the `input` group,
//! a moment later. So a device is offered when it appears and again each time its permissions
//! change ([`Hotplug::read`]), and [`crate::Keyboards::add`] or [`crate::Pointers::add`] takes
//! it once it can be opened. A device unplugged needs no watching: reading it fails, and it's
//! dropped then.

use std::ffi::{CString, OsStr};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

pub const INPUT_DIR: &str = "/dev/input";

pub struct Hotplug {
    fd: OwnedFd,
    dir: PathBuf,
}

impl Hotplug {
    /// Watches `/dev/input`.
    pub fn new() -> io::Result<Hotplug> {
        Hotplug::watch(Path::new(INPUT_DIR))
    }

    /// Watches `dir` for `event*` appearing or changing.
    pub fn watch(dir: &Path) -> io::Result<Hotplug> {
        // SAFETY: plain calls; the fd is owned from here.
        let raw = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        if raw < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: inotify_init1 gave a new fd, ours alone.
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };
        let c = CString::new(dir.as_os_str().as_bytes()).map_err(io::Error::other)?;
        // SAFETY: a NUL-terminated path.
        let wd = unsafe { libc::inotify_add_watch(raw, c.as_ptr(), libc::IN_CREATE | libc::IN_ATTRIB) };
        if wd < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Hotplug { fd, dir: dir.to_path_buf() })
    }

    /// For poll: readable when something under the directory appeared or changed.
    pub fn fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }

    /// The `event*` devices that appeared or changed since the last call, each once, without
    /// blocking: to offer to `add`.
    pub fn read(&mut self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            // SAFETY: reading into a buffer of its length, non-blocking.
            let n = unsafe { libc::read(self.fd(), buf.as_mut_ptr().cast(), buf.len()) };
            if n <= 0 {
                break;
            }
            // struct inotify_event: wd (i32), mask, cookie, len (u32), then `len` bytes of name.
            let mut at = 0usize;
            while at + 16 <= n as usize {
                let len = u32::from_ne_bytes(buf[at + 12..at + 16].try_into().unwrap()) as usize;
                let name = &buf[at + 16..(at + 16 + len).min(n as usize)];
                let name = &name[..name.iter().position(|&b| b == 0).unwrap_or(name.len())];
                if name.starts_with(b"event") {
                    let p = self.dir.join(OsStr::from_bytes(name));
                    if !out.contains(&p) {
                        out.push(p);
                    }
                }
                at += 16 + len;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sees_a_device_appear_and_change() {
        let dir = std::env::temp_dir().join(format!("lp-keys-hotplug-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut h = Hotplug::watch(&dir).unwrap();
        assert!(h.read().is_empty());
        std::fs::write(dir.join("event7"), b"").unwrap();
        std::fs::write(dir.join("mouse0"), b"").unwrap();
        assert_eq!(h.read(), vec![dir.join("event7")]);
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.join("event7"), std::fs::Permissions::from_mode(0o660)).unwrap();
        assert_eq!(h.read(), vec![dir.join("event7")]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
