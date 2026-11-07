# suite-light-panel

The pieces a tool in the Dexter family needs to put its front panel on a whole screen with
nothing between it and the hardware: no X, no Wayland, no browser, no terminal. The kernel's mode
setting for the screen, its input devices for the keys, and drawing done here. MIT.

It sits beside `suite-audio-libs` and follows its rules: a tool depends on a crate by this
repository's git address pinned to a commit, patched to the clone beside it while working; a fix
goes in here; nothing is here that's only one tool's. Each tool's own panel (what it shows, its
keys, its protocol) lives with the tool.

```toml
[workspace.dependencies]
lp-kms = { git = "https://github.com/verkaro/suite-light-panel", rev = "COMMIT" }
```

## The crates

- `lp-kms`: the screen through DRM/KMS. The first card with a screen connected, the first
  connected output at its preferred mode, two dumb buffers (XRGB8888) flipped on the vertical
  blank, and what was showing put back on exit. `Console` puts the VT in graphics mode, and
  takes VT switches by signal (`VT_PROCESS`) so the card is given up and taken back politely.
  `outputs()` lists a card's outputs without being its master. Only `libc`: the ioctls are
  declared here and checked against the kernel's headers.
- `lp-keys`: every keyboard under `/dev/input`, read with evdev, optionally grabbed so the keys
  reach the panel alone. Press, repeat and release, the modifiers, and the character on a US
  layout. A keyboard is grabbed only once no key is down on it: grabbed between a press and its
  release, the console never sees the release and is left thinking Ctrl or Alt is held.
  `Pointers`: the mice, touchpads (a tap clicks), touchscreens and absolute pointers (a tablet,
  a virtual machine's), as one position on the screen, with clicks and the wheel. `Hotplug`:
  `/dev/input` watched with inotify (no udev), so a keyboard or pointer plugged in later is
  taken (`Keyboards::add`, `Pointers::add`) once udev has given it to the `input` group; one
  unplugged is dropped when reading it fails. Only `libc`.
- `lp-paint`: drawing into pixels: rectangles, rounded ones, discs and rings with smoothed
  edges, glows, text in the console's own Terminus bitmap fonts (PSF, read at run time from
  `/usr/share/consolefonts`), a `View` that fits a logical canvas (Dexter's 960 × 540) to the
  screen and takes a click back to it, the pointer's arrow (nothing else draws one), a PNG writer for snapshots, and the family's two meters:
  - `ladder`: the LED ladder, bottom up, green, amber, red (Dexter's deck);
  - `disc_led`: one LED a track, white brighter as it's louder, then yellow, then red, held with
    the peak, ringed when armed, recording or clipped (Dexter's remote). For a small screen, or
    many tracks: 64 fit on 800 × 480.

  Draw into an `Image` in memory and copy it to the screen once a frame
  (`Surface::copy_from`): a KMS buffer is slow to read, and blending reads.

## Needs

- In the `video` group, for `/dev/dri/card*`, and no display server holding the card.
- In the `input` group, for `/dev/input/event*` (a new login after joining it).
- Debian's `console-setup-linux` for the Terminus fonts (installed with most systems).

## Commands

```
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Where it's got to (2026-11-07)

Run on a real screen: `dexter-kms` (github verkaro/dexter-kms), full screen through KMS on a
local machine, with keyboard input through evdev and live meters from Dexter over the LAN, and
switching VTs. The ioctl numbers and struct sizes match the kernel's headers.

The first run left the console's keyboard dead after quitting (its Ctrl and Alt stuck, from a
grab taken while they were down); the grab now waits for the keys to be let go, and the console
works after quitting. The mouse works on a real screen too. Hotplug is tested against a
directory only, not yet with a device plugged in. Not yet: a second tool built on these crates.
