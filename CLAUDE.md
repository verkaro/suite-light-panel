# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

The pieces a Dexter-family tool needs to put its front panel on a whole screen, with nothing
between it and the hardware: no X, no Wayland, no browser, no terminal. Four crates, each
needing only `libc` and each other (plus `miniz_oxide` for PNG):

- `lp-kms`: the screen through DRM/KMS, two dumb buffers flipped on the vertical blank, and the
  VT handed over and back by signal (`Console`).
- `lp-keys`: keyboards and pointers (mice, touchpads, touchscreens) through evdev, grabbed
  only once no key is down, with `Hotplug` watching `/dev/input` through inotify. `common`
  holds the family's common keys by value, and holds a tool's own list to them.
- `lp-paint`: drawing into an in-memory `Image`. It covers shapes, arcs and glows; Terminus
  bitmap text read from `/usr/share/consolefonts`; a `View` that maps a logical canvas (for
  example Dexter's 960 × 540) to the screen and maps clicks back; the `Pen` that draws through
  one; the dark skin (`dark`, page-kit's colours); the pointer's arrow; PNG output for
  snapshots; the family's meters (`ladder`, `disc_led`); and the parts a panel is made of: a
  key, a form's field, a list, a waveform, and `Hits`, where a click can land.
- `lp-panel`: the loop a panel runs on the other three (`run` and the `Panel` trait), the
  pipe that wakes it, and `snapshot`, the same panel run with no screen for its tests. Also
  what takes keys: the command bar (`bar`), a line being typed (`line`), and `?` (`keys`).

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

## Rules shared with the audio suite (Warren, 2026-11-11)

- **One of each thing:** within the suite there is one copy of anything. A second found is fixed
  in the session that finds it, with a `CHANGES.md` line, never invited to move "when it wants".
- **A session here ends with `../family.sh check suite-light-panel`:** the panels patched to this
  folder are run before it's pushed.
- **A panel under muntin** follows the checklist in `../STYLE.md`, "A panel under muntin".

## How it grows (Warren, 2026-11-07)

- **A tool's panel lives in the tool,** as a crate or binary of its own there: what it shows,
  its keys, its protocol. dexter-kms is a separate repository only because Dexter itself must
  not depend on its panel.
- **The suite grows from the second panel.** Code moves here when a second tool's panel would
  otherwise copy it from dexter-kms, not before. Tape's panel is the second, so these came
  from dexter-kms on 2026-11-07, before it starts (`CHANGES.md`):
  - the screen loop: poll, wake pipe, VT release and acquire, hotplug (`lp-panel`'s `run`)
  - the `--snapshot --click --drag --keys` harness that tests a panel without a screen
    (`lp-panel`'s `snapshot`)
  - the `Pen` text helper and the dark skin's palette (`lp_paint::Pen`, `lp_paint::dark`). The
    colours are page-kit's dark skin by value, and a test compares them with `page-kit.js`
    when `../suite-audio-libs` is beside this suite, so the two can't drift apart unseen.

  Still to come, when a second panel wants it: the fader law. The dial came with
  focusrite-ctrl (2026-11-10), the second panel to want one, written here rather than moved
  from dexter-kms, whose own dial can move onto it at its own pace.
- **The keys, the bar and the library's parts came with dexter-irma's panel** (2026-11-10,
  `CHANGES.md`): the common keys held by value and compared with `page-kit.js`, the command
  bar, `?`, and a key, a field, a list and a waveform. They draw in `dark` by its names, as
  everything here does until there's a `Skin` value (`../STYLE.md`, "Not yet the same", 1).
  `cargo run -p lp-panel --example parts -- out.png` draws them all. None has been on a real
  screen yet: that's Warren's, through irma's panel.
- **Where a part goes:** one that only draws is in `lp-paint`. One that takes keys is in
  `lp-panel`, since `lp-paint` doesn't know a key.
- **A new panel** is a `lp_panel::Panel` run by `lp_panel::run` (with `frame_key` on, for
  muntin), and `snapshot::Headless` for its tests; its server through `as-ws`'s client. Its
  keys are a `keys.json` held to `lp_keys::common`, with `?` and the bar drawn from it. dexter-kms
  is the example (its `main.rs`, `Screen`).
- **For muntin, the frame** (`../muntin/docs/rfc.md`, section 7): the shared key and the
  console's handover went in on 2026-11-07 (`CHANGES.md`).
- **The deck's WebSocket client went to the audio suite,** as `as-ws`'s client half, not here
  (2026-11-07, `../suite-audio-libs/PLAN.md`).
