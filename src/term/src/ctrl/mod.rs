use std::fmt;
use std::cmp;
use std::str;
use std::num;

use chomp;
use chomp::{
    U8Result,
    Input,
    parse_only,
    ParseResult
};
use chomp::ascii::decimal;
use chomp::parsers::{
    any,
    scan,
    token,
    take,
    take_till,
};
use chomp::combinators::{
    option,
    sep_by1,
};

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

pub enum Error {
    CharAttrError(Vec<u8>),
    UnknownCharset(u8, Option<u8>),
    UnknownCSI(u8, Vec<u8>),
    UnknownOSC(Vec<u8>),
    UnknownEscapeChar(u8),
    UnknownSetReset(usize),
    UnknownSetResetData(Vec<u8>),
    UnknownPrivateSetReset(usize),
    UnknownPrivateSetResetData(Vec<u8>),
    IntParseError(num::ParseIntError),
    UnexpectedUTF8Byte(u8),
    ParseError(chomp::Error<u8>),
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::Error::*;

        match *self {
            UnknownCSI(c, ref data)           => write!(f, "Unknown control sequence: {:?} {:?}", c as char, String::from_utf8_lossy(data)),
            UnknownOSC(ref data)              => write!(f, "Unknown operating system command: {:?}", String::from_utf8_lossy(data)),
            UnknownCharset(c, None)           => write!(f, "Unknown charset sequence: {:?}", c),
            UnknownCharset(c, Some(d))        => write!(f, "Unknown charset sequence: {:?} {:?}", c, d),
            UnknownEscapeChar(c)              => write!(f, "Unknown escape character: {:?}", c),
            UnknownSetReset(m)                => write!(f, "Unknown set/reset mode: {:?}", m),
            UnknownPrivateSetReset(m)         => write!(f, "Unknown private set/reset mode: {:?}", m),
            UnknownSetResetData(ref d)        => write!(f, "Unknown set/reset mode data: {:?}", String::from_utf8_lossy(d)),
            UnknownPrivateSetResetData(ref d) => write!(f, "Unknown private set/reset mode data: {:?}", String::from_utf8_lossy(d)),
            IntParseError(ref e)              => fmt::Display::fmt(e, f),
            UnexpectedUTF8Byte(b)             => write!(f, "Unexpected UTF8 byte: {:?}", b),
            CharAttrError(ref b)              => write!(f, "Error parsing character attribute: {:?}", String::from_utf8_lossy(b)),
            ParseError(ref p)                 => write!(f, "Parse error: {:?}", p),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::Error::*;

        match *self {
            UnknownCSI(c, ref data)           => write!(f, "Unknown control sequence: {:?} {:?}", c as char, String::from_utf8_lossy(data)),
            UnknownOSC(ref data)              => write!(f, "Unknown operating system command: {:?}", String::from_utf8_lossy(data)),
            UnknownCharset(c, None)           => write!(f, "Unknown charset sequence: {:?}", c),
            UnknownCharset(c, Some(d))        => write!(f, "Unknown charset sequence: {:?} {:?}", c, d),
            UnknownEscapeChar(c)              => write!(f, "Unknown escape character: {:?}", c),
            UnknownSetReset(m)                => write!(f, "Unknown set/reset mode: {:?}", m),
            UnknownPrivateSetReset(m)         => write!(f, "Unknown private set/reset mode: {:?}", m),
            UnknownSetResetData(ref d)        => write!(f, "Unknown set/reset mode data: {:?}", String::from_utf8_lossy(d)),
            UnknownPrivateSetResetData(ref d) => write!(f, "Unknown private set/reset mode data: {:?}", String::from_utf8_lossy(d)),
            IntParseError(ref e)              => fmt::Display::fmt(e, f),
            UnexpectedUTF8Byte(b)             => write!(f, "Unexpected UTF8 byte: {:?}", b),
            CharAttrError(ref b)              => write!(f, "Error parsing character attribute: {:?}", String::from_utf8_lossy(b)),
            ParseError(ref p)                 => write!(f, "Parse error: {:?}", p),
        }
    }
}

impl From<chomp::Error<u8>> for Error {
    fn from(e: chomp::Error<u8>) -> Error {
        Error::ParseError(e)
    }
}

// TODO: Upstream to Chomp and remove Copy
fn maybe<I: Copy, T, E, F>(i: Input<I>, f: F) -> ParseResult<I, Option<T>, E>
  where F: FnOnce(Input<I>) -> ParseResult<I, T, E> {
    option(i, |i| f(i).map(Some), None)
}

/// Attempts to parse characters or escape sequences from the given buffer.
pub fn parser(m: Input<u8>) -> ParseResult<u8, Seq, Error> {
    any(m).bind(|m, c| match c {
        0x05 => m.ret(Seq::ReturnTerminalStatus),
        0x07 => m.ret(Seq::Bell),
        0x08 => m.ret(Seq::Backspace),
        0x09 => m.ret(Seq::Tab),
        0x0A => m.ret(Seq::LineFeed),
        0x0B => m.ret(Seq::TabVertical),
        0x0C => m.ret(Seq::FormFeed),
        0x0D => m.ret(Seq::CarriageReturn),
        0x0E => m.ret(Seq::ShiftOut),
        0x0F => m.ret(Seq::ShiftIn),

        0x1B => parse_esc(m),
        c    => unicode(m, c),
    })
}

fn unicode(m: Input<u8>, c: u8) -> ParseResult<u8, Seq, Error> {
    match c {
        0b00000000...0b01111111 => m.ret(c as u32),
        0b11000000...0b11011111 =>                           unicode_tail(m.ret((c & 0b00011111) as u32)),
        0b11100000...0b11101111 =>              unicode_tail(unicode_tail(m.ret((c & 0b00001111) as u32))),
        0b11110000...0b11110111 => unicode_tail(unicode_tail(unicode_tail(m.ret((c & 0b00000111) as u32)))),
        _                       => m.err(Error::UnexpectedUTF8Byte(c)),
    }.map(Seq::Unicode)
}

fn unicode_tail(m: ParseResult<u8, u32, Error>) -> ParseResult<u8, u32, Error> {
    m.bind(|m, c| any(m).bind(|m, b| m.ret((c << 6) + (b & 0x3f) as u32)))
}

#[inline]
fn parse_esc(m: Input<u8>) -> ParseResult<u8, Seq, Error> {
    any(m).bind(|m, c| match c {
        b'D'  => m.ret(Seq::Index), /* IND */
        b'E'  => m.ret(Seq::NextLine), /* NEL */
        b'H'  => m.ret(Seq::TabSet), /* HTS */
        b'M'  => m.ret(Seq::ReverseIndex), /* RI */
        b'N'  => m.ret(Seq::SingleShiftSelectG2CharSet), /* SS2 */
        b'O'  => m.ret(Seq::SingleShiftSelectG3CharSet), /* SS3 */
        b'P'  => m.ret(Seq::DeviceControlString), /* DCS */
        b'V'  => m.ret(Seq::StartOfGuardedArea), /* SPA */
        b'W'  => m.ret(Seq::EndOfGuardedArea), /* EPA */
        b'X'  => m.ret(Seq::StartOfString), /* SOS */
        b'Z'  => m.ret(Seq::ReturnTerminalId), /* DECID */
        b'['  => parse_csi(m),
        b'\\' => m.ret(Seq::StringTerminator), /* ST */
        b']'  => parse_osc(m), /* OSC */
        b'^'  => m.ret(Seq::PrivacyMessage), /* PM */
        b'_'  => m.ret(Seq::ApplicationProgramCommand), /* APC */

        b'>'  => m.ret(Seq::SetKeypadMode(KeypadMode::Numeric)),
        b'='  => m.ret(Seq::SetKeypadMode(KeypadMode::Application)),
        b'('  => parse_charset(m, CharsetIndex::G0),
        b')'  => parse_charset(m, CharsetIndex::G1),
        b'*'  => parse_charset(m, CharsetIndex::G2),
        b'+'  => parse_charset(m, CharsetIndex::G3),

        c     => m.err(Error::UnknownEscapeChar(c)),
    })
}

/// Attempts to parse a control sequence
fn parse_csi(m: Input<u8>) -> ParseResult<u8, Seq, Error> {
    take_till(m, |c| c >= 0x40 && c <= 0x7E).bind(|m, buf|
        any(m).bind(|m, c| match c {
            // Identify terminal
            b'c' => match buf.get(0) {
                Some(&b'>') => m.ret(Seq::SendSecondaryDeviceAttributes),
                _           => m.ret(Seq::SendPrimaryDeviceAttributes),
            },
            b'd' => m.from_result(parse_int(buf).map(|n| Seq::LinePositionAbsolute(n.unwrap_or(1)))),
            // TODO: is CursorDown ok for Line Position Relative?
            b'e' => m.from_result(parse_int(buf).map(|n| Seq::CursorDown(n.unwrap_or(1)))),
            // Mode set
            b'h' => match buf.get(0) {
                Some(&b'?') => m.from_result(parse_private_mode(&buf[1..]).map(|a| Seq::PrivateModeSet(a))),
                _           => m.from_result(parse_mode(buf).map(|a| Seq::ModeSet(a))),
            },
            // Mode unset
            b'l' => match buf.get(0) {
                Some(&b'?')  => m.from_result(parse_private_mode(&buf[1..]).map(|a| Seq::PrivateModeReset(a))),
                _            => m.from_result(parse_mode(buf).map(|a| Seq::ModeReset(a))),
            },
            // Color codes
            // Multiple control sequences for color codes can be present in the same sequence.

            // No parameters equals ``CSI 0 m`` which means Reset
            b'm' => if buf.len() == 0 {
                m.ret(Seq::CharAttr(vec![CharAttr::Reset]))
            } else {
                // TODO: Check for empty buf at end?
                m.from_result(parse_only(|i| sep_by1(i, parse_char_attr, |i| token(i, b';')), buf)
                              .map(|a| Seq::CharAttr(a))
                              .map_err(|_| Error::CharAttrError(buf.to_owned())))
            },
            b'n' => match parse_int(buf) {
                Ok(Some(6)) => m.ret(Seq::CursorPositionReport),
                _           => m.err(Error::UnknownCSI(b'n', From::from(buf))),
            },
            b'r' => m.from_result(parse_only(parser!{
                    let top = maybe(decimal);
                    let bot = maybe(parser!{token(b';'); decimal()});

                    ret @ _, Error: Seq::ScrollingRegion(top, bot)
                }, buf).map_err(|_| Error::UnknownCSI(b'r', buf.to_owned()))),
            b'A' => m.from_result(parse_int(buf).map(|n| Seq::CursorUp(n.unwrap_or(1)))),
            b'B' => m.from_result(parse_int(buf).map(|n| Seq::CursorDown(n.unwrap_or(1)))),
            b'C' => m.from_result(parse_int(buf).map(|n| Seq::CursorForward(n.unwrap_or(1)))),
            b'D' => m.from_result(parse_int(buf).map(|n| Seq::CursorBackward(n.unwrap_or(1)))),
            b'E' => m.from_result(parse_int(buf).map(|n| Seq::CursorNextLine(n.unwrap_or(1)))),
            b'F' => m.from_result(parse_int(buf).map(|n| Seq::CursorPreviousLine(n.unwrap_or(1)))),
            b'G' => m.from_result(parse_int(buf).map(|n| Seq::CursorHorizontalAbsolute(cmp::max(1, n.unwrap_or(1)) - 1))),
            b'H' => m.from_result(parse_only(parser!{
                    let row = option(decimal, 1);
                    let col = option(parser!{token(b';'); decimal()}, 1);

                    ret @ _, Error: Seq::CursorPosition(cmp::max(1, row) - 1, cmp::max(1, col) - 1)
                }, buf).map_err(|_| Error::UnknownCSI(b'H', buf.to_owned()))),
            b'I' => m.from_result(parse_int(buf).map(|n| Seq::CursorForwardTabulation(n.unwrap_or(1)))),
            b'J' => m.ret(match parse_int(buf) {
                Ok(Some(1)) => Seq::EraseInDisplay(EraseInDisplay::Above),
                Ok(Some(2)) => Seq::EraseInDisplay(EraseInDisplay::All),
                _           => Seq::EraseInDisplay(EraseInDisplay::Below),
            }),
            b'K' => m.ret(match parse_int(buf) {
                Ok(Some(1)) => Seq::EraseInLine(EraseInLine::Left),
                Ok(Some(2)) => Seq::EraseInLine(EraseInLine::All),
                _           => Seq::EraseInLine(EraseInLine::Right),
            }),
            b'L' => m.from_result(parse_int(buf).map(|n| Seq::InsertLines(cmp::max(1, n.unwrap_or(1))))),
            b'M' => m.from_result(parse_int(buf).map(|n| Seq::DeleteLines(cmp::max(1, n.unwrap_or(1))))),
            b'P' => m.from_result(parse_int(buf).map(|n| Seq::DeleteCharacter(n.unwrap_or(1)))),
            b'Z' => m.from_result(parse_int(buf).map(|n| Seq::CursorBackwardsTabulation(n.unwrap_or(1)))),
            c    => m.err(Error::UnknownCSI(c, From::from(buf))),
        })
    )
}

/// Attempts to parse an operating system command from the given buffer.
fn parse_osc(i: Input<u8>) -> ParseResult<u8, Seq, Error> {
    // ``ESC \`` = ``ST``
    // ie. 0x1B 0x5C => 0x07
    parse!{i;
        let buf = scan(0, |prev, c| if c == 0x07 || c == 0x5C && prev == 0x1B { None } else { Some(c) });
        // We have ST or \ left, get rid of it
        take(1);
        i -> {
            // Strip ESC if present
            let buf = if buf.last() == Some(&0x5C) { &buf[.. buf.len() - 2] } else { buf };

            match buf.first() {
                Some(&b'0') => i.ret(Seq::SetWindowTitle(String::from_utf8_lossy(&buf[2..]).into_owned())), // And icon name
                Some(&b'1') => i.ret(Seq::SetIconName(String::from_utf8_lossy(&buf[2..]).into_owned())),
                Some(&b'2') => i.ret(Seq::SetWindowTitle(String::from_utf8_lossy(&buf[2..]).into_owned())),
                Some(&b'3') => i.ret(Seq::SetXProps(String::from_utf8_lossy(&buf[2..]).into_owned())),
                Some(&b'4') => i.ret(Seq::SetColorNumber(String::from_utf8_lossy(&buf[2..]).into_owned())),
                _           => i.err(Error::UnknownOSC(From::from(buf))),
            }
        }
    }
}

fn parse_charset(m: Input<u8>, index: CharsetIndex) -> ParseResult<u8, Seq, Error> {
    any(m).bind(|m, c| match c {
        b'0' => m.ret(Seq::Charset(index, Charset::DECSpecialAndLineDrawing)),
        b'<' => m.ret(Seq::Charset(index, Charset::DECSupplementary)),
        b'>' => m.ret(Seq::Charset(index, Charset::DECTechnical)),
        b'A' => m.ret(Seq::Charset(index, Charset::UnitedKingdom)),
        b'B' => m.ret(Seq::Charset(index, Charset::UnitedStates)),
        b'4' => m.ret(Seq::Charset(index, Charset::Dutch)),
        b'C' => m.ret(Seq::Charset(index, Charset::Finnish)),
        b'5' => m.ret(Seq::Charset(index, Charset::Finnish)),
        b'R' => m.ret(Seq::Charset(index, Charset::French)),
        b'f' => m.ret(Seq::Charset(index, Charset::French)),
        b'Q' => m.ret(Seq::Charset(index, Charset::FrenchCanadian)),
        b'9' => m.ret(Seq::Charset(index, Charset::FrenchCanadian)),
        b'K' => m.ret(Seq::Charset(index, Charset::German)),
        b'Y' => m.ret(Seq::Charset(index, Charset::Italian)),
        b'`' => m.ret(Seq::Charset(index, Charset::NorwegianDanish)),
        b'E' => m.ret(Seq::Charset(index, Charset::NorwegianDanish)),
        b'6' => m.ret(Seq::Charset(index, Charset::NorwegianDanish)),
        b'Z' => m.ret(Seq::Charset(index, Charset::Spanish)),
        b'H' => m.ret(Seq::Charset(index, Charset::Swedish)),
        b'7' => m.ret(Seq::Charset(index, Charset::Swedish)),
        b'=' => m.ret(Seq::Charset(index, Charset::Swiss)),
        b'%' => any(m).bind(|m, c| match c {
            b'5' => m.ret(Seq::Charset(index, Charset::DECSupplementaryGraphics)),
            b'6' => m.ret(Seq::Charset(index, Charset::Portuguese)),
            c    => m.err(Error::UnknownCharset(b'%', Some(c))),
        }),
        c => m.err(Error::UnknownCharset(c, None)),
    })
}

/// Parses a single mode attribute.
///
/// Expects to receive data after the sequences ``ESC [`` but before ``h`` or ``l``.
fn parse_mode(buffer: &[u8]) -> Result<Vec<Mode>, Error> {
    use self::Mode::*;

    chomp::parse_only(|i| sep_by1(i, parser!{
        let n = decimal();

        i -> match n {
            2  => i.ret(KeyboardAction),
            4  => i.ret(Insert),
            12 => i.ret(SendReceive),
            20 => i.ret(AutomaticNewline),
            n  => i.err(Error::UnknownSetReset(n)),
        }
    }, |i| token(i, b';')), buffer).map_err(|_| Error::UnknownSetResetData(buffer.to_owned()))
}

/// Parses a single private mode attribute.
///
/// Expects to receive data after the sequences ``ESC [ ?`` but before ``h`` or ``l``.
fn parse_private_mode(buffer: &[u8]) -> Result<Vec<PrivateMode>, Error> {
    use self::PrivateMode::*;

    chomp::parse_only(|i| sep_by1(i, parser!{
        let n = decimal();

        i -> match n {
            1    => i.ret(ApplicationCursorKeys),
            5    => i.ret(LightScreen),
            7    => i.ret(Autowrap),
            8    => i.ret(Autorepeat),
            12   => i.ret(CursorBlink),
            25   => i.ret(ShowCursor),
            47   => i.ret(AlternateScreenBuffer),
            1000 => i.ret(MouseTrackingX11),
            1002 => i.ret(MouseTrackingCell),
            1005 => i.ret(MouseModeUtf8),
            1006 => i.ret(MouseModeSGR),
            1004 => i.ret(SendFocusEvents),
            1047 => i.ret(AlternateScreenBuffer),
            1048 => i.ret(SaveCursor),
            1049 => i.ret(SaveCursorAlternateBufferClear),
            n    => i.err(Error::UnknownPrivateSetReset(n)),
        }
    }, |i| token(i, b';')), buffer).map_err(|_| Error::UnknownPrivateSetResetData(buffer.to_owned()))
}

/// Parses a single character attribute
///
/// Expects to receive data after the sequence ``ESC [`` but before ``m``,
/// if a previous number has been read it expects to receive data after the following ``;``.
fn parse_char_attr(i: Input<u8>) -> U8Result<CharAttr> {
    use self::CharAttr::*;
    use self::CharType::*;
    use self::Color::*;

    fn next_num(i: Input<u8>) -> U8Result<u8> {
        token(i, b';').bind(|i, _| decimal(i))
    }

    decimal(i).bind(|i, n| match n {
        0        => i.ret(Reset),
        1        => i.ret(Set(Bold)),
        2        => i.ret(Set(Faint)),
        3        => i.ret(Set(Italicized)),
        4        => i.ret(Set(Underlined)),
        5        => i.ret(Set(Blink)),
        7        => i.ret(Set(Inverse)),
        8        => i.ret(Set(Invisible)),
        9        => i.ret(Set(CrossedOut)),
        21       => i.ret(Set(DoublyUnderlined)),
        22       => i.ret(Set(Normal)), /* Not bold, not faint */
        23       => i.ret(Unset(Italicized)),
        24       => i.ret(Unset(Underlined)),
        25       => i.ret(Unset(Blink)),
        27       => i.ret(Unset(Inverse)),
        28       => i.ret(Unset(Invisible)),
        29       => i.ret(Unset(CrossedOut)),
        30       => i.ret(FGColor(Black)),
        31       => i.ret(FGColor(Red)),
        32       => i.ret(FGColor(Green)),
        33       => i.ret(FGColor(Yellow)),
        34       => i.ret(FGColor(Blue)),
        35       => i.ret(FGColor(Magenta)),
        36       => i.ret(FGColor(Cyan)),
        37       => i.ret(FGColor(White)),
        39       => i.ret(FGColor(Default)),
        40       => i.ret(BGColor(Black)),
        41       => i.ret(BGColor(Red)),
        42       => i.ret(BGColor(Green)),
        43       => i.ret(BGColor(Yellow)),
        44       => i.ret(BGColor(Blue)),
        45       => i.ret(BGColor(Magenta)),
        46       => i.ret(BGColor(Cyan)),
        47       => i.ret(BGColor(White)),
        49       => i.ret(BGColor(Default)),
        38       => next_num(i).bind(|i, n| match n {
            2 => parse!{i;
                let r = next_num();
                let g = next_num();
                let b = next_num();
                ret FGColor(RGB(r, g, b))
            },
            5 => next_num(i).bind(|i, p| i.ret(FGColor(Palette(p)))),
            _ => i.err(chomp::Error::Unexpected),
        }),
        48       => next_num(i).bind(|i, n| match n {
            2 => parse!{i;
                let r = next_num();
                let g = next_num();
                let b = next_num();
                ret BGColor(RGB(r, g, b))
            },
            5 => next_num(i).bind(|i, p| i.ret(BGColor(Palette(p)))),
            _ => i.err(chomp::Error::Unexpected),
        }),
        90     => i.ret(FGColor(Palette(8))),  // Black intense
        91     => i.ret(FGColor(Palette(9))),  // Red intense
        92     => i.ret(FGColor(Palette(10))), // Green intense
        93     => i.ret(FGColor(Palette(11))), // Yellow intense
        94     => i.ret(FGColor(Palette(12))), // Blue intense
        95     => i.ret(FGColor(Palette(13))), // Magenta intense
        96     => i.ret(FGColor(Palette(14))), // Cyan intense
        97     => i.ret(FGColor(Palette(15))), // White intense
        99     => i.ret(BGColor(Default)),
        100    => i.ret(BGColor(Palette(8))),  // Black intense
        101    => i.ret(BGColor(Palette(9))),  // Red intense
        102    => i.ret(BGColor(Palette(10))), // Green intense
        103    => i.ret(BGColor(Palette(11))), // Yellow intense
        104    => i.ret(BGColor(Palette(12))), // Blue intense
        105    => i.ret(BGColor(Palette(13))), // Magenta intense
        106    => i.ret(BGColor(Palette(14))), // Cyan intense
        107    => i.ret(BGColor(Palette(15))), // White intense
        _ => i.err(chomp::Error::Unexpected),
    })
}

// TODO: Replace with proper int parser
fn parse_int(buf: &[u8]) -> Result<Option<usize>, Error> {
    if buf.is_empty() {
        return Ok(None);
    }

    unsafe {
        // Should be ok for numbers
        str::from_utf8_unchecked(buf)
    }.parse::<usize>()
     .map(|i| Some(i))
     .map_err(|e| Error::IntParseError(e))
}
