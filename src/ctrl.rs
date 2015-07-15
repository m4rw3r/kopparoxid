use std::io;
use std::str;
use std::fmt;
use std::result;

#[derive(Debug)]
pub enum Seq {
    /* Single character functions */
    Bell,
    Backspace,
    CarriageReturn,
    ReturnTerminalStatus,
    FormFeed,
    LineFeed,
    ShiftIn,
    ShiftOut,
    Tab,
    TabVertical,

    Delete,

    Unicode(u32),

    Index,
    NextLine,
    TabSet,
    ReverseIndex,
    SingleShiftSelectG2CharSet,
    SingleShiftSelectG3CharSet,
    DeviceControlString,
    StartOfGuardedArea,
    EndOfGuardedArea,
    StartOfString,
    ReturnTerminalId,
    ControlSequenceIntroducer,
    StringTerminator,
    OperatingSystemCommand,
    PrivacyMessage,
    ApplicationProgramCommand,

    Charset(CharsetIndex, Charset),

    SetKeypadMode(KeypadMode),

    /* CSI */
    CharAttr(CharAttr),
    EraseInLine(EraseInLine),
    EraseInDisplay(EraseInDisplay),
    /// Set cursor position, zero-indexed row-column
    CursorPosition(usize, usize),
    /* OSC */
    SetWindowTitle(String),
    SetIconName(String),
    SetXProps(String),
    SetColorNumber(String),
}

#[derive(Debug)]
pub enum KeypadMode {
    Numeric,
    Application,
}

#[derive(Debug)]
pub enum EraseInLine {
    Left,
    Right,
    All,
}

#[derive(Debug)]
pub enum EraseInDisplay {
    Above,
    Below,
    All,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum CharType {
    Normal,
    Bold,
    Faint,

    Italicized,
    Underlined,
    Blink,
    Inverse,
    Invisible,
    CrossedOut,
    DoublyUnderlined,
}

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub enum Color {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    Default,
    Palette(u8),
    RGB(u8, u8, u8)
}

impl Default for Color {
    fn default() -> Color {
        Color::Default
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum CharAttr {
    Reset,
    Set(CharType),
    Unset(CharType),
    FGColor(Color),
    BGColor(Color),
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Charset {
    DECSpecialAndLineDrawing,
    DECSupplementary,
    DECSupplementaryGraphics,
    DECTechnical,
    UnitedKingdom,
    UnitedStates,
    Dutch,
    Finnish,
    French,
    FrenchCanadian,
    German,
    Italian,
    NorwegianDanish,
    Portuguese,
    Spanish,
    Swedish,
    Swiss,
    Unicode,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum CharsetIndex {
    G0,
    G1,
    G2,
    G3,
}

#[derive(Debug)]
pub enum Error {
    ParseError(ParserState, Vec<u8>),
    UnknownCharset(u8, Option<u8>),
    UnknownEscapeChar(u8),
    UnexpectedUTF8Byte(u8),
    IoError(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::ParseError(state, ref data) => write!(f, "Unknown sequence found in state {:?}: {}", state, unsafe { String::from_utf8_unchecked(data.clone()) }),
            Error::UnknownCharset(c, None)     => write!(f, "Unknown charset sequence: {:?}", c),
            Error::UnknownCharset(c, Some(d))  => write!(f, "Unknown charset sequence: {:?} {:?}", c, d),
            Error::UnknownEscapeChar(c)        => write!(f, "Unknown escape character: {:?}", c),
            Error::UnexpectedUTF8Byte(b)       => write!(f, "Unexpected UTF8 byte: {:?}", b),
            Error::IoError(ref err)            => err.fmt(f),
        }
    }
}

pub type Result = result::Result<Seq, Error>;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ParserState {
    Default,
    ESC,
    CSI,
    OSC,
    CharAttr,
    Charset(CharsetIndex),
    Unicode(u32, u8),
}

pub struct Parser<T: io::Read> {
    src:   io::Bytes<T>,
    state: ParserState,
    buf:   Vec<u8>,
}

macro_rules! parse_osc (
    ( $me:ident ) => ({
        let r = if $me.buf.len() > 2 && $me.buf[1] == b';' {
            match $me.buf[0] {
                b'0' => Some(Seq::SetWindowTitle(String::from_utf8_lossy(&$me.buf[2..]).into_owned())), // And icon name
                b'1' => Some(Seq::SetIconName(String::from_utf8_lossy(&$me.buf[2..]).into_owned())),
                b'2' => Some(Seq::SetWindowTitle(String::from_utf8_lossy(&$me.buf[2..]).into_owned())),
                b'3' => Some(Seq::SetXProps(String::from_utf8_lossy(&$me.buf[2..]).into_owned())),
                b'4' => Some(Seq::SetColorNumber(String::from_utf8_lossy(&$me.buf[2..]).into_owned())),
                _    => None
            }
        }
        else {
            None
        };

        return match r {
            Some(x) => {
                $me.state = ParserState::Default;

                $me.buf.truncate(0);

                Some(Ok(x))
            },
            _ => {
                let r = Some(Err(Error::ParseError($me.state, $me.buf.clone())));

                $me.state = ParserState::Default;

                $me.buf.truncate(0);

                r
            }
        }
    })
);

/// Retrieves the next character from the internal iterator.
macro_rules! next_char (
    ( $me:ident ) => ( match $me.src.next() {
        Some(Ok(c))    => c,
        Some(Err(err)) => {
            return Some(Err(Error::IoError(err)));
        },
        None           => return None
    } )
);

/// Returns the next int (or None) from the buffer, separated by ';'.
/// 
/// Parses up to the next ';' or the end of the buffer (throwing away
/// the ';', leaving the reset present in the buffer.
macro_rules! buf_next_int (
    ( $me:ident, $typ:ty ) => (
        String::from_utf8(match $me.buf.iter().position(|c| *c == b';') {
            Some(p) => {
                let v = $me.buf[..p].to_vec();

                $me.buf = $me.buf[p+1..].to_vec();

                v
            },
            None => {
                let v = $me.buf.clone();

                $me.buf.truncate(0);

                v
            }
        })
        .ok()
        .map_or(None, |s| s.parse::<$typ>().ok())
    )
);

/// Prints a warning message that a control sequence introducer, attempts
/// to print the result as a unicode string if possible, otheriwse dumps
/// byte array.
/// 
/// Resets state and buffer.
macro_rules! unknown_csi (
    ( $me:ident ) => ({
        let r = Some(Err(Error::ParseError($me.state, $me.buf.clone())));

        $me.buf.truncate(0);

        $me.state = ParserState::Default;

        return r;
    });

    ( $me:ident, $buf:ident ) => ({
        let r = Some(Err(Error::ParseError($me.state, $buf)));

        $me.buf.truncate(0);

        $me.state = ParserState::Default;

        return r;
    });
);

/// Returns the given sequence entry and resets the parser state.
/// Does NOT reset the buffer.
macro_rules! return_reset (
    ( $me:ident, $ret:expr ) => ({
        $me.state = ParserState::Default;

        return Some(Ok($ret));
    })
);

static UTF8_TRAILING: [u8; 256] = [
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0, 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0, 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0, 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0, 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0, 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0, 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1, 1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,
    2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2, 3,3,3,3,3,3,3,3,4,4,4,4,5,5,5,5];

impl<T: io::Read> Iterator for Parser<T> {
    type Item = Result;

    fn next(&mut self) -> Option<Result> {
        loop {
            match self.state {
                ParserState::Default => match next_char!(self) {
                    0x07 => return Some(Ok(Seq::Bell)),
                    0x08 => return Some(Ok(Seq::Backspace)),
                    0x09 => return Some(Ok(Seq::Tab)),
                    0x0A => return Some(Ok(Seq::LineFeed)),
                    0x0B => return Some(Ok(Seq::TabVertical)),
                    0x0C => return Some(Ok(Seq::FormFeed)),
                    0x0D => return Some(Ok(Seq::CarriageReturn)),
                    0x0E => return Some(Ok(Seq::ShiftOut)),
                    0x0F => return Some(Ok(Seq::ShiftIn)),

                    0x1B => self.state = ParserState::ESC,

                    c if c > 127 => {
                        let tail = UTF8_TRAILING[c as usize];
                        let chr  = (c as u32) & (0xff >> (tail + 2));

                        self.state = ParserState::Unicode(chr, tail - 1)
                    },
                    c => return Some(Ok(Seq::Unicode(c as u32))),
                },
                ParserState::Unicode(chr, 0) => match next_char!(self) {
                    c if c > 127 => return_reset!(self, Seq::Unicode((chr << 6) + ((c as u32) & 0x3f))),
                    c => return Some(Err(Error::UnexpectedUTF8Byte(c)))
                },
                ParserState::Unicode(chr, i) => match next_char!(self) {
                    c if c > 127 => {
                        self.state = ParserState::Unicode((chr << 6) + ((c as u32) & 0x3f), i - 1);
                    },
                    c => return Some(Err(Error::UnexpectedUTF8Byte(c)))
                },
                ParserState::ESC => match next_char!(self) {
                    b'D' => return_reset!(self, Seq::Index), /* IND */
                    b'E' => return_reset!(self, Seq::NextLine), /* NEL */
                    b'H' => return_reset!(self, Seq::TabSet), /* HTS */
                    b'M' => return_reset!(self, Seq::ReverseIndex), /* RI */
                    b'N' => return_reset!(self, Seq::SingleShiftSelectG2CharSet), /* SS2 */
                    b'O' => return_reset!(self, Seq::SingleShiftSelectG3CharSet), /* SS3 */
                    b'P' => return_reset!(self, Seq::DeviceControlString), /* DCS */
                    b'V' => return_reset!(self, Seq::StartOfGuardedArea), /* SPA */
                    b'W' => return_reset!(self, Seq::EndOfGuardedArea), /* EPA */
                    b'X' => return_reset!(self, Seq::StartOfString), /* SOS */
                    b'Z' => return_reset!(self, Seq::ReturnTerminalId), /* DECID */
                    b'[' => self.state = ParserState::CSI, /* CSI */
                    b'\\' => return_reset!(self, Seq::StringTerminator), /* ST */
                    b']' => self.state = ParserState::OSC, /* OSC */
                    b'^' => return_reset!(self, Seq::PrivacyMessage), /* PM */
                    b'_' => return_reset!(self, Seq::ApplicationProgramCommand), /* APC */

                    b'>' => return_reset!(self, Seq::SetKeypadMode(KeypadMode::Numeric)),
                    b'=' => return_reset!(self, Seq::SetKeypadMode(KeypadMode::Application)),
                    b'(' => self.state = ParserState::Charset(CharsetIndex::G0),
                    b')' => self.state = ParserState::Charset(CharsetIndex::G1),
                    b'*' => self.state = ParserState::Charset(CharsetIndex::G2),
                    b'+' => self.state = ParserState::Charset(CharsetIndex::G3),

                    c => {
                        self.state = ParserState::Default;

                        return Some(Err(Error::UnknownEscapeChar(c)));
                    }
                    // Some(b" ") => match 
                },
                ParserState::CSI => match next_char!(self) {
                    b'm' => self.state = ParserState::CharAttr,
                    b'J' => {
                        let r = match buf_next_int!(self, u8) {
                            Some(1) => Some(Ok(Seq::EraseInDisplay(EraseInDisplay::Above))),
                            Some(2) => Some(Ok(Seq::EraseInDisplay(EraseInDisplay::All))),
                            _       => Some(Ok(Seq::EraseInDisplay(EraseInDisplay::Below))),
                        };

                        self.buf.truncate(0);

                        self.state = ParserState::Default;

                        return r;
                    },
                    b'K' => {
                        let r = match buf_next_int!(self, u8) {
                            Some(1) => Some(Ok(Seq::EraseInLine(EraseInLine::Left))),
                            Some(2) => Some(Ok(Seq::EraseInLine(EraseInLine::All))),
                            _       => Some(Ok(Seq::EraseInLine(EraseInLine::Right))),
                        };

                        self.buf.truncate(0);

                        self.state = ParserState::Default;

                        return r;
                    },
                    b'H' => {
                        let row = buf_next_int!(self, usize).unwrap_or(1);
                        let col = buf_next_int!(self, usize).unwrap_or(1);

                        self.buf.truncate(0);

                        self.state = ParserState::Default;

                        return Some(Ok(Seq::CursorPosition(row - 1, col - 1)));
                    },
                    c if c >= 0x40 && c <= 0x7E => {
                        self.buf.push(c);

                        unknown_csi!(self);
                    },
                    c => self.buf.push(c)
                },
                ParserState::CharAttr => {
                    // No parameters equals CSI 0 m
                    if self.buf.len() == 0 {
                        self.buf.push(b'0');
                    }

                    let mut cpy = self.buf.clone();

                    cpy.push(b'm');

                    let r = match buf_next_int!(self, u8) {
                        Some(0)              => Some(Ok(Seq::CharAttr(CharAttr::Reset))),
                        Some(1)              => Some(Ok(Seq::CharAttr(CharAttr::Set(CharType::Bold)))),
                        Some(2)              => Some(Ok(Seq::CharAttr(CharAttr::Set(CharType::Faint)))),
                        Some(3)              => Some(Ok(Seq::CharAttr(CharAttr::Set(CharType::Italicized)))),
                        Some(4)              => Some(Ok(Seq::CharAttr(CharAttr::Set(CharType::Underlined)))),
                        Some(5)              => Some(Ok(Seq::CharAttr(CharAttr::Set(CharType::Blink)))),
                        Some(7)              => Some(Ok(Seq::CharAttr(CharAttr::Set(CharType::Inverse)))),
                        Some(8)              => Some(Ok(Seq::CharAttr(CharAttr::Set(CharType::Invisible)))),
                        Some(9)              => Some(Ok(Seq::CharAttr(CharAttr::Set(CharType::CrossedOut)))),
                        Some(21)             => Some(Ok(Seq::CharAttr(CharAttr::Set(CharType::DoublyUnderlined)))),
                        Some(22)             => Some(Ok(Seq::CharAttr(CharAttr::Set(CharType::Normal)))), /* Not bold, not faint */
                        Some(23)             => Some(Ok(Seq::CharAttr(CharAttr::Unset(CharType::Italicized)))),
                        Some(24)             => Some(Ok(Seq::CharAttr(CharAttr::Unset(CharType::Underlined)))),
                        Some(25)             => Some(Ok(Seq::CharAttr(CharAttr::Unset(CharType::Blink)))),
                        Some(27)             => Some(Ok(Seq::CharAttr(CharAttr::Unset(CharType::Inverse)))),
                        Some(28)             => Some(Ok(Seq::CharAttr(CharAttr::Unset(CharType::Invisible)))),
                        Some(29)             => Some(Ok(Seq::CharAttr(CharAttr::Unset(CharType::CrossedOut)))),
                        Some(30) | Some(90)  => Some(Ok(Seq::CharAttr(CharAttr::FGColor(Color::Black)))),
                        Some(31) | Some(91)  => Some(Ok(Seq::CharAttr(CharAttr::FGColor(Color::Red)))),
                        Some(32) | Some(92)  => Some(Ok(Seq::CharAttr(CharAttr::FGColor(Color::Green)))),
                        Some(33) | Some(93)  => Some(Ok(Seq::CharAttr(CharAttr::FGColor(Color::Yellow)))),
                        Some(34) | Some(94)  => Some(Ok(Seq::CharAttr(CharAttr::FGColor(Color::Blue)))),
                        Some(35) | Some(95)  => Some(Ok(Seq::CharAttr(CharAttr::FGColor(Color::Magenta)))),
                        Some(36) | Some(96)  => Some(Ok(Seq::CharAttr(CharAttr::FGColor(Color::Cyan)))),
                        Some(37) | Some(97)  => Some(Ok(Seq::CharAttr(CharAttr::FGColor(Color::White)))),
                        Some(39) | Some(99)  => Some(Ok(Seq::CharAttr(CharAttr::FGColor(Color::Default)))),
                        Some(40) | Some(100) => Some(Ok(Seq::CharAttr(CharAttr::BGColor(Color::Black)))),
                        Some(41) | Some(101) => Some(Ok(Seq::CharAttr(CharAttr::BGColor(Color::Red)))),
                        Some(42) | Some(102) => Some(Ok(Seq::CharAttr(CharAttr::BGColor(Color::Green)))),
                        Some(43) | Some(103) => Some(Ok(Seq::CharAttr(CharAttr::BGColor(Color::Yellow)))),
                        Some(44) | Some(104) => Some(Ok(Seq::CharAttr(CharAttr::BGColor(Color::Blue)))),
                        Some(45) | Some(105) => Some(Ok(Seq::CharAttr(CharAttr::BGColor(Color::Magenta)))),
                        Some(46) | Some(106) => Some(Ok(Seq::CharAttr(CharAttr::BGColor(Color::Cyan)))),
                        Some(47) | Some(107) => Some(Ok(Seq::CharAttr(CharAttr::BGColor(Color::White)))),
                        Some(49)             => Some(Ok(Seq::CharAttr(CharAttr::BGColor(Color::Default)))),
                        Some(38)             => match (buf_next_int!(self, u8), buf_next_int!(self, u8), buf_next_int!(self, u8), buf_next_int!(self, u8)) {
                            (Some(2), Some(r), Some(g), Some(b)) => Some(Ok(Seq::CharAttr(CharAttr::FGColor(Color::RGB(r, g, b))))),
                            (Some(5), Some(p), None, None)       => Some(Ok(Seq::CharAttr(CharAttr::FGColor(Color::Palette(p))))),
                            _                                    => unknown_csi!(self, cpy)
                        },
                        Some(48)             => match (buf_next_int!(self, u8), buf_next_int!(self, u8), buf_next_int!(self, u8), buf_next_int!(self, u8)) {
                            (Some(2), Some(r), Some(g), Some(b)) => Some(Ok(Seq::CharAttr(CharAttr::BGColor(Color::RGB(r, g, b))))),
                            (Some(5), Some(p), None, None)       => Some(Ok(Seq::CharAttr(CharAttr::BGColor(Color::Palette(p))))),
                            _                                    => unknown_csi!(self, cpy)
                        },
                        _ => {
                            unknown_csi!(self, cpy);
                        }
                    };

                    if self.buf.len() == 0 {
                        self.state = ParserState::Default;
                    }

                    return r;
                },
                ParserState::Charset(index) => match next_char!(self) {
                    b'0' => return_reset!(self, Seq::Charset(index, Charset::DECSpecialAndLineDrawing)),
                    b'<' => return_reset!(self, Seq::Charset(index, Charset::DECSupplementary)),
                    b'>' => return_reset!(self, Seq::Charset(index, Charset::DECTechnical)),
                    b'A' => return_reset!(self, Seq::Charset(index, Charset::UnitedKingdom)),
                    b'B' => return_reset!(self, Seq::Charset(index, Charset::UnitedStates)),
                    b'4' => return_reset!(self, Seq::Charset(index, Charset::Dutch)),
                    b'C' => return_reset!(self, Seq::Charset(index, Charset::Finnish)),
                    b'5' => return_reset!(self, Seq::Charset(index, Charset::Finnish)),
                    b'R' => return_reset!(self, Seq::Charset(index, Charset::French)),
                    b'f' => return_reset!(self, Seq::Charset(index, Charset::French)),
                    b'Q' => return_reset!(self, Seq::Charset(index, Charset::FrenchCanadian)),
                    b'9' => return_reset!(self, Seq::Charset(index, Charset::FrenchCanadian)),
                    b'K' => return_reset!(self, Seq::Charset(index, Charset::German)),
                    b'Y' => return_reset!(self, Seq::Charset(index, Charset::Italian)),
                    b'`' => return_reset!(self, Seq::Charset(index, Charset::NorwegianDanish)),
                    b'E' => return_reset!(self, Seq::Charset(index, Charset::NorwegianDanish)),
                    b'6' => return_reset!(self, Seq::Charset(index, Charset::NorwegianDanish)),
                    b'Z' => return_reset!(self, Seq::Charset(index, Charset::Spanish)),
                    b'H' => return_reset!(self, Seq::Charset(index, Charset::Swedish)),
                    b'7' => return_reset!(self, Seq::Charset(index, Charset::Swedish)),
                    b'=' => return_reset!(self, Seq::Charset(index, Charset::Swiss)),
                    b'%' => match next_char!(self) {
                        b'5' => return_reset!(self, Seq::Charset(index, Charset::DECSupplementaryGraphics)),
                        b'6' => return_reset!(self, Seq::Charset(index, Charset::Portuguese)),
                        c    => {
                            self.state = ParserState::Default;

                            return Some(Err(Error::UnknownCharset(b'%', Some(c))));
                        }
                    },
                    c => {
                        self.state = ParserState::Default;

                        return Some(Err(Error::UnknownCharset(c, None)));
                    }
                },
                ParserState::OSC => match next_char!(self) {
                    0x5C if self.buf.iter().last() == Some(&0x1B) => {
                        /* Remove ESC */
                        self.buf.pop();

                        parse_osc!(self)
                    },
                    0x07 => parse_osc!(self),
                    c    => self.buf.push(c)
                }
            }
        }
    }
}

pub fn new_parser<T: io::Read>(r: T) -> Parser<T> {
    Parser{
        src:   r.bytes(),
        state: ParserState::Default,
        buf:   Vec::new(),
    }
}
