use std::io;
use std::fmt;
use std::cmp;
use std::str;
use std::result;

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
    StringTerminator,
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
    // Unicode,
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

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::IoError(err)
    }
}

pub type Result = result::Result<Seq, Error>;

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

pub struct Parser<'a, T: 'a + io::BufRead> {
    buffer: &'a mut T,
    used:   usize,
    state:  ParserState,
}

impl<'a, T: 'a + io::BufRead> Parser<'a, T> {
    pub fn new(buffer: &'a mut T) -> Self {
        Parser {
            buffer: buffer,
            used:   0,
            state:  ParserState::Default,
        }
    }
}

impl<'a, T: 'a + io::BufRead> Iterator for Parser<'a, T> {
    type Item = Result;

    fn next(&mut self) -> Option<Result> {
        // println!("used: {}", self.used);
        // TODO: Is there any way to actually make sure we consume it?
        self.buffer.consume(self.used);

        let buffer = match self.buffer.fill_buf() {
            Ok(buf)  => buf,
            Err(err) => return Some(Err(From::from(err))),
        };

        if buffer.is_empty() {
            return None;
        }

        // println!("{:?}", self.state);
        // println!("{:?}", String::from_utf8_lossy(From::from(buffer)));

        for (i, &c) in buffer.iter().enumerate() {
            // println!("s: {:?}", self.state);
            // Yields a value
            macro_rules! ret {
                ( $ret:expr ) => ({
                    let ret = $ret;

                    // self.buffer.consume(i + 1);
                    self.used  = i + 1;
                    self.state = ParserState::Default;

                    return Some(Ok(ret));
                })
            };

            // Yields an error
            macro_rules! err {
                ( $err:expr ) => ({
                    let ret = $err;

                    // self.buffer.consume(i + 1);
                    self.used  = i + 1;
                    self.state = ParserState::Default;

                    return Some(Err(ret));
                })
            };

            macro_rules! buf {
                ( $start:expr ) => (buffer[$start..i]);
            };

            match self.state {
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

                    0x1B => self.state = ParserState::ESC,

                    c if c > 127 => {
                        let tail = UTF8_TRAILING[c as usize];
                        let chr  = (c as u32) & (0xff >> (tail + 2));

                        self.state = ParserState::Unicode(chr, tail - 1)
                    },
                    c => ret!(Seq::Unicode(c as u32)),
                },
                ParserState::Unicode(chr, 0) => match c {
                    c if c > 127 => ret!(Seq::Unicode((chr << 6) + ((c as u32) & 0x3f))),
                    c => err!(Error::UnexpectedUTF8Byte(c))
                },
                ParserState::Unicode(chr, i) => match c {
                    c if c > 127 => self.state = ParserState::Unicode((chr << 6) + ((c as u32) & 0x3f), i - 1),
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
                    b'[' => self.state = ParserState::CSI(i + 1),
                    b'\\' => ret!(Seq::StringTerminator), /* ST */
                    b']' => self.state = ParserState::OSC(i + 1), /* OSC */
                    b'^' => ret!(Seq::PrivacyMessage), /* PM */
                    b'_' => ret!(Seq::ApplicationProgramCommand), /* APC */

                    b'>' => ret!(Seq::SetKeypadMode(KeypadMode::Numeric)),
                    b'=' => ret!(Seq::SetKeypadMode(KeypadMode::Application)),
                    b'(' => self.state = ParserState::Charset(CharsetIndex::G0),
                    b')' => self.state = ParserState::Charset(CharsetIndex::G1),
                    b'*' => self.state = ParserState::Charset(CharsetIndex::G2),
                    b'+' => self.state = ParserState::Charset(CharsetIndex::G3),

                    c => err!(Error::UnknownEscapeChar(c))
                    // Some(b" ") => match 
                },
                ParserState::CSI(start) => match c {
                    b'm' => self.state = {
                        // Color codes
                        // Multiple control sequences for color codes can be present in the
                        // same sequence.
                        use self::CharAttr::*;
                        use self::CharType::*;
                        use self::Color::*;

                        // No parameters equals CSI 0 m
                        // which means Reset
                        if buf!(start).is_empty() {
                            ret!(Seq::CharAttr(CharAttr::Reset));
                        }

                        let mut int_buf = Buffer::new(&buf!(start));

                        let r = match int_buf.next::<u8>() {
                            Some(0)              => Seq::CharAttr(Reset),
                            Some(1)              => Seq::CharAttr(Set(Bold)),
                            Some(2)              => Seq::CharAttr(Set(Faint)),
                            Some(3)              => Seq::CharAttr(Set(Italicized)),
                            Some(4)              => Seq::CharAttr(Set(Underlined)),
                            Some(5)              => Seq::CharAttr(Set(Blink)),
                            Some(7)              => Seq::CharAttr(Set(Inverse)),
                            Some(8)              => Seq::CharAttr(Set(Invisible)),
                            Some(9)              => Seq::CharAttr(Set(CrossedOut)),
                            Some(21)             => Seq::CharAttr(Set(DoublyUnderlined)),
                            Some(22)             => Seq::CharAttr(Set(Normal)), /* Not bold, not faint */
                            Some(23)             => Seq::CharAttr(Unset(Italicized)),
                            Some(24)             => Seq::CharAttr(Unset(Underlined)),
                            Some(25)             => Seq::CharAttr(Unset(Blink)),
                            Some(27)             => Seq::CharAttr(Unset(Inverse)),
                            Some(28)             => Seq::CharAttr(Unset(Invisible)),
                            Some(29)             => Seq::CharAttr(Unset(CrossedOut)),
                            Some(30) | Some(90)  => Seq::CharAttr(FGColor(Black)),
                            Some(31) | Some(91)  => Seq::CharAttr(FGColor(Red)),
                            Some(32) | Some(92)  => Seq::CharAttr(FGColor(Green)),
                            Some(33) | Some(93)  => Seq::CharAttr(FGColor(Yellow)),
                            Some(34) | Some(94)  => Seq::CharAttr(FGColor(Blue)),
                            Some(35) | Some(95)  => Seq::CharAttr(FGColor(Magenta)),
                            Some(36) | Some(96)  => Seq::CharAttr(FGColor(Cyan)),
                            Some(37) | Some(97)  => Seq::CharAttr(FGColor(White)),
                            Some(39) | Some(99)  => Seq::CharAttr(FGColor(Default)),
                            Some(40) | Some(100) => Seq::CharAttr(BGColor(Black)),
                            Some(41) | Some(101) => Seq::CharAttr(BGColor(Red)),
                            Some(42) | Some(102) => Seq::CharAttr(BGColor(Green)),
                            Some(43) | Some(103) => Seq::CharAttr(BGColor(Yellow)),
                            Some(44) | Some(104) => Seq::CharAttr(BGColor(Blue)),
                            Some(45) | Some(105) => Seq::CharAttr(BGColor(Magenta)),
                            Some(46) | Some(106) => Seq::CharAttr(BGColor(Cyan)),
                            Some(47) | Some(107) => Seq::CharAttr(BGColor(White)),
                            Some(49)             => Seq::CharAttr(BGColor(Default)),
                            Some(38)             => match int_buf.next::<u8>() {
                                Some(2) => match (int_buf.next::<u8>(), int_buf.next::<u8>(), int_buf.next::<u8>()) {
                                    (Some(r), Some(g), Some(b)) => Seq::CharAttr(FGColor(RGB(r, g, b))),
                                    _                           => err!(Error::ParseError(self.state, From::from(&buf!(start)))),
                                },
                                Some(5) => if let Some(p) = int_buf.next::<u8>() {
                                    Seq::CharAttr(FGColor(Palette(p)))
                                } else {
                                    err!(Error::ParseError(self.state, From::from(&buf!(start))))
                                },
                                _ => err!(Error::ParseError(self.state, From::from(&buf!(start)))),
                            },
                            Some(48) => match int_buf.next::<u8>() {
                                Some(2) => match (int_buf.next::<u8>(), int_buf.next::<u8>(), int_buf.next::<u8>()) {
                                    (Some(r), Some(g), Some(b)) => Seq::CharAttr(BGColor(RGB(r, g, b))),
                                    _                           => err!(Error::ParseError(self.state, From::from(&buf!(start)))),
                                },
                                Some(5) => if let Some(p) = int_buf.next::<u8>() {
                                    Seq::CharAttr(BGColor(Palette(p)))
                                } else {
                                    err!(Error::ParseError(self.state, From::from(&buf!(start))))
                                },
                                _ => err!(Error::ParseError(self.state, From::from(&buf!(start)))),
                            },
                            _ => err!(Error::ParseError(self.state, From::from(&buf!(start)))),
                        };

                        let used = start + int_buf.used();

                        if used >= i {
                            // We have used all the data in this color sequence,
                            // consume the "m" at the end and reset
                            self.used  = used + 1;
                            self.state = ParserState::Default;
                        } else {
                            // We still have additional color data to parse,
                            // leave the state in CSI but reset the buffer offset
                            // and consume the data we have parsed
                            self.used  = used;
                            self.state = ParserState::CSI(0);
                        };

                        return Some(Ok(r));
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
                        let mut int_buf = Buffer::new(&buf!(start));

                        let row = cmp::max(1, int_buf.next::<usize>().unwrap_or(1));
                        let col = cmp::max(1, int_buf.next::<usize>().unwrap_or(1));

                        ret!(Seq::CursorPosition(row - 1, col - 1));
                    },
                    c if c >= 0x40 && c <= 0x7E => err!(Error::ParseError(self.state, From::from(&buf!(start)))),
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
                    b'%' => self.state = ParserState::CharsetSuppOrPortuguese(index),
                    c    => err!(Error::UnknownCharset(c, None)),
                },
                ParserState::CharsetSuppOrPortuguese(index) => match c {
                    b'5' => ret!(Seq::Charset(index, Charset::DECSupplementaryGraphics)),
                    b'6' => ret!(Seq::Charset(index, Charset::Portuguese)),
                    c    => err!(Error::UnknownCharset(b'%', Some(c))),
                },
                ParserState::OSC(start) => {
                    // "ESC \" = "ST"
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

                    let mut b = Buffer::new(&buffer[start..end]);

                    // println!("{:?}", (buf!(start, end).get(0), buf!(start, end).get(1)));

                    match b.next::<u8>() {
                        Some(0) => ret!(Seq::SetWindowTitle(String::from_utf8_lossy(b.rest()).into_owned())), // And icon name
                        Some(1) => ret!(Seq::SetIconName(String::from_utf8_lossy(b.rest()).into_owned())),
                        Some(2) => ret!(Seq::SetWindowTitle(String::from_utf8_lossy(b.rest()).into_owned())),
                        Some(3) => ret!(Seq::SetXProps(String::from_utf8_lossy(b.rest()).into_owned())),
                        Some(4) => ret!(Seq::SetColorNumber(String::from_utf8_lossy(b.rest()).into_owned())),
                        _       => err!(Error::ParseError(self.state, From::from(&buffer[start..end]))),
                    }
                }
            }
        }

        None
    }
}

struct Buffer<'a> {
    buffer: &'a [u8],
    cursor: usize,
}

impl<'a> Buffer<'a> {
    fn new(buffer: &'a [u8]) -> Self {
        Buffer {
            buffer: buffer,
            cursor: 0,
        }
    }

    /// Yields the next integer from the buffer.
    fn next<T: str::FromStr>(&mut self) -> Option<T> {
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
    fn rest(&self) -> &[u8] {
        &self.buffer[self.cursor..]
    }

    /// The number of used bytes of the buffer, includes trailing ';'.
    fn used(&self) -> usize {
        self.cursor
    }
}

fn parse_int<T: str::FromStr>(buf: &[u8]) -> Option<T> {
    unsafe {
        // Should be ok for numbers
        str::from_utf8_unchecked(buf)
    }.parse::<T>().ok()
}
