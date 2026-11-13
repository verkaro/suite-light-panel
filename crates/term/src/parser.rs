//! The parser: bytes in, actions out, with no I/O. DEC's state machine for ANSI terminals, as
//! vt100.net publishes it (Paul Flo Williams'), with three changes for a terminal in UTF-8:
//!
//! - Bytes from 0x80 are UTF-8, never 8-bit C1 controls. A bad sequence prints U+FFFD, and the
//!   byte that broke it is read again on its own, so an ESC in the middle of a character still
//!   starts an escape.
//! - A CSI's parameters may hold `:` subparameters (`38:2::r:g:b`).
//! - The Linux console's palette sequences, which have no terminator: `ESC ] R` (reset) and
//!   `ESC ] P` with seven hex digits (`nrrggbb`). A plain OSC would swallow everything after
//!   `reset` (terminfo's `rs1` is `\Ec\E]R`).
//!
//! DCS, SOS, PM and APC strings are read and dropped.

/// How many parameters a CSI keeps; more are dropped.
pub(crate) const MAX_PARAMS: usize = 32;
/// How long an OSC may be; more is dropped.
const MAX_OSC: usize = 1024;
/// How many intermediates a sequence may have before it's ignored.
const MAX_INTERMEDIATES: usize = 2;

/// What the parser asks of its terminal.
pub(crate) trait Perform {
    /// A character to draw.
    fn print(&mut self, c: char);
    /// A C0 control.
    fn execute(&mut self, byte: u8);
    /// `CSI`: the private marker (`?`, `>`, `<`, `=`, or 0), the parameters, the intermediates
    /// and the final byte.
    fn csi(&mut self, private: u8, params: &Params, intermediates: &[u8], fin: u8);
    /// `ESC`, its intermediates and its final byte.
    fn esc(&mut self, intermediates: &[u8], fin: u8);
    /// An OSC's contents, without its terminator.
    fn osc(&mut self, data: &[u8]);
    /// Linux's `ESC ] P nrrggbb`: colour `n` set (`Some`), or `ESC ] R`: the palette reset.
    fn palette(&mut self, set: Option<(u8, [u8; 3])>);
}

/// A CSI's parameters. A missing one is 0. `sub` marks a parameter joined to the one before by
/// `:`.
#[derive(Clone, Debug, Default)]
pub(crate) struct Params {
    vals: [u16; MAX_PARAMS],
    sub: [bool; MAX_PARAMS],
    len: usize,
}

impl Params {
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    /// Parameter `i`, 0 if it's missing.
    pub(crate) fn get(&self, i: usize) -> u16 {
        if i < self.len { self.vals[i] } else { 0 }
    }

    /// Parameter `i`, or `default` where it's missing or 0, as most sequences take it.
    pub(crate) fn or(&self, i: usize, default: u16) -> u16 {
        match self.get(i) {
            0 => default,
            n => n,
        }
    }

    /// Whether parameter `i` was joined to the one before by `:`.
    pub(crate) fn is_sub(&self, i: usize) -> bool {
        i < self.len && self.sub[i]
    }

    fn push(&mut self, val: u16, sub: bool) {
        if self.len < MAX_PARAMS {
            self.vals[self.len] = val;
            self.sub[self.len] = sub;
            self.len += 1;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum State {
    #[default]
    Ground,
    Escape,
    EscapeIntermediate,
    CsiEntry,
    CsiParam,
    CsiIntermediate,
    CsiIgnore,
    Osc,
    /// Linux's `ESC ] P`, reading its seven hex digits.
    LinuxPalette,
    /// DCS, SOS, PM and APC: everything up to ESC (the start of ST), CAN or SUB is dropped.
    StringSkip,
}

/// The parser's state between bytes.
#[derive(Clone, Debug, Default)]
pub(crate) struct Parser {
    state: State,
    params: Params,
    /// The parameter being read, and whether it follows a `:`.
    param: u16,
    param_sub: bool,
    private: u8,
    intermediates: [u8; MAX_INTERMEDIATES],
    n_intermediates: usize,
    /// More intermediates than fit: the sequence is read and not acted on.
    overflow: bool,
    osc: Vec<u8>,
    hex: [u8; 7],
    n_hex: usize,
    /// UTF-8: the code point so far, the continuation bytes still wanted, and the least the
    /// code point may be (below it is overlong).
    cp: u32,
    need: u8,
    min: u32,
}

impl Parser {
    pub(crate) fn advance<P: Perform>(&mut self, p: &mut P, byte: u8) {
        if self.need > 0 {
            if byte & 0xC0 == 0x80 {
                self.cp = (self.cp << 6) | u32::from(byte & 0x3F);
                self.need -= 1;
                if self.need == 0 {
                    self.utf8_done(p);
                }
                return;
            }
            // The character was cut short: say so, and read this byte afresh.
            self.need = 0;
            p.print(char::REPLACEMENT_CHARACTER);
        }

        // From any state.
        match byte {
            0x18 | 0x1A => {
                p.execute(byte);
                self.state = State::Ground;
                return;
            }
            0x1B => {
                if self.state == State::Osc {
                    p.osc(&self.osc);
                }
                self.clear();
                self.state = State::Escape;
                return;
            }
            _ => {}
        }

        match self.state {
            State::Ground => self.ground(p, byte),
            State::Escape => match byte {
                0x00..=0x1F => p.execute(byte),
                0x20..=0x2F => {
                    self.collect(byte);
                    self.state = State::EscapeIntermediate;
                }
                b'[' => {
                    self.clear();
                    self.state = State::CsiEntry;
                }
                b']' => {
                    self.osc.clear();
                    self.state = State::Osc;
                }
                b'P' | b'X' | b'^' | b'_' => self.state = State::StringSkip,
                0x30..=0x7E => {
                    p.esc(&[], byte);
                    self.state = State::Ground;
                }
                _ => {}
            },
            State::EscapeIntermediate => match byte {
                0x00..=0x1F => p.execute(byte),
                0x20..=0x2F => self.collect(byte),
                0x30..=0x7E => {
                    if !self.overflow {
                        p.esc(&self.intermediates[..self.n_intermediates], byte);
                    }
                    self.state = State::Ground;
                }
                _ => {}
            },
            State::CsiEntry | State::CsiParam => match byte {
                0x00..=0x1F => p.execute(byte),
                b'0'..=b'9' => {
                    self.param = self.param.saturating_mul(10).saturating_add(u16::from(byte - b'0'));
                    self.state = State::CsiParam;
                }
                b';' | b':' => {
                    self.params.push(self.param, self.param_sub);
                    self.param = 0;
                    self.param_sub = byte == b':';
                    self.state = State::CsiParam;
                }
                0x3C..=0x3F => {
                    if self.state == State::CsiEntry {
                        self.private = byte;
                        self.state = State::CsiParam;
                    } else {
                        self.state = State::CsiIgnore;
                    }
                }
                0x20..=0x2F => {
                    self.collect(byte);
                    self.state = State::CsiIntermediate;
                }
                0x40..=0x7E => self.csi_dispatch(p, byte),
                _ => {}
            },
            State::CsiIntermediate => match byte {
                0x00..=0x1F => p.execute(byte),
                0x20..=0x2F => self.collect(byte),
                0x30..=0x3F => self.state = State::CsiIgnore,
                0x40..=0x7E => self.csi_dispatch(p, byte),
                _ => {}
            },
            State::CsiIgnore => match byte {
                0x00..=0x1F => p.execute(byte),
                0x40..=0x7E => self.state = State::Ground,
                _ => {}
            },
            State::Osc => match byte {
                0x07 => {
                    p.osc(&self.osc);
                    self.state = State::Ground;
                }
                b'R' if self.osc.is_empty() => {
                    p.palette(None);
                    self.state = State::Ground;
                }
                b'P' if self.osc.is_empty() => {
                    self.n_hex = 0;
                    self.state = State::LinuxPalette;
                }
                0x00..=0x1F => {}
                _ => {
                    if self.osc.len() < MAX_OSC {
                        self.osc.push(byte);
                    }
                }
            },
            State::LinuxPalette => {
                let Some(digit) = (byte as char).to_digit(16) else {
                    // Not a palette after all: dropped, as the console does.
                    self.state = State::Ground;
                    return;
                };
                self.hex[self.n_hex] = digit as u8;
                self.n_hex += 1;
                if self.n_hex == self.hex.len() {
                    let h = self.hex;
                    p.palette(Some((h[0], [h[1] << 4 | h[2], h[3] << 4 | h[4], h[5] << 4 | h[6]])));
                    self.state = State::Ground;
                }
            }
            State::StringSkip => {}
        }
    }

    fn ground<P: Perform>(&mut self, p: &mut P, byte: u8) {
        match byte {
            0x00..=0x1F => p.execute(byte),
            0x20..=0x7E => p.print(byte as char),
            0x7F => {}
            0xC2..=0xDF => self.utf8_start(u32::from(byte & 0x1F), 1, 0x80),
            0xE0..=0xEF => self.utf8_start(u32::from(byte & 0x0F), 2, 0x800),
            0xF0..=0xF4 => self.utf8_start(u32::from(byte & 0x07), 3, 0x1_0000),
            // A stray continuation, or a byte UTF-8 never uses.
            _ => p.print(char::REPLACEMENT_CHARACTER),
        }
    }

    fn utf8_start(&mut self, cp: u32, need: u8, min: u32) {
        self.cp = cp;
        self.need = need;
        self.min = min;
    }

    fn utf8_done<P: Perform>(&mut self, p: &mut P) {
        match char::from_u32(self.cp) {
            // C1 controls arriving as UTF-8 are dropped, not obeyed.
            Some(c) if self.cp >= self.min && !('\u{80}'..='\u{9F}').contains(&c) => p.print(c),
            Some(_) if self.cp >= self.min => {}
            _ => p.print(char::REPLACEMENT_CHARACTER),
        }
    }

    fn clear(&mut self) {
        self.params = Params::default();
        self.param = 0;
        self.param_sub = false;
        self.private = 0;
        self.n_intermediates = 0;
        self.overflow = false;
    }

    fn collect(&mut self, byte: u8) {
        if self.n_intermediates < MAX_INTERMEDIATES {
            self.intermediates[self.n_intermediates] = byte;
            self.n_intermediates += 1;
        } else {
            self.overflow = true;
        }
    }

    fn csi_dispatch<P: Perform>(&mut self, p: &mut P, fin: u8) {
        self.params.push(self.param, self.param_sub);
        if !self.overflow {
            p.csi(self.private, &self.params, &self.intermediates[..self.n_intermediates], fin);
        }
        self.state = State::Ground;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every action, written down.
    #[derive(Default)]
    struct Log(Vec<String>);

    impl Perform for Log {
        fn print(&mut self, c: char) {
            match self.0.last_mut() {
                Some(s) if s.starts_with('"') => s.push(c),
                _ => self.0.push(format!("\"{c}")),
            }
        }
        fn execute(&mut self, byte: u8) {
            self.0.push(format!("x{byte:02x}"));
        }
        fn csi(&mut self, private: u8, params: &Params, inter: &[u8], fin: u8) {
            let mut s = String::from("CSI ");
            if private != 0 {
                s.push(private as char);
            }
            for i in 0..params.len() {
                if i > 0 {
                    s.push(if params.is_sub(i) { ':' } else { ';' });
                }
                s += &params.get(i).to_string();
            }
            s += &String::from_utf8_lossy(inter);
            s.push(fin as char);
            self.0.push(s);
        }
        fn esc(&mut self, inter: &[u8], fin: u8) {
            self.0.push(format!("ESC {}{}", String::from_utf8_lossy(inter), fin as char));
        }
        fn osc(&mut self, data: &[u8]) {
            self.0.push(format!("OSC {}", String::from_utf8_lossy(data)));
        }
        fn palette(&mut self, set: Option<(u8, [u8; 3])>) {
            self.0.push(format!("palette {set:?}"));
        }
    }

    fn parse(bytes: &[u8]) -> Vec<String> {
        let mut parser = Parser::default();
        let mut log = Log::default();
        for &b in bytes {
            parser.advance(&mut log, b);
        }
        log.0
    }

    #[test]
    fn text_and_controls() {
        assert_eq!(parse(b"ab\r\ncd\x07"), ["\"ab", "x0d", "x0a", "\"cd", "x07"]);
        assert_eq!(parse(b"a\x7fb"), ["\"ab"], "DEL is ignored");
    }

    #[test]
    fn csi_parameters() {
        assert_eq!(parse(b"\x1b[H"), ["CSI 0H"]);
        assert_eq!(parse(b"\x1b[12;34H"), ["CSI 12;34H"]);
        assert_eq!(parse(b"\x1b[;5H"), ["CSI 0;5H"]);
        assert_eq!(parse(b"\x1b[?1049h"), ["CSI ?1049h"]);
        assert_eq!(parse(b"\x1b[38:2::1:2:3m"), ["CSI 38:2:0:1:2:3m"]);
        assert_eq!(parse(b"\x1b[3 q"), ["CSI 3 q"]);
        assert_eq!(parse(b"\x1b[99999999C"), ["CSI 65535C"], "saturates");
        let many: Vec<u8> = b"\x1b[".iter().chain(b"1;".repeat(40).iter()).chain(b"m").copied().collect();
        let log = parse(&many);
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].matches(';').count(), MAX_PARAMS - 1, "extra parameters dropped");
    }

    #[test]
    fn csi_oddities() {
        assert_eq!(parse(b"\x1b[1\r2A"), ["x0d", "CSI 12A"], "a C0 inside still executes");
        assert_eq!(parse(b"\x1b[1\x18A"), ["x18", "\"A"], "CAN aborts");
        assert_eq!(parse(b"\x1b[1\x1b[2A"), ["CSI 2A"], "ESC starts again");
        assert_eq!(parse(b"\x1b[1?2Ab"), ["\"b"], "a private marker late is ignored to the final");
        assert_eq!(parse(b"\x1b[1 !\"Ab"), ["\"b"], "too many intermediates");
        assert_eq!(parse(b"\x1b(0\x1b)B"), ["ESC (0", "ESC )B"]);
        assert_eq!(parse(b"\x1b#8\x1bc"), ["ESC #8", "ESC c"]);
    }

    #[test]
    fn utf8() {
        assert_eq!(parse("é€😀".as_bytes()), ["\"é€😀"]);
        let r = "\"\u{fffd}";
        assert_eq!(parse(b"\x80"), [r], "a stray continuation");
        assert_eq!(parse(b"\xc0\xaf"), [format!("{r}\u{fffd}")], "C0 never starts one");
        assert_eq!(parse(b"\xe0\x80\xaf"), [r], "overlong");
        assert_eq!(parse(b"\xed\xa0\x80"), [r], "a surrogate");
        assert_eq!(parse(b"\xf4\x90\x80\x80"), [r], "past U+10FFFF");
        assert_eq!(parse(b"\xe2\x82a"), [format!("{r}a")], "cut short by ASCII");
        assert_eq!(parse(b"\xe2\x1b[A"), [r, "CSI 0A"], "cut short by ESC");
        assert_eq!(parse(b"\x9b2A"), [format!("{r}2A")], "0x9B is not CSI");
        assert_eq!(parse(b"\xc2\x9b2A"), ["\"2A"], "nor is U+009B");
    }

    #[test]
    fn osc() {
        assert_eq!(parse(b"\x1b]0;hi\x07x"), ["OSC 0;hi", "\"x"]);
        assert_eq!(parse(b"\x1b]2;hi\x1b\\x"), ["OSC 2;hi", "ESC \\", "\"x"]);
        assert_eq!(parse("\x1b]2;é\x07".as_bytes()), ["OSC 2;é"]);
        assert_eq!(parse(b"\x1b]2;a\x18x"), ["x18", "\"x"], "CAN drops it");
        let long: Vec<u8> = b"\x1b]2;".iter().chain(&[b'a'; 5000]).chain(b"\x07").copied().collect();
        assert_eq!(parse(&long)[0].len(), "OSC ".len() + MAX_OSC);
    }

    #[test]
    fn linux_palette() {
        assert_eq!(parse(b"\x1bc\x1b]Rok"), ["ESC c", "palette None", "\"ok"], "rs1, then text");
        assert_eq!(parse(b"\x1b]P1ff8000ok"), ["palette Some((1, [255, 128, 0]))", "\"ok"]);
        assert_eq!(parse(b"\x1b]Pfok"), ["\"k"], "not hex: dropped");
    }

    #[test]
    fn strings_skipped() {
        assert_eq!(parse(b"\x1bP1;2|abc\x07def\x1b\\x"), ["ESC \\", "\"x"]);
        assert_eq!(parse(b"\x1b_apc\x1b\\x"), ["ESC \\", "\"x"]);
    }
}
