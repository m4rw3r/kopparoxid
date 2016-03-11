use std::io;
use std::ptr;

use ctrl;
use util::Coord;

pub mod color;

pub mod char_mode {
    bitflags!{
        pub flags CharMode: u32 {
            const BOLD       = 0b00000001,
            const ITALIC     = 0b00000010,
            const INVERSE    = 0b00000100,
            const UNDERLINED = 0b00001000,

            const DEFAULT   = 0,
        }
    }

    impl Default for CharMode {
        fn default() -> Self {
            DEFAULT
        }
    }
}

pub use self::char_mode::CharMode;

#[derive(Copy, Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Character {
    glyph: usize,
    fg:    ctrl::Color,
    bg:    ctrl::Color,
    attrs: CharMode,
}

/// Describes a cell in the terminal
pub trait Cell {
    /// Returns the column for this cell, 0-indexed, from the left edge of the terminal.
    fn col(&self) -> usize;
    /// Returns the row for this cell, 0-indexed, from the top edge of the terminal.
    fn row(&self) -> usize;
    /// Returns the unicode glyph to draw in this cell.
    fn glyph(&self) -> usize;
    /// Returns the foreground color to use.
    fn fg(&self) -> ctrl::Color;
    /// Returns the backgroudn color to use.
    fn bg(&self) -> ctrl::Color;
    /// Returns the character attributes for this cell
    fn attrs(&self) -> CharMode;
}

bitflags!{
    pub flags PrivateMode: u32 {
        // TODO: Implement
        const APP_CURSOR_KEYS = 0b00000001,
        // TODO: Implement
        const AUTOREPEAT      = 0b00000010,
        const AUTOWRAP        = 0b00000100,
        // TODO: Implement
        const CURSOR_BLINK    = 0b00001000,
        // TODO: Implement
        const SHOW_CURSOR     = 0b00010000,
        // TODO: Implement
        const SAVE_CURSOR     = 0b00100000,
    }
}

impl Default for PrivateMode {
    fn default() -> Self {
        AUTOREPEAT | AUTOWRAP
    }
}

bitflags!{
    pub flags Mode: u32 {
        const NEW_LINE = 0b00000001,
        const INSERT   = 0b00000010,
    }
}

impl Default for Mode {
    fn default() -> Self {
        Mode::empty()
    }
}

/// Describes the visual area which is visible of the terminal contents.
pub trait Display {
    /// Iterates all glyphs used in the current displayed content and calls the provided
    /// closure for each glyph.
    ///
    /// NOTE: The closure may be called with the same glyph more than once.
    ///
    /// TODO: When iterators can properly be returned from functions, use an iterator instead of
    /// the closure.
    fn glyphs<F>(&self, mut f: F) where F: Sized + FnMut(usize, CharMode);
    /// Iterates all the cells to be displayed for the content and calls the provided closure
    /// for each cell.
    ///
    /// NOTE: Empty cells may be skipped
    ///
    /// TODO: When iterators can properly be returned from functions, use an iterator instead of
    /// the closure.
    fn cells<F>(&self, mut f: F) where F: Sized + FnMut(&Cell);
}

impl Display for Term {
    fn glyphs<F>(&self, mut f: F)
      where F: Sized + FnMut(usize, CharMode) {
        for r in self.data.iter() {
            for c in r.iter().filter(|c| c.glyph != 0) {
                f(c.glyph, c.attrs)
            }
        }
    }

    fn cells<F>(&self, mut f: F)
      where F: Sized + FnMut(&Cell) {
        use self::char_mode::*;

        struct C {
            col:   usize,
            row:   usize,
            glyph: usize,
            attrs: CharMode,
            fg:    ctrl::Color,
            bg:    ctrl::Color,
        }

        impl Cell for C {
            fn col(&self) -> usize {
                self.col
            }

            fn row(&self) -> usize {
                self.row
            }

            fn glyph(&self) -> usize {
                self.glyph
            }

            fn fg(&self) -> ctrl::Color {
                if self.attrs.contains(INVERSE) { self.bg } else { self.fg }
            }

            fn bg(&self) -> ctrl::Color {
                if self.attrs.contains(INVERSE) { self.fg } else { self.bg }
            }

            fn attrs(&self) -> CharMode {
                self.attrs
            }
        }

        for (row, r) in self.data.iter().enumerate() {
            for (col, c) in r.iter().enumerate().filter(|&(_, c)| c.glyph != 0) {
                f(&C{
                    col:   col,
                    row:   row,
                    glyph: c.glyph,
                    attrs: c.attrs,
                    fg:    c.fg,
                    bg:    c.bg,
                })
            }
        }
    }
}

#[derive(Debug)]
pub struct Term {
    pub data:   Vec<Vec<Character>>,
    /// Output buffer
    out_buf: Vec<u8>,
    /// Terminal size, (columns, rows), 1-indexed
    size:   Coord<usize>,
    /// Cursor position (column, row), 0-indexed
    pos:    Coord<usize>,
    style:  Character,
    pmode:  PrivateMode,
    mode:   Mode,
    dirty:  bool,
    wrap_next: bool,
}

impl Term {
    pub fn new() -> Self {
        Term::new_with_size(Coord { col: 0, row: 0 })
    }

    pub fn new_with_size(size: Coord<usize>) -> Self {
        let data: Vec<Vec<Character>> = (0..size.row).map(|_| (0..size.col).map(|_| Character::default()).collect()).collect();

        Term {
            size:    size,
            out_buf: Vec::new(),
            pos:     Coord::default(),
            style:   Character::default(),
            pmode:   PrivateMode::default(),
            mode:    Mode::default(),
            dirty:   false,
            wrap_next: false,
            data:    data,
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn set_dirty(&mut self, dirty: bool) {
        self.dirty = dirty
    }

    pub fn resize(&mut self, size: Coord<usize>) {
        if size != self.size {
            self.dirty = true;
        }

        self.data.truncate(size.row);

        for r in self.data.iter_mut() {
            r.truncate(size.col);

            let cols = r.len();

            r.extend((cols..size.col).map(|_| Character::default()));
        }

        let len = self.data.len();

        self.data.extend((len..size.row).map(|_| (0..size.col).map(|_| Character::default()).collect()));

        self.size = size;
        self.pos  = self.pos.limit_within(self.size);

        println!("TERMSIZE: width: {}, height: {}", self.size.col, self.size.row);
    }

    fn set(&mut self, c: Character) {
        self.data[self.pos.row][self.pos.col] = c;
    }

    fn set_char(&mut self, c: usize) {
        let ch = Character {
            glyph: c,
            ..self.style
        };

        self.set(ch)
    }

    fn put(&mut self, c: Character) {
        // Wrap if we decided we would do it last time and autowrap is on
        if self.pmode.contains(AUTOWRAP) && self.wrap_next {
            self.pos_diff_scroll(Coord { col: 0, row: 1 });
            self.pos.col = 0;
        }

        self.data[self.pos.row][self.pos.col] = c;

        // Wrap on next if we are at end
        if self.pos.col + 1 == self.size.col {
            self.wrap_next = true;
        }
        else {
            // Move within bounds
            self.pos.col  += 1;
            self.wrap_next = false;
        }
    }

    fn put_char(&mut self, c: usize) {
        let ch = Character {
            glyph: c,
            ..self.style
        };

        self.put(ch)
    }

    /// Sets position relative within window, no scrolling behavior.
    fn pos_diff(&mut self, diff: Coord<isize>) {
        let pos: Coord<usize> = (Into::<Coord<isize>>::into(self.pos) + diff).into();

        self.pos = pos.limit_within(self.size);
        self.wrap_next = false;
    }

    fn pos_diff_scroll(&mut self, diff: Coord<isize>) {
        let pos: Coord<isize> = (Into::<Coord<isize>>::into(self.pos) + diff).into();

        /*
        // TODO negative col?
        if pos.col >= self.size.col as isize {
            println!("Wrapping");
            pos.row = pos.row + 1;
            pos.col = pos.col % self.size.col as isize;
        }
        */

        if pos.row >= self.size.row as isize {
            for i in 0..(self.size.row - 1) {
                self.data.swap(i, i + 1);
            }

            for c in self.data[self.size.row - 1].iter_mut() {
                c.glyph = 0;
            }
        }
        else if pos.row < 0 {
            for i in (0..(self.size.row - 1)).rev() {
                self.data.swap(i + 1, i);
            }

            for c in self.data[0].iter_mut() {
                c.glyph = 0;
            }
        }

        self.pos       = Into::<Coord<usize>>::into(pos).limit_within(self.size);
        self.wrap_next = false;
    }

    fn set_pos(&mut self, pos: Coord<usize>) {
        self.pos       = pos.limit_within(self.size);
        self.wrap_next = false;
    }

    fn erase_in_display_below(&mut self) {
        // Erase everything to the right of the current position
        self.erase_in_line_right();

        // Do not erase current line
        for r in self.data.iter_mut().skip(self.pos.row + 1) {
            for c in r.iter_mut() {
                *c = Character::default();
            }
        }
    }

    fn erase_in_display_all(&mut self) {
        for r in self.data.iter_mut() {
            for c in r.iter_mut() {
                *c = Character::default();
            }
        }
    }

    fn erase_in_line_right(&mut self) {
        for c in self.data[self.pos.row].iter_mut().skip(self.pos.col) {
            *c = Character::default();
        }
    }

    pub fn handle(&mut self, item: ctrl::Seq) {
        use std::io::Write;
        use self::char_mode::*;

        use ctrl::Seq::*;
        use ctrl::CharAttr::*;
        use ctrl::CharType;
        use ctrl::PrivateMode;

        self.dirty = true;

        match item {
            // Already managed
            SetWindowTitle(_) => {},
            Unicode(c)        => self.put_char(c as usize),
            CharAttr(list)    => {
                for a in list {
                    match a {
                        Reset                       => self.style = Character::default(),
                        FGColor(c)                  => self.style.fg = c,
                        BGColor(c)                  => self.style.bg = c,
                        Set(CharType::Bold)         => self.style.attrs.insert(BOLD),
                        Set(CharType::Italicized)   => self.style.attrs.insert(ITALIC),
                        Set(CharType::Inverse)      => self.style.attrs.insert(INVERSE),
                        Set(CharType::Underlined)   => self.style.attrs.insert(UNDERLINED),
                        Unset(CharType::Bold)       => self.style.attrs.remove(BOLD),
                        Unset(CharType::Italicized) => self.style.attrs.remove(ITALIC),
                        Unset(CharType::Inverse)    => self.style.attrs.remove(INVERSE),
                        Unset(CharType::Underlined) => self.style.attrs.remove(UNDERLINED),
                        _                           => {
                            println!("Unknown char attr: {:?}", a);
                        },
                    }
                }
            },
            EraseInDisplay(ctrl::EraseInDisplay::Below) => self.erase_in_display_below(),
            EraseInLine(ctrl::EraseInLine::Right)       => self.erase_in_line_right(),
            CursorPosition(row, col)                    => self.set_pos(Coord { col: col, row: row }),
            CursorUp(n)                                 => self.pos_diff(Coord { row: -(n as isize), col: 0 }),
            CursorDown(n)                               => self.pos_diff(Coord { row: n as isize, col: 0 }),
            CursorForward(n)                            => self.pos_diff(Coord { row: 0, col: n as isize }),
            CursorBackward(n)                           => self.pos_diff(Coord { row: 0, col: -(n as isize) }),
            CarriageReturn                              => {
                // TODO: This is bad, move cursor scroll into something else
                self.pos.col   = 0;
                self.wrap_next = false;
            },
            ReverseIndex                                => self.pos_diff_scroll(Coord { row: -1, col: 0 }),
            Index                                       => self.pos_diff_scroll(Coord { row: 1, col: 0 }),
            Backspace                                   => self.pos_diff(Coord { col: -1, row: 0 }),
            NextLine                                    => self.pos_diff_scroll(Coord { col: 0, row: 1 }),
            LineFeed                                    => {
                self.pos_diff_scroll(Coord { col: 0, row: 1 });

                // The reset state causes the interpretation of the line feed (LF), defined in ANSI Standard X3.4-1977, to imply only vertical movement of the active position and causes the RETURN key (CR) to send the single code CR. The set state causes the LF to imply movement to the first position of the following line and causes the RETURN key to send the two codes (CR, LF). This is the New Line (NL) option.
                if self.mode.contains(NEW_LINE) {
                    self.pos.col = 0
                }
            },
            EraseInDisplay(ctrl::EraseInDisplay::All)   => self.erase_in_display_all(),
            SendPrimaryDeviceAttributes                 => {
                // CSI ? Pm c
                // where Pm = int separated by ;
                // we support an international terminal
                // 1  132 columns
                // 2  Printer port
                // 4  Sixel
                // 6  Selective erase
                // 7  Soft character set (DRCS)
                // 8  User-defined keys (UDKs)
                // 9  National replacement character sets (NRCS) (International terminal only)
                // 12 Yugoslavian (SCS)
                // 15 Technical character set
                // 18 Windowing capability
                // 21 Horizontal scrolling
                // 23 Greek
                // 24 Turkish
                // 42 ISO Latin-2 character set
                // 44 PCTerm
                // 45 Soft key map
                // 46 ASCII emulation
                write!(self.out_buf, "\x1B[?64;1;2;6;7;8;9;12;15;18;21;23;24;42;44;45;46c").unwrap();
            },
            SendSecondaryDeviceAttributes               => {
                // we pretend to be a VT525 here, version 2.0
                write!(self.out_buf, "\x1B[>65;20;1c").unwrap();
            },
            CursorPositionReport                        => {
                // CSI [ line ; col R
                write!(self.out_buf, "\x1B[{};{}R", self.pos.col + 1, self.pos.col + 1).unwrap()
            },
            ModeSet(modes) => {
                for m in modes {
                    match m {
                        _ => println!("Unknown mode (set): {:?}", m),
                    }
                }
            },
            ModeReset(modes) => {
                for m in modes {
                    match m {
                        _ => println!("Unknown mode (reset): {:?}", m),
                    }
                }
            },
            PrivateModeSet(modes) => {
                for m in modes {
                    match m {
                        PrivateMode::ShowCursor => self.pmode.insert(SHOW_CURSOR),
                        _                       => println!("Unknown private mode (set): {:?}", m),
                    }
                }
            },
            PrivateModeReset(modes) => {
                for m in modes {
                    match m {
                        PrivateMode::ShowCursor => self.pmode.remove(SHOW_CURSOR),
                        _                       => println!("Unknown private mode (reset): {:?}", m),
                    }
                }
            },
            _                                           => {
                println!("Unknown seq: {:?}", item);
            },
        }

        // TODO: Propagate focus information
        // \x1B[I for focus in and \x1B[O for focus out
    }

    pub fn write_output<W: io::Write>(&mut self, mut w: W) -> io::Result<usize> {
        w.write(&self.out_buf).map(|n| unsafe {
            debug_assert!(n <= self.out_buf.len());

            let new_len = self.out_buf.len() - n;
            let buf     = self.out_buf.as_mut_ptr();

            ptr::copy(buf.offset(n as isize), buf, new_len);

            self.out_buf.truncate(new_len);

            n
        })
    }

}
