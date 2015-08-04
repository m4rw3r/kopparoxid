use std::fmt;
use std::cmp;
use std::str;

mod sequences;

pub use self::sequences::{
    CharAttr,
    CharType,
    Charset,
    CharsetIndex,
    Color,
    EraseInDisplay,
    EraseInLine,
    KeypadMode,
    Mode,
    PrivateMode,
    Seq,
};

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
    CharAttrError,
    UnknownCharset(u8, Option<u8>),
    UnknownCSI(Vec<u8>),
    UnknownOSC(Vec<u8>),
    UnknownEscapeChar(u8),
    UnknownSetReset(u32),
    UnknownSetResetData(Vec<u8>),
    UnknownPrivateSetReset(u32),
    UnknownPrivateSetResetData(Vec<u8>),
    UnexpectedUTF8Byte(u8),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::Error::*;

        match *self {
            UnknownCSI(ref data)              => write!(f, "Unknown control sequence: {:?}", String::from_utf8_lossy(data)),
            UnknownOSC(ref data)              => write!(f, "Unknown operating system command: {:?}", String::from_utf8_lossy(data)),
            UnknownCharset(c, None)           => write!(f, "Unknown charset sequence: {:?}", c),
            UnknownCharset(c, Some(d))        => write!(f, "Unknown charset sequence: {:?} {:?}", c, d),
            UnknownEscapeChar(c)              => write!(f, "Unknown escape character: {:?}", c),
            UnknownSetReset(m)                => write!(f, "Unknown set/reset mode: {:?}", m),
            UnknownPrivateSetReset(m)         => write!(f, "Unknown private set/reset mode: {:?}", m),
            UnknownSetResetData(ref d)        => write!(f, "Unknown set/reset mode data: {:?}", String::from_utf8_lossy(d)),
            UnknownPrivateSetResetData(ref d) => write!(f, "Unknown private set/reset mode data: {:?}", String::from_utf8_lossy(d)),
            UnexpectedUTF8Byte(b)             => write!(f, "Unexpected UTF8 byte: {:?}", b),
            CharAttrError                     => write!(f, "Error parsing character attribute"),
        }
    }
}

/// Attempts to parse characters or escape sequences from the given buffer.
pub fn parser(buffer: &[u8]) -> Parsed<Seq, Error> {
    match buffer.first() {
        Some(&0x05) => Parsed::Data(1, Seq::ReturnTerminalStatus),
        Some(&0x07) => Parsed::Data(1, Seq::Bell),
        Some(&0x08) => Parsed::Data(1, Seq::Backspace),
        Some(&0x09) => Parsed::Data(1, Seq::Tab),
        Some(&0x0A) => Parsed::Data(1, Seq::LineFeed),
        Some(&0x0B) => Parsed::Data(1, Seq::TabVertical),
        Some(&0x0C) => Parsed::Data(1, Seq::FormFeed),
        Some(&0x0D) => Parsed::Data(1, Seq::CarriageReturn),
        Some(&0x0E) => Parsed::Data(1, Seq::ShiftOut),
        Some(&0x0F) => Parsed::Data(1, Seq::ShiftIn),

        Some(&0x1B) => parse_esc(&buffer[1..]).inc_used(1),

        Some(&c) if c > 127 => parse_multibyte(c, &buffer[1..]).inc_used(1),
        Some(&c)            => Parsed::Data(1, Seq::Unicode(c as u32)),

        None => Parsed::Incomplete,
    }
}

#[inline]
fn parse_esc(buffer: &[u8]) -> Parsed<Seq, Error> {
    match buffer.first() {
        Some(&b'D')  => Parsed::Data(1, Seq::Index), /* IND */
        Some(&b'E')  => Parsed::Data(1, Seq::NextLine), /* NEL */
        Some(&b'H')  => Parsed::Data(1, Seq::TabSet), /* HTS */
        Some(&b'M')  => Parsed::Data(1, Seq::ReverseIndex), /* RI */
        Some(&b'N')  => Parsed::Data(1, Seq::SingleShiftSelectG2CharSet), /* SS2 */
        Some(&b'O')  => Parsed::Data(1, Seq::SingleShiftSelectG3CharSet), /* SS3 */
        Some(&b'P')  => Parsed::Data(1, Seq::DeviceControlString), /* DCS */
        Some(&b'V')  => Parsed::Data(1, Seq::StartOfGuardedArea), /* SPA */
        Some(&b'W')  => Parsed::Data(1, Seq::EndOfGuardedArea), /* EPA */
        Some(&b'X')  => Parsed::Data(1, Seq::StartOfString), /* SOS */
        Some(&b'Z')  => Parsed::Data(1, Seq::ReturnTerminalId), /* DECID */
        Some(&b'[')  => parse_csi(&buffer[1..]).inc_used(1),
        Some(&b'\\') => Parsed::Data(1, Seq::StringTerminator), /* ST */
        Some(&b']')  => parse_osc(&buffer[1..]).inc_used(1), /* OSC */
        Some(&b'^')  => Parsed::Data(1, Seq::PrivacyMessage), /* PM */
        Some(&b'_')  => Parsed::Data(1, Seq::ApplicationProgramCommand), /* APC */

        Some(&b'>')  => Parsed::Data(1, Seq::SetKeypadMode(KeypadMode::Numeric)),
        Some(&b'=')  => Parsed::Data(1, Seq::SetKeypadMode(KeypadMode::Application)),
        Some(&b'(')  => parse_charset(CharsetIndex::G0, &buffer[1..]).inc_used(1),
        Some(&b')')  => parse_charset(CharsetIndex::G1, &buffer[1..]).inc_used(1),
        Some(&b'*')  => parse_charset(CharsetIndex::G2, &buffer[1..]).inc_used(1),
        Some(&b'+')  => parse_charset(CharsetIndex::G3, &buffer[1..]).inc_used(1),

        Some(&c)     => Parsed::Error(1, Error::UnknownEscapeChar(c)),

        None         => Parsed::Incomplete,
    }
}

/// Attempts to parse a control sequence
fn parse_csi(buffer: &[u8]) -> Parsed<Seq, Error> {
    let mut private = false;

    for (i, &c) in buffer.iter().enumerate() {
        match c {
            b'h' => return match private {
                true  => multiple(&buffer[1..i], parse_private_mode).map(|a| Seq::PrivateModeSet(a)).inc_used(2),
                false => multiple(&buffer[..i], parse_mode).map(|a| Seq::ModeSet(a)).inc_used(1),
            },
            b'l' => return match private {
                true  => multiple(&buffer[1..i], parse_private_mode).map(|a| Seq::PrivateModeReset(a)).inc_used(2),
                false => multiple(&buffer[..i], parse_mode).map(|a| Seq::ModeReset(a)).inc_used(1),
            },
            // Color codes
            // Multiple control sequences for color codes can be present in the same sequence.

            // No parameters equals ``CSI 0 m`` which means Reset
            b'm' => return multiple_w_default(&buffer[..i], parse_char_attr, CharAttr::Reset)
                .map(|a| Seq::CharAttr(a))
                .map_err(|_| Error::CharAttrError)
                .inc_used(1),
            b'H' => return {
                // 1-indexed coordinates, row;col, defaults to 1 if not present.
                let mut int_buf = Window::new(&buffer[..i]);

                let row = cmp::max(1, int_buf.next::<usize>().unwrap_or(1));
                let col = cmp::max(1, int_buf.next::<usize>().unwrap_or(1));

                Parsed::Data(i + 1, Seq::CursorPosition(row - 1, col - 1))
            },
            b'J' => return Parsed::Data(i + 1, match parse_int::<u8>(&buffer[..i]) {
                Some(1) => Seq::EraseInDisplay(EraseInDisplay::Above),
                Some(2) => Seq::EraseInDisplay(EraseInDisplay::All),
                _       => Seq::EraseInDisplay(EraseInDisplay::Below),
            }),
            b'K' => return Parsed::Data(i + 1, match parse_int::<u8>(&buffer[..i]) {
                Some(1) => Seq::EraseInLine(EraseInLine::Left),
                Some(2) => Seq::EraseInLine(EraseInLine::All),
                _       => Seq::EraseInLine(EraseInLine::Right),
            }),
            b'?' => private = true, /* In buffer, set private mode for mode-setters/resetters */
            c if c >= 0x40 && c <= 0x7E =>
                return Parsed::Error(i + 1, Error::UnknownCSI(From::from(&buffer[..i + 1]))),
            _ => {} /* In buffer */
        }
    }

    Parsed::Incomplete
}

/// Attempts to parse an operating system command from the given buffer.
fn parse_osc(buffer: &[u8]) -> Parsed<Seq, Error> {
    for (i, &c) in buffer.iter().enumerate() {
        // ``ESC \`` = ``ST``
        // ie. 0x1B 0x5C => 0x07
        if ! (c == 0x07 || i > 0 && (c == 0x5C && buffer.get(i - 1) == Some(&0x1B))) {
            // Not end of OSC (ST or ESC \)
            continue;
        }

        let end = if c == 0x5C  {
            i - 1
        } else {
            i
        };

        let mut b = Window::new(&buffer[..end]);

        return match b.next::<u8>() {
            Some(0) => Parsed::Data(i + 1, Seq::SetWindowTitle(String::from_utf8_lossy(b.rest()).into_owned())), // And icon name
            Some(1) => Parsed::Data(i + 1, Seq::SetIconName(String::from_utf8_lossy(b.rest()).into_owned())),
            Some(2) => Parsed::Data(i + 1, Seq::SetWindowTitle(String::from_utf8_lossy(b.rest()).into_owned())),
            Some(3) => Parsed::Data(i + 1, Seq::SetXProps(String::from_utf8_lossy(b.rest()).into_owned())),
            Some(4) => Parsed::Data(i + 1, Seq::SetColorNumber(String::from_utf8_lossy(b.rest()).into_owned())),
            _       => Parsed::Error(i + 1, Error::UnknownOSC(From::from(&buffer[..i + 1]))),
        }
    }

    Parsed::Incomplete
}

fn parse_charset(index: CharsetIndex, buffer: &[u8]) -> Parsed<Seq, Error> {
    match buffer.first() {
        Some(&b'0') => Parsed::Data(1, Seq::Charset(index, Charset::DECSpecialAndLineDrawing)),
        Some(&b'<') => Parsed::Data(1, Seq::Charset(index, Charset::DECSupplementary)),
        Some(&b'>') => Parsed::Data(1, Seq::Charset(index, Charset::DECTechnical)),
        Some(&b'A') => Parsed::Data(1, Seq::Charset(index, Charset::UnitedKingdom)),
        Some(&b'B') => Parsed::Data(1, Seq::Charset(index, Charset::UnitedStates)),
        Some(&b'4') => Parsed::Data(1, Seq::Charset(index, Charset::Dutch)),
        Some(&b'C') => Parsed::Data(1, Seq::Charset(index, Charset::Finnish)),
        Some(&b'5') => Parsed::Data(1, Seq::Charset(index, Charset::Finnish)),
        Some(&b'R') => Parsed::Data(1, Seq::Charset(index, Charset::French)),
        Some(&b'f') => Parsed::Data(1, Seq::Charset(index, Charset::French)),
        Some(&b'Q') => Parsed::Data(1, Seq::Charset(index, Charset::FrenchCanadian)),
        Some(&b'9') => Parsed::Data(1, Seq::Charset(index, Charset::FrenchCanadian)),
        Some(&b'K') => Parsed::Data(1, Seq::Charset(index, Charset::German)),
        Some(&b'Y') => Parsed::Data(1, Seq::Charset(index, Charset::Italian)),
        Some(&b'`') => Parsed::Data(1, Seq::Charset(index, Charset::NorwegianDanish)),
        Some(&b'E') => Parsed::Data(1, Seq::Charset(index, Charset::NorwegianDanish)),
        Some(&b'6') => Parsed::Data(1, Seq::Charset(index, Charset::NorwegianDanish)),
        Some(&b'Z') => Parsed::Data(1, Seq::Charset(index, Charset::Spanish)),
        Some(&b'H') => Parsed::Data(1, Seq::Charset(index, Charset::Swedish)),
        Some(&b'7') => Parsed::Data(1, Seq::Charset(index, Charset::Swedish)),
        Some(&b'=') => Parsed::Data(1, Seq::Charset(index, Charset::Swiss)),
        Some(&b'%') => match buffer.get(1) {
            Some(&b'5') => Parsed::Data(2, Seq::Charset(index, Charset::DECSupplementaryGraphics)),
            Some(&b'6') => Parsed::Data(2, Seq::Charset(index, Charset::Portuguese)),
            Some(&c)    => Parsed::Error(2, Error::UnknownCharset(b'%', Some(c))),
            None        => Parsed::Incomplete,
        },
        Some(&c) => Parsed::Error(1, Error::UnknownCharset(c, None)),
        None     => Parsed::Incomplete,
    }
}

/// Attempts to parse the continuation of a multibyte character (UTF-8).
#[inline]
fn parse_multibyte(c: u8, buffer: &[u8]) -> Parsed<Seq, Error> {
    debug_assert!(c > 127);

    let tail    = UTF8_TRAILING[c as usize];
    let mut i   = 0;
    let mut chr = (c as u32) & (0xff >> (tail + 2));

    for &c in buffer.iter().take(tail as usize) {
        if c <= 127 {
            return Parsed::Error(i + 1, Error::UnexpectedUTF8Byte(c));
        }

        chr = (chr << 6) + ((c as u32) & 0x3f);
        i   = i + 1;
    }

    if i == tail as usize {
        Parsed::Data(tail as usize, Seq::Unicode(chr))
    } else {
        Parsed::Incomplete
    }
}

/// Attempts to parse multiple items but if the buffer is empty will return the default
#[inline]
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
#[inline]
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
            Parsed::Error(consumed, err) => return Parsed::Error(cursor + consumed, err),
            Parsed::Incomplete           => return Parsed::Incomplete,
        }

        if cursor == buffer.len() {
            break;
        }
    }

    Parsed::Data(cursor, items)
}

/// Parses a single mode attribute.
/// 
/// Expects to receive data after the sequences ``ESC [`` but before ``h`` or ``l``.
fn parse_mode(buffer: &[u8]) -> Parsed<Mode, Error> {
    use self::Mode::*;

    let mut int_buf = Window::new(buffer);

    macro_rules! ret { ( $ret:expr ) => ( Parsed::Data(int_buf.used(), $ret) ) };

    match int_buf.next::<u32>() {
        Some(2)  => ret!(KeyboardAction),
        Some(4)  => ret!(Insert),
        Some(12) => ret!(SendReceive),
        Some(20) => ret!(AutomaticNewline),
        Some(n)  => Parsed::Error(buffer.len(), Error::UnknownSetReset(n)),
        None     => Parsed::Error(buffer.len(), Error::UnknownSetResetData(buffer.to_owned())),
    }
}

/// Parses a single private mode attribute.
/// 
/// Expects to receive data after the sequences ``ESC [ ?`` but before ``h`` or ``l``.
fn parse_private_mode(buffer: &[u8]) -> Parsed<PrivateMode, Error> {
    use self::PrivateMode::*;

    let mut int_buf = Window::new(buffer);

    macro_rules! ret { ( $ret:expr ) => ( Parsed::Data(int_buf.used(), $ret) ) };

    match int_buf.next::<u32>() {
        Some(1) => ret!(ApplicationCursorKeys),
        Some(n) => Parsed::Error(buffer.len(), Error::UnknownPrivateSetReset(n)),
        None    => Parsed::Error(buffer.len(), Error::UnknownPrivateSetResetData(buffer.to_owned())),
    }
}

/// Parses a single character attribute
/// 
/// Expects to receive data after the sequence ``ESC [`` but before ``m``,
/// if a previous number has been read it expects to receive data after the following ``;``.
fn parse_char_attr(buffer: &[u8]) -> Parsed<CharAttr, ()> {
    use self::CharAttr::*;
    use self::CharType::*;
    use self::Color::*;

    let mut int_buf = Window::new(buffer);

    macro_rules! ret { ( $ret:expr ) => ( Parsed::Data(int_buf.used(), $ret) ) };
    macro_rules! err { ()            => ( Parsed::Error(int_buf.used(), ()) ) };

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
                _                           => err!(),
            },
            Some(5) => if let Some(p) = int_buf.next::<u8>() {
                ret!(FGColor(Palette(p)))
            } else {
                err!()
            },
            _ => err!(),
        },
        Some(48) => match int_buf.next::<u8>() {
            Some(2) => match (int_buf.next::<u8>(), int_buf.next::<u8>(), int_buf.next::<u8>()) {
                (Some(r), Some(g), Some(b)) => ret!(BGColor(RGB(r, g, b))),
                _                           => err!(),
            },
            Some(5) => if let Some(p) = int_buf.next::<u8>() {
                ret!(BGColor(Palette(p)))
            } else {
                err!()
            },
            _ => err!(),
        },
        _ => err!(),
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
