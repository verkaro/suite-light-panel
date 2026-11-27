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

## Planned: `lp-edit`, vi in a box (Warren, 2026-11-27)

Every panel's typing, with vi's keys: a line, or a short note of a few lines. Settled by Warren,
2026-11-27, all as Claude suggested (`../NOTES.md`, "a vi editor for a line or a short note",
has the reasoning). Not begun. When: one session at a time changes the suites, and it fits
between the bookmark's tools (Dexter, irma-sweep, vkro); Warren picks the session.

**What it is.** A crate, `lp-edit`: the text, the caret, an undo list and vi's grammar, a state
machine fed `lp_keys::Key`s. It draws nothing and opens nothing. `lp_panel::line::Line` becomes
its one-line use, so the bar (`bar::Bar`) and every field (`lp_paint::Field`) take it at once,
and no tool has two ways to type. A box of a few lines is its other use, drawn by `lp-paint`.
`--tty`'s bar takes it when that's built. Pages don't (Warren, 2026-11-27): their boxes stay the
browser's.

**Its keys:**
- A box opens typing, as now, so nobody needs vi.
- The arrows, Home, End, Backspace and Delete do what they do now, in either mode. A click or a
  tap puts the caret there.
- **Esc goes to normal mode,** shown in the box. **Esc again leaves the box,** dropping what was
  typed, as Esc leaves anything. **Enter keeps it,** from either mode. In a note, Enter in
  typing is a new line, and keeping is Enter in normal mode (Claude's, added while writing
  this, open: to be tried on a real screen).
- The box takes every key until it closes, so `s` in normal mode changes a letter and never
  stops the tape. A mark's note during REC keeps `STYLE.md`'s rule: neither key touches the
  transport.

**The fence** (Warren, 2026-11-27). A line or a short note, never a file: files are vim's, in
transom.
- In: `h` `l` `w` `b` `e` `W` `B` `E` `0` `^` `$` `f` `t` `F` `T` `;`; `x` `r` `s` `~`; `i` `a`
  `I` `A`; `d` `c` `y` with a motion, `dd` `cc` `yy` `D` `C`; `p` `P`; `u` and Ctrl+R within the
  box; a count. In a note, `j` `k` `o` `O` `J`.
- Out: ex commands, and `:` inside a box (it's the bar's key); registers beyond the one; macros;
  `.`; search in a box; visual mode.
- Settled was the fence as suggested. Added while writing this, by Claude, Warren's to strike:
  `W` `B` `E` `F` `T` `;` `~` `dd` `cc` `yy` `D` `C` `P` `J`, and `.` and visual mode named as
  out.
- `dd` is a doubled letter, which `STYLE.md` keeps for moving, never for what needs confirming.
  Inside a box nothing needs confirming, and `u` undoes it, so it stands.
- A field's limit (`Line::at_most`) holds in normal mode too: `p` past it takes what fits.

**Tests.** Key strokes in, text, caret and mode out, for every key in the fence, at the limits
(empty, full, the caret at either end), and through `Line`'s present uses, so the bar's tests
still pass unchanged but for Esc.

**When it's built.** A `CHANGES.md` line: Esc in a box no longer leaves it at once. Each tool
takes it at its next pin. `../STYLE.md`'s "Typing is a box" and "How far vi goes" then say it as
done. Warren's gate: a field, the bar and a note on a real screen, by hand.

**Who uses it most:** Dexter's mark notes, BRR's mark names, track names; discvault's panel
(Warren, 2026-11-27: it'll benefit greatly from vi navigation and text entry); q28's panel, the
heaviest user, later (`../Q28.md`).

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
