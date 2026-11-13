# Plan

What's next in this suite, and what's waiting on it. Begun 2026-11-11. What's done goes to
`CHANGES.md`; where a handoff stands is `../HANDOFFS.md`'s.

## Next: the saver (`../Saver.md`)

The picture moves a few pixels every few minutes and never blanks, for LCDs on the console.
Settled by Warren, 2026-11-11: no blanking and no DPMS off; one setting for the whole console;
its button is muntin's alone (`SAVER`, beside `FILES`); a panel only reads it.

### Where it goes

Today a frame reaches the screen in two places, the same way: `lp_panel::run` and muntin's own
`present` (`../muntin/src/main.rs`), each doing `Surface::copy_from` and then the arrow. The
saver goes in both, so:

1. **`lp-paint`: `Surface::copy_shifted(src, dx, dy, fill)`.** `copy_from` with an offset, the
   uncovered strip filled (the caller passes `dark::BG`). `copy_from` stays, as `dx = dy = 0`
   of it, so there's one copy.
2. **`lp-kms`: `Saver`**, only `libc`, as the handoff asks: the orbit and the setting, no
   pixels.
   - `orbit(n) -> (i32, i32)`: step `n` of the path. Up to 4 pixels each way, a pixel a step,
     round a square, back to 0, 0 (32 steps; about an hour and a half at a step every 3
     minutes).
   - `offset(now: Instant) -> (i32, i32)`: the clock passed in, so a test steps it without
     waiting. (0, 0) while it's off.
   - `due(now)`: when the next step falls, so a loop knows to present again without the panel
     drawing.
   - The setting: one file, read when a loop starts and again when its mtime changes (looked at
     on the loop's tick, no inotify needed). Written new and renamed over the old, created
     with `O_EXCL`. `set(on)` is what muntin's button calls.
3. **`lp-panel`: `present(scr, canvas, cursor, saver)`**, the frame onto the screen, shared by
   `run` and muntin, so muntin's own `present` goes and a third loop can't drift. `run` holds a
   `Saver`, presents when a step is due even with nothing dirty, and takes the offset off a
   pointer's position before `Panel::point`, so a click lands where it's drawn. The arrow is
   drawn where the pointer really is, unshifted.
4. **Unmoved:** `snapshot` never moves, so every tool's goldens stay as they are.

### Tests

- The orbit pinned: the offsets in order, and back to 0, 0.
- `copy_shifted`: each way, and the strip filled.
- A click through `run`'s path, shifted, lands on the same `Hits` box as unshifted.
- The setting: a missing file, on, off, written and read back, under a scratch
  `XDG_STATE_HOME`.

### Settled (Warren, 2026-11-11)

- **The file:** `$XDG_STATE_HOME/muntin/saver` (else `~/.local/state/muntin/saver`), since
  muntin owns the button. A panel only reads it.
- **With no file yet, it's on.** Off stays off across a restart, as the handoff says.
- **The path:** a square, up to 4 pixels each way, a step every 3 minutes. A real screen may
  still move the figures (Warren's gate).

### After it

- `CHANGES.md`: muntin and each panel take it at their next pin bump; muntin drops its
  `present` for `lp_panel::present` and adds the button in its own session.
- `../STYLE.md`, "A panel under muntin", gains its line (a family session's).
- dexter-kms gets it when it's on `lp-panel`.
- Warren's gate: a step on a real screen isn't seen as a jump, and muntin's button turns it off
  and on, for muntin and the panels.

## Also begun: `lp-term`, a terminal's emulator (`../Transom.md`)

A crate of its own, written fresh (Warren, 2026-11-13): the DEC parser, the grid, the PTY, keys
to bytes, and the cells drawn in `lp-paint`. For transom, a terminal under muntin, and for a
`--tty` shown in a screen. `../Transom.md`, "The suite's half" and steps 1 and 2, has what it is.
Begun 2026-11-13 from transom's session, at Warren's go-ahead; it goes beside the saver, not
ahead of it.

- **Step 1, done 2026-11-13:** the parser and the grid, `Term` (`CHANGES.md`).
- **Step 2, done 2026-11-13; Warren's gate passed that day ("all tests successful"):** the PTY, keys to bytes, the grid
  drawn (`lp_paint::grid`), and `cargo run -p lp-panel --example term`, a terminal standing
  alone on a screen (`CHANGES.md`). Run headless (`--snapshot`) with `/bin/sh`, `less`, `vim`
  and `htop` under `env -i` and a scratch `HOME`. The gate asked for `bash`, `vim`, `less`,
  `htop` and `mc` on a real screen; Warren ran it with the build installed as
  `~/.local/bin/term` and said "all tests successful" (2026-11-13).
