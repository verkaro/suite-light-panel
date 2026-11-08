# Changes

What changed in an interface or a behaviour, newest first: a line for each, naming the crate and
what a tool has to do about it. Read it before moving a pin. Begun 2026-11-07; for before that,
the git log.

## 2026-11-07

- `lp-panel`: `snapshot`, the harness that tests a panel without a screen, from dexter-kms:
  `Script::parse` (`--size`, `--click`, `--drag`, `--keys`), `keys_of`, and `snapshot`, which
  plays the script through the panel's own `Panel` methods and writes a PNG. A panel is
  `Headless` by saying how long to wait for its server (`settle`). Nothing to do.
- `lp-panel`, a new crate, from dexter-kms's screen loop: `run` drives a `Panel` (`take`,
  `key`, `point`, `plugged`, `tick`, `draw`) on the screen, with the console, the VT switched
  by signal, the keys and pointers (plugged in later too), and a `Wake` pipe for the panel's
  own threads. `quit()` ends it, as SIGTERM does. `vt_key` for Ctrl+Alt+F*n*. With
  `Options::frame_key`, the frame's key quits under muntin before the panel sees it. A new
  panel runs on it rather than writing its own.
- `lp-paint`: `Pen` and `Align`, from dexter-kms: rectangles and text placed on a logical
  canvas through a `View` (text set smaller, then cut with …, to fit its box; text up the
  screen; a symbol the font has; a section's rule in the dark skin). Nothing to do.
- `lp-paint`: `dark`, the family's dark skin as `page-kit.js` has it (`dark::TEXT`,
  `dark::BTN_EDGE`, …, and `dark::ALL` by the page's names), from dexter-kms. Nothing to do;
  a panel uses it in place of colours of its own.
- `lp-keys`: `FRAME_KEY` (Ctrl+Alt+Esc), `is_frame_key` and `under_frame()`, the key every panel
  shares to leave for muntin. A panel acts on it only under muntin, quitting with status 0
  (`../muntin/DexterKms.md` for dexter-kms).
- `lp-keys`: `Keyboards::set_grab(true)` reads which keys are held from each keyboard
  (`EVIOCGKEY`), so the modifiers aren't left wrong after another program had the keys.
  Nothing to do.
- `lp-kms`: `Console` puts back the VT mode it found when dropped, not always text: text
  standing alone, as before; graphics under muntin, unless muntin has gone. Nothing to do,
  but keep the `Screen` dropped before the `Console`.
- `lp-kms`: `Console::reclaim()` (graphics and switching by signal again, typed input dropped)
  and `Console::is_foreground()`, for a frame taking the console back from a panel.
- No crate changed. muntin's asks of `lp-kms` and `lp-keys` (the shared key, the VT's mode put
  back, the keys read when the grab is taken back) are listed in `CLAUDE.md`, "How it grows",
  from `../muntin/docs/rfc.md`. Their lines come here when they're made.
- No crate changed. `CLAUDE.md` and this file added.
