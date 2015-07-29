use std::fmt;
use std::cmp;
use std::str;

mod sequences;

pub use self::sequences::{Seq, KeypadMode, EraseInLine, EraseInDisplay, CharType, CharAttr, Color, Charset, CharsetIndex};

use ::parser::Parsed;

static UTF8_TRAILING: [u8; 256] = [
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0, 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0, 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0, 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0, 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0, 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0, 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1, 1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,
    2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2, 3,3,3,3,3,3,3,3,4,4,4,4,5,5,5,5];

#[derive(Debug)]
pub enum Error {
    ParseError(ParserState, Vec<u8>),
    CharAttrError,
    UnknownCharset(u8, Option<u8>),
    UnknownEscapeChar(u8),
    UnexpectedUTF8Byte(u8),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::ParseError(state, ref data) => write!(f, "Unknown sequence found in state {:?}: {:?}", state, String::from_utf8_lossy(data)),
            Error::UnknownCharset(c, None)     => write!(f, "Unknown charset sequence: {:?}", c),
            Error::UnknownCharset(c, Some(d))  => write!(f, "Unknown charset sequence: {:?} {:?}", c, d),
            Error::UnknownEscapeChar(c)        => write!(f, "Unknown escape character: {:?}", c),
            Error::UnexpectedUTF8Byte(b)       => write!(f, "Unexpected UTF8 byte: {:?}", b),
            Error::CharAttrError               => write!(f, "Error parsing character attribute"),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ParserState {
    Default,
    ESC,
    CSI(usize),
    OSC(usize),
    Charset(CharsetIndex),
    CharsetSuppOrPortuguese(CharsetIndex),
    Unicode(u32, u8),
}

pub fn parser(buffer: &[u8]) -> Parsed<Seq, Error> {
    let mut state = ParserState::Default;

    for (i, &c) in buffer.iter().enumerate() {
        // Yields a value, consuming data
        macro_rules! ret { ( $ret:expr ) => ( return Parsed::Data(i + 1, $ret) ) };
        // Yields an error, consuming data
        macro_rules! err { ( $err:expr ) => ( return Parsed::Error(i + 1, $err) ) };
        macro_rules! parse_err { () => { return Parsed::Error(i + 1, Error::ParseError(state, From::from(&buffer[..i + 1]))) } };

        macro_rules! buf { ( $start:expr ) => (buffer[$start..i]); };

        match state {
            ParserState::Default => match c {
                0x05 => ret!(Seq::ReturnTerminalStatus),
                0x07 => ret!(Seq::Bell),
                0x08 => ret!(Seq::Backspace),
                0x09 => ret!(Seq::Tab),
                0x0A => ret!(Seq::LineFeed),
                0x0B => ret!(Seq::TabVertical),
                0x0C => ret!(Seq::FormFeed),
                0x0D => ret!(Seq::CarriageReturn),
                0x0E => ret!(Seq::ShiftOut),
                0x0F => ret!(Seq::ShiftIn),

                0x1B => state = ParserState::ESC,

                c if c > 127 => {
                    let tail = UTF8_TRAILING[c as usize];
                    let chr  = (c as u32) & (0xff >> (tail + 2));

                    state = ParserState::Unicode(chr, tail - 1)
                },
                c => ret!(Seq::Unicode(c as u32)),
            },
            ParserState::Unicode(chr, 0) => match c {
                c if c > 127 => ret!(Seq::Unicode((chr << 6) + ((c as u32) & 0x3f))),
                c => err!(Error::UnexpectedUTF8Byte(c))
            },
            ParserState::Unicode(chr, i) => match c {
                c if c > 127 => state = ParserState::Unicode((chr << 6) + ((c as u32) & 0x3f), i - 1),
                c => err!(Error::UnexpectedUTF8Byte(c))
            },
            ParserState::ESC => match c {
                b'D' => ret!(Seq::Index), /* IND */
                b'E' => ret!(Seq::NextLine), /* NEL */
                b'H' => ret!(Seq::TabSet), /* HTS */
                b'M' => ret!(Seq::ReverseIndex), /* RI */
                b'N' => ret!(Seq::SingleShiftSelectG2CharSet), /* SS2 */
                b'O' => ret!(Seq::SingleShiftSelectG3CharSet), /* SS3 */
                b'P' => ret!(Seq::DeviceControlString), /* DCS */
                b'V' => ret!(Seq::StartOfGuardedArea), /* SPA */
                b'W' => ret!(Seq::EndOfGuardedArea), /* EPA */
                b'X' => ret!(Seq::StartOfString), /* SOS */
                b'Z' => ret!(Seq::ReturnTerminalId), /* DECID */
                b'[' => state = ParserState::CSI(i + 1),
                b'\\' => ret!(Seq::StringTerminator), /* ST */
                b']' => state = ParserState::OSC(i + 1), /* OSC */
                b'^' => ret!(Seq::PrivacyMessage), /* PM */
                b'_' => ret!(Seq::ApplicationProgramCommand), /* APC */

                b'>' => ret!(Seq::SetKeypadMode(KeypadMode::Numeric)),
                b'=' => ret!(Seq::SetKeypadMode(KeypadMode::Application)),
                b'(' => state = ParserState::Charset(CharsetIndex::G0),
                b')' => state = ParserState::Charset(CharsetIndex::G1),
                b'*' => state = ParserState::Charset(CharsetIndex::G2),
                b'+' => state = ParserState::Charset(CharsetIndex::G3),

                c => err!(Error::UnknownEscapeChar(c))
                // Some(b" ") => match 
            },
            ParserState::CSI(start) => match c {
                // Color codes
                // Multiple control sequences for color codes can be present in the same sequence.

                // No parameters equals ``CSI 0 m`` which means Reset
                b'm' => match multiple_w_default(&buf!(start), parse_char_attr, CharAttr::Reset) {
                    Parsed::Data(_, attrs) => ret!(Seq::CharAttr(attrs)),
                    Parsed::Error(_, err)  => err!(err),
                    Parsed::Incomplete     => unreachable!(),
                },
                b'J' => ret!(match parse_int::<u8>(&buf!(start)) {
                    Some(1) => Seq::EraseInDisplay(EraseInDisplay::Above),
                    Some(2) => Seq::EraseInDisplay(EraseInDisplay::All),
                    _       => Seq::EraseInDisplay(EraseInDisplay::Below),
                }),
                b'K' => ret!(match parse_int::<u8>(&buf!(start)) {
                    Some(1) => Seq::EraseInLine(EraseInLine::Left),
                    Some(2) => Seq::EraseInLine(EraseInLine::All),
                    _       => Seq::EraseInLine(EraseInLine::Right),
                }),
                b'H' => {
                    let mut int_buf = Window::new(&buf!(start));

                    let row = cmp::max(1, int_buf.next::<usize>().unwrap_or(1));
                    let col = cmp::max(1, int_buf.next::<usize>().unwrap_or(1));

                    ret!(Seq::CursorPosition(row - 1, col - 1));
                },
                c if c >= 0x40 && c <= 0x7E => parse_err!(),
                _ => {} /* In buffer */
            },
            ParserState::Charset(index) => match c {
                b'0' => ret!(Seq::Charset(index, Charset::DECSpecialAndLineDrawing)),
                b'<' => ret!(Seq::Charset(index, Charset::DECSupplementary)),
                b'>' => ret!(Seq::Charset(index, Charset::DECTechnical)),
                b'A' => ret!(Seq::Charset(index, Charset::UnitedKingdom)),
                b'B' => ret!(Seq::Charset(index, Charset::UnitedStates)),
                b'4' => ret!(Seq::Charset(index, Charset::Dutch)),
                b'C' => ret!(Seq::Charset(index, Charset::Finnish)),
                b'5' => ret!(Seq::Charset(index, Charset::Finnish)),
                b'R' => ret!(Seq::Charset(index, Charset::French)),
                b'f' => ret!(Seq::Charset(index, Charset::French)),
                b'Q' => ret!(Seq::Charset(index, Charset::FrenchCanadian)),
                b'9' => ret!(Seq::Charset(index, Charset::FrenchCanadian)),
                b'K' => ret!(Seq::Charset(index, Charset::German)),
                b'Y' => ret!(Seq::Charset(index, Charset::Italian)),
                b'`' => ret!(Seq::Charset(index, Charset::NorwegianDanish)),
                b'E' => ret!(Seq::Charset(index, Charset::NorwegianDanish)),
                b'6' => ret!(Seq::Charset(index, Charset::NorwegianDanish)),
                b'Z' => ret!(Seq::Charset(index, Charset::Spanish)),
                b'H' => ret!(Seq::Charset(index, Charset::Swedish)),
                b'7' => ret!(Seq::Charset(index, Charset::Swedish)),
                b'=' => ret!(Seq::Charset(index, Charset::Swiss)),
                b'%' => state = ParserState::CharsetSuppOrPortuguese(index),
                c    => err!(Error::UnknownCharset(c, None)),
            },
            ParserState::CharsetSuppOrPortuguese(index) => match c {
                b'5' => ret!(Seq::Charset(index, Charset::DECSupplementaryGraphics)),
                b'6' => ret!(Seq::Charset(index, Charset::Portuguese)),
                c    => err!(Error::UnknownCharset(b'%', Some(c))),
            },
            ParserState::OSC(start) => {
                // ``ESC \`` = ``ST``
                // ie. 0x1B 0x5C => 0x07
                if c != 0x07 && (c != 0x5C && buffer.get(i - 1) != Some(&0x1B)) {
                    // Not end of OSC (ST or ESC \)
                    continue;
                }

                let end = if c == 0x5C  {
                    i - 1
                } else {
                    i
                };

                let mut b = Window::new(&buffer[start..end]);

                match b.next::<u8>() {
                    Some(0) => ret!(Seq::SetWindowTitle(String::from_utf8_lossy(b.rest()).into_owned())), // And icon name
                    Some(1) => ret!(Seq::SetIconName(String::from_utf8_lossy(b.rest()).into_owned())),
                    Some(2) => ret!(Seq::SetWindowTitle(String::from_utf8_lossy(b.rest()).into_owned())),
                    Some(3) => ret!(Seq::SetXProps(String::from_utf8_lossy(b.rest()).into_owned())),
                    Some(4) => ret!(Seq::SetColorNumber(String::from_utf8_lossy(b.rest()).into_owned())),
                    _       => parse_err!(),
                }
            }
        }
    }

    Parsed::Incomplete
}

/// Attempts to parse multiple items but if the buffer is empty will return the default
fn multiple_w_default<F, T, E>(buffer: &[u8], f: F, default: T) -> Parsed<Vec<T>, E>
  where F: Sized + Fn(&[u8]) -> Parsed<T, E> {
    if buffer.is_empty() {
        let mut attrs  = Vec::with_capacity(1);

        attrs.push(default);

        return Parsed::Data(0, attrs);
    }

    multiple(buffer, f)
}

/// Parses multiple occurrences of the item
fn multiple<F, T, E>(buffer: &[u8], f: F) -> Parsed<Vec<T>, E>
  where F: Sized + Fn(&[u8]) -> Parsed<T, E> {
    let mut cursor = 0;
    let mut items  = Vec::new();

    loop {
        match f(&buffer[cursor..]) {
            Parsed::Data(consumed, item) => {
                cursor = cursor + consumed;

                items.push(item);
            },
            Parsed::Error(consumed, err) => return Parsed::Error(consumed, err),
            Parsed::Incomplete           => return Parsed::Incomplete,
        }

        if cursor == buffer.len() {
            break;
        }
    }

    Parsed::Data(cursor, items)
}

/// Parses a single character attribute
/// 
/// Expects to receive data after the sequence ``ESC [`` but before ``m``.
fn parse_char_attr(buffer: &[u8]) -> Parsed<CharAttr, Error> {
    use self::CharAttr::*;
    use self::CharType::*;
    use self::Color::*;

    let mut int_buf = Window::new(buffer);

    macro_rules! ret { ( $ret:expr ) => ( Parsed::Data(int_buf.used(), $ret) ) };
    macro_rules! err { ( $ret:expr ) => ( Parsed::Error(int_buf.used(), $ret) ) };

    match int_buf.next::<u8>() {
        Some(0)              => ret!(Reset),
        Some(1)              => ret!(Set(Bold)),
        Some(2)              => ret!(Set(Faint)),
        Some(3)              => ret!(Set(Italicized)),
        Some(4)              => ret!(Set(Underlined)),
        Some(5)              => ret!(Set(Blink)),
        Some(7)              => ret!(Set(Inverse)),
        Some(8)              => ret!(Set(Invisible)),
        Some(9)              => ret!(Set(CrossedOut)),
        Some(21)             => ret!(Set(DoublyUnderlined)),
        Some(22)             => ret!(Set(Normal)), /* Not bold, not faint */
        Some(23)             => ret!(Unset(Italicized)),
        Some(24)             => ret!(Unset(Underlined)),
        Some(25)             => ret!(Unset(Blink)),
        Some(27)             => ret!(Unset(Inverse)),
        Some(28)             => ret!(Unset(Invisible)),
        Some(29)             => ret!(Unset(CrossedOut)),
        Some(30) | Some(90)  => ret!(FGColor(Black)),
        Some(31) | Some(91)  => ret!(FGColor(Red)),
        Some(32) | Some(92)  => ret!(FGColor(Green)),
        Some(33) | Some(93)  => ret!(FGColor(Yellow)),
        Some(34) | Some(94)  => ret!(FGColor(Blue)),
        Some(35) | Some(95)  => ret!(FGColor(Magenta)),
        Some(36) | Some(96)  => ret!(FGColor(Cyan)),
        Some(37) | Some(97)  => ret!(FGColor(White)),
        Some(39) | Some(99)  => ret!(FGColor(Default)),
        Some(40) | Some(100) => ret!(BGColor(Black)),
        Some(41) | Some(101) => ret!(BGColor(Red)),
        Some(42) | Some(102) => ret!(BGColor(Green)),
        Some(43) | Some(103) => ret!(BGColor(Yellow)),
        Some(44) | Some(104) => ret!(BGColor(Blue)),
        Some(45) | Some(105) => ret!(BGColor(Magenta)),
        Some(46) | Some(106) => ret!(BGColor(Cyan)),
        Some(47) | Some(107) => ret!(BGColor(White)),
        Some(49)             => ret!(BGColor(Default)),
        Some(38)             => match int_buf.next::<u8>() {
            Some(2) => match (int_buf.next::<u8>(), int_buf.next::<u8>(), int_buf.next::<u8>()) {
                (Some(r), Some(g), Some(b)) => ret!(FGColor(RGB(r, g, b))),
                _                           => err!(Error::CharAttrError),
            },
            Some(5) => if let Some(p) = int_buf.next::<u8>() {
                ret!(FGColor(Palette(p)))
            } else {
                err!(Error::CharAttrError)
            },
            _ => err!(Error::CharAttrError),
        },
        Some(48) => match int_buf.next::<u8>() {
            Some(2) => match (int_buf.next::<u8>(), int_buf.next::<u8>(), int_buf.next::<u8>()) {
                (Some(r), Some(g), Some(b)) => ret!(BGColor(RGB(r, g, b))),
                _                           => err!(Error::CharAttrError),
            },
            Some(5) => if let Some(p) = int_buf.next::<u8>() {
                ret!(BGColor(Palette(p)))
            } else {
                err!(Error::CharAttrError)
            },
            _ => err!(Error::CharAttrError),
        },
        _ => err!(Error::CharAttrError),
    }
}

pub struct Window<'a> {
    buffer: &'a [u8],
    cursor: usize,
}

impl<'a> Window<'a> {
    pub fn new(buffer: &'a [u8]) -> Self {
        Window {
            buffer: buffer,
            cursor: 0,
        }
    }

    /// Yields the next integer from the buffer.
    pub fn next<T: str::FromStr>(&mut self) -> Option<T> {
        if self.cursor >= self.buffer.len() {
            return None
        }

        let partial = &self.buffer[self.cursor..];

        parse_int::<T>(match partial.iter().position(|c| *c == b';') {
            Some(p) => {
                self.cursor = self.cursor + p + 1;

                &partial[..p]
            },
            None => {
                self.cursor = self.buffer.len();

                partial
            }
        })
    }

    /// Yields the unparsed portion of the buffer
    pub fn rest(&self) -> &[u8] {
        &self.buffer[self.cursor..]
    }

    /// The number of used bytes of the buffer, includes trailing ';'.
    pub fn used(&self) -> usize {
        self.cursor
    }
}

pub fn parse_int<T: str::FromStr>(buf: &[u8]) -> Option<T> {
    unsafe {
        // Should be ok for numbers
        str::from_utf8_unchecked(buf)
    }.parse::<T>().ok()
}
