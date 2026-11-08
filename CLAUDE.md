# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

The pieces a Dexter-family tool needs to put its front panel on a whole screen, with nothing
between it and the hardware: no X, no Wayland, no browser, no terminal. Three crates, each
needing only `libc` (plus `miniz_oxide` for PNG):

- `lp-kms`: the screen through DRM/KMS, two dumb buffers flipped on the vertical blank, and the
  VT handed over and back by signal (`Console`).
- `lp-keys`: keyboards and pointers (mice, touchpads, touchscreens) through evdev, grabbed
  only once no key is down, with `Hotplug` watching `/dev/input` through inotify.
- `lp-paint`: drawing into an in-memory `Image`. It covers shapes, arcs and glows; Terminus
  bitmap text read from `/usr/share/consolefonts`; a `View` that maps a logical canvas (for
  example Dexter's 960 × 540) to the screen and maps clicks back; the pointer's arrow; PNG
  output for snapshots; and the family's meters (`ladder`, `disc_led`).

It follows `../suite-audio-libs`'s rules and works the same way: tools pin it by git commit and
patch it to this folder in a `.cargo/config.toml` kept out of git. Only shared code belongs here.
A change to an interface or a behaviour gets a line in `CHANGES.md`, newest first, in the same
commit. The family's rules are `../CLAUDE.md`.

## Commands

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Running anything on a real screen needs membership of the `video` and `input` groups, a real
console (Ctrl+Alt+F*n*, not ssh), and no display server holding the card. Changes are proved on
a real screen through a tool, today `../dexter-kms`. The README's "Where it's got to" section
records what has been confirmed that way.

## Design rules

- **ioctls:** they are declared by hand. Check their numbers and struct sizes against the
  kernel's headers whenever one is added.
- **Drawing:** draw into an `Image`, then copy it to the screen once a frame
  (`Surface::copy_from`). Reading a KMS buffer is slow, and blending has to read.
- **Grabbing keys:** never grab a keyboard while a key is down on it. The console would never
  see the key's release and would stay stuck, with Ctrl or Alt held.

## How it grows (Warren, 2026-11-07)

- **A tool's panel lives in the tool,** as a crate or binary of its own there: what it shows,
  its keys, its protocol. dexter-kms is a separate repository only because Dexter itself must
  not depend on its panel.
- **The suite grows from the second panel.** Code moves here when a second tool's panel would
  otherwise copy it from dexter-kms, not before. The likely candidates:
  - the screen loop: poll, wake pipe, VT release and acquire, hotplug
  - the `--snapshot --click --drag --keys` harness that tests a panel without a screen
  - the `Pen` text helper and the dark skin's palette (shared with `as-page-kit`'s skins if it
    can be, so the browser page and the console panel share one source of colours)
  - widgets such as the fader law and the dial
- **The deck's WebSocket client goes to the audio suite,** as `as-ws`'s client half, not here
  (`../suite-audio-libs/PLAN.md`).
