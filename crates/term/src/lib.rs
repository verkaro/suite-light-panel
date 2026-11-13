//! A terminal's emulator, written fresh for the family (Warren, 2026-11-13): for transom, a
//! terminal under muntin, and for a tool's `--tty` shown in a screen (`../Transom.md`).
//!
//! [`Term`] reads the bytes a shell writes and keeps the screen they draw: a grid of [`Cell`]s,
//! the cursor, the [`Modes`], the scroll region, the tab stops and the scrollback, with the rows
//! changed since last asked. It does no I/O and has no dependencies.
//!
//! Its parser is DEC's state machine as vt100.net publishes it, in UTF-8. A character's width
//! is glibc's, from a table `width.sh` writes, so the grid counts cells as the programs drawing
//! into it do. GPL terminals are read, never copied.
//!
//! Still to come: the PTY and keys to bytes, here, and the cells drawn, in `lp-paint`.

mod cell;
mod parser;
mod term;
mod width;
mod width_table;

pub use cell::{Attrs, Cell, Color, Row, Width};
pub use term::{Cursor, Modes, Term};
pub use width::width;
pub use width_table::UNICODE;
