# Changes

What changed in an interface or a behaviour, newest first: a line for each, naming the crate and
what a tool has to do about it. Read it before moving a pin. Begun 2026-11-07; for before that,
the git log.

## 2026-11-07

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
