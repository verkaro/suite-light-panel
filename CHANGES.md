# Changes

What changed in an interface or a behaviour, newest first: a line for each, naming the crate and
what a tool has to do about it. Read it before moving a pin. Begun 2026-11-07; for before that,
the git log.

## 2026-11-13

- `lp-term`: the PTY and the keys, for transom's step 2. `Pty::spawn(program, args, env, dir,
  cols, rows)`: the program in a session of its own on a pseudo-terminal, with the environment
  given and nothing inherited; `read` and `write` without waiting, `resize`, `try_wait`,
  `hangup` (SIGHUP to its group, so its jobs end too). `key_bytes(key, modes)`: what a key
  sends, as terminfo's `linux` has it (F1 `ESC [[A`, Home `ESC [1~`, Backspace DEL), Ctrl and
  a letter its control byte, Alt as ESC first. It now needs `libc` and `lp-keys`. Nothing for a
  tool to do.
- `lp-paint`: `grid`, a terminal's rows drawn at the screen's own pixels in Terminus
  (`draw_row`), in `Colours::dark()`, the one place a terminal's sixteen colours are named,
  from the dark skin. A character Terminus hasn't got is a box; bold is Terminus Bold, and bright
  in the first eight colours, as the console draws it; the cursor an amber block.
  `Fonts::for_grid` (the largest Terminus that gives 80 × 24) and `Fonts::sized`. `lp-paint`
  now depends on `lp-term`, which needs only `libc` and `lp-keys`. Nothing for a tool to do.
- `lp-keys`: `us_char(code, shift)`, the US layout's character for a key whatever else is held
  (a terminal wants the letter under Ctrl), and `code::F3` to `F9` and the keypad's. Nothing
  for a tool to do.
- `lp-panel`: `examples/term.rs`, a terminal standing alone on a screen: the shell in the
  panel, no server. Ctrl+Alt+Esc leaves; Alt+Tab never reaches the shell; Shift+PgUp and
  Shift+PgDn scroll back. `--snapshot` runs it headless. Not yet on a real screen.

- `lp-term`, new: a terminal's emulator, for transom (`../Transom.md`, step 1), written fresh
  (Warren, 2026-11-13). `Term::new(cols, rows, scrollback)`, `feed(bytes)`, `resize`; the grid
  read by `row(i)` (`Row`, `Cell`, `Color`, `Attrs`, `Width`), `scrollback(i)`, `cursor()`,
  `modes()`, `title()`, `palette()`; and what's taken once: `take_dirty()` (the rows changed),
  `take_replies()` (to write to the shell), `take_bells()`. What `TERM=linux` claims, the
  alternate screen and bracketed paste. `width(c)` counts cells as glibc does (Unicode
  16.0.0, `UNICODE`). Nothing for a tool to do.

## 2026-11-12

- `lp-keys`, `lp-panel`: the frame's second key, **Alt+Tab** (Warren, 2026-11-12): under muntin,
  straight to the panel opened before this one, as on a desktop. `SWITCH_KEY`, `is_switch_key`,
  and `ask_switch`, which writes a byte into the pipe muntin names in `MUNTIN_SWITCH_FD`.
  `lp_panel::run` takes the key itself, as it does Ctrl+Alt+Esc, with `Options::frame_key`, its
  repeats and release too: on the press it asks, then quits with exit 0. Under a muntin from before the key it's swallowed and nothing
  quits. Standing alone, a panel sees Alt+Tab as before. **What a tool has to do:** move its pin
  here, and say the key in `?` beside Ctrl+Alt+Esc where it says that one. A panel with a loop
  of its own (dexter-kms) calls `is_switch_key` and `ask_switch` itself.
- `lp-keys`: `Keyboards::open` reads which keys are already held (`EVIOCGKEY`), as
  `set_grab(true)` does, so a key down when a panel starts carries its modifiers: Alt still held
  from Alt+Tab, a repeating Tab isn't Tab alone in the next panel. Nothing to do.
- `lp-panel`: `keys::keys_over`'s key column is as wide as the widest key it shows, 44 at the
  least (as before) and half a column at most, so "Alt+Tab" isn't cut short. Nothing to do; a
  list of short keys draws as it did.

- `lp-panel`: `sheet`, the FROM, verb, TO sheet (`Picker`, `Lists`, `Word`, `Line`, `Did`),
  moved in from focusrite-ctrl at `052c497` for jackbay (jackbay's `docs/dev/SPEC.md`, 8.2).
  Nothing for a tool to do. focusrite-ctrl may move its PATCH onto it at its own pace, its knobs'
  column staying its own: the picker's `[` at FROM gives the key back for that. The `parts`
  example draws it, on a taller canvas (1400 × 1340).

## 2026-11-11

- `lp-keys`: the common keys gain tier 3's letters (Warren, 2026-11-11, `../STYLE.md`, "Keys"):
  `m` "mark here", `-` "mute", `=` "solo", `o` "the loop on or off", as `as-page-kit` has them. **What
  a tool has to do** when its pin moves here: a `keys.json` that uses one of the four for
  something else fails the keys test until it moves the key or says why in `differs`. BRR (mark
  on `t`, mute and solo on `m` and `M`), foley (loop on `l`) and irma (LOOP with no key) have
  moves listed in `../STYLE.md`, "Not yet the same", item 17. irma's `m` already reads "mark here". irma's `o` (the outs) and focusrite-ctrl's `o` (the scenes) move
  off it (Warren, 2026-11-11). Patched to this folder, BRR, irma and focusrite-ctrl fail their keys
  test at once, not only when their pins move; each deals with it in its next session.

## 2026-11-10

- `lp-paint`: `Dial::inner`, a second ring inside the first: the signal after the knob's gain,
  the outer ring then what comes in (focusrite-ctrl, Warren's ask). None draws one ring, as
  before. A `Dial { … }` written out in full needs `inner: None`.
- `lp-paint`: `Dial`, drawn with `Pen::dial`, written here for focusrite-ctrl, the second panel
  to want a dial (dexter-kms has its own). It has a sweep of three quarters of a turn, a mark (0
  dB), a pointer, its label over it and its value under it, and a ring outside it lit by the
  signal: a `Disc`, in the disc's own colours (`disc_led` and the ring now share them), with a
  second ring while a clip is latched, and no ring where nothing's metered. `held` dims it and
  says `KNOB` (a line on the interface's knob), `cursor` lights the sweep amber, `typing` shows
  what's typed in a readout. It's listed in the bar by its label; `dial_part` says whether a
  click fell on the knob or the ring. `knob_pos` and `knob_db` are the law: off at 0, −40 dB a
  quarter of the way, then even in dB to the top. Nothing to do; dexter-kms may move its own
  dial onto it (`../HANDOFFS.md`).

Brought by dexter-irma, the third panel, ahead of Tape (Warren, 2026-11-09): the panel's half of
the keys and the bar (`../suite-audio-libs/PLAN.md`, item 5), and the parts `../STYLE.md` didn't
have for a panel yet. Drawn in the dark skin by its names. None has been on a real screen.

- `lp-keys`: `common`, the family's common keys by value (`KEYS`, `means`), compared with
  `page-kit.js`'s `KEYS` by a test, as the dark skin is. `Declared` is one entry of a tool's
  `keys.json`, and `check` holds a tool's list to the table: a common key that does something
  else needs its `differs`. `Key::name` names a key as a page does (`"j"`, `" "`, `"Escape"`,
  `"ArrowLeft"`), so one `keys.json` serves a page and a panel. `shown` is how `?` shows a key.
  `code_of` is the key that types a character, and `every` is every key with a name, for a
  panel's test that no key does something it doesn't list. A panel reads its `keys.json` itself:
  nothing here reads JSON. Nothing to do, until a panel takes up the table.
- `lp-panel`: `bar::Bar`, the command bar along a panel's footer, acting as `kitBar` does on a
  page: a question answered with `y`, an offer the next key decides, a line typed, a choice
  narrowed by typing, with levels. It gives back an `Act` naming the tool's own id where the
  page's bar calls a function. `Bar::screen` makes the levels from the keys drawn. A panel
  hands it every key first.
- `lp-panel`: `line::Line`, a line being typed with a caret in it, for the bar and for a form's
  field.
- `lp-panel`: `keys::keys_over`, `?`: a tool's keys over the screen, by screen, from its list.
- `lp-panel`: `snapshot::keys_of` types every character a US keyboard has, with Shift where it
  needs it (`:` and `]` weren't keys before), and names `{home}`, `{end}` and `{delete}`.
  `key_code` finds a shifted character's key too. A test that counted on `]` being refused
  wants another character.
- `lp-paint`: `Hits`, where a click can land, by id, the last drawn on top, as the page kit's
  `HITS`. A key in it carries the bar's name for it and its section. `bar_name` makes that name
  from a label.
- `lp-paint`: `Pen::key` and `KeyLook`, the family's key: plain, lit, or not for now.
- `lp-paint`: `Pen::field`, `Field` and `FieldState`, a form's field: a label and a value in a
  box, plain, under the cursor, being typed in with a caret, or waiting in amber.
- `lp-paint`: `List`, `Column`, `Cell` and `ListLook`, a list: rows in columns under their
  heads, the chosen row lit, scrolled as little as keeps it in view, a mark for where a long
  list is, each row somewhere to click.
- `lp-paint`: `Pen::wave`, `Wave`, `Scale` and `Marker`, a waveform: the lowest and highest
  sample a column, straight or in dB down to a floor, with the times along the bottom, places
  marked, and a curve over the dB picture. `Wave::time_at` takes a click back to a time. For
  looking: nothing plays from it. Tape draws one of its own, and could move to this.
- `lp-paint`: `Pen::run` and `Pen::width`, a line of text built a piece at a time.
- `lp-panel`: an example, `parts`, draws every part to a PNG.

## 2026-11-07

- `lp-panel`: `snapshot::keys_of` names four more keys, `{tab}`, `{backspace}`, `{pgup}` and
  `{pgdn}`, for Tape's panel (its focus and its typing). Nothing to do.
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
