//! [`run`](crate::run) in a Wayland window: the console's loop, with `lp-wayland`'s window for the
//! screen and its keys and pointer for evdev's. What a panel is called with, and when, is the same.

use crate::{Ctl, Options, Panel, TICK, Wake, cursor_px, quit, quitting};
use lp_paint::Image;
use lp_wayland::{Event, Window};
use std::time::{Duration, Instant};

pub(crate) fn run(o: &Options, wake: &Wake, panel: &mut impl Panel) -> Result<(), String> {
    let name = o.name;
    let mut win = Window::open(name, o.pointer)?;
    eprintln!("{name}: in a Wayland window");
    let tick = Duration::from_millis(TICK as u64);
    let mut canvas = Image::new(0, 0);
    let mut dirty = true;
    let mut shown_cursor = None;
    let mut last_tick = Instant::now();

    while !quitting() {
        let mut fds = [libc::pollfd { fd: wake.fd(), events: libc::POLLIN, revents: 0 }];
        let events = win.poll(&mut fds, tick.saturating_sub(last_tick.elapsed()))?;
        wake.drain();
        dirty |= panel.take();
        let (w, h) = win.size().unwrap_or((0, 0));
        for e in events {
            match e {
                Event::Key(k) => {
                    if k.down() {
                        win.hide_cursor();
                    }
                    // Ctrl+Alt+F*n* is the compositor's, so a panel's switch of VT is let be.
                    dirty |= panel.key(&k, &mut Ctl::default());
                }
                Event::Pointer(p) => dirty |= panel.point(p, w, h),
                Event::Plugged(said) => {
                    panel.plugged(said);
                    dirty = true;
                }
                Event::Resized => dirty = true,
                Event::Closed => quit(),
            }
        }
        if last_tick.elapsed() >= tick {
            last_tick = Instant::now();
        }
        dirty |= panel.tick();

        if (w, h) == (0, 0) {
            continue;
        }
        if (canvas.w, canvas.h) != (w, h) {
            canvas = Image::new(w, h);
            dirty = true;
        }
        let cursor = win.cursor();
        if (dirty || cursor != shown_cursor) && win.ready() {
            // As on the console: drawn in memory, then copied with the pointer on top, so moving
            // it doesn't draw the panel again.
            if dirty {
                panel.draw(&mut canvas.surface());
                dirty = false;
            }
            win.present(&canvas.surface(), cursor_px(h))?;
            shown_cursor = cursor;
        }
    }
    Ok(())
}
