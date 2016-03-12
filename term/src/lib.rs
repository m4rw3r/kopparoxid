#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate chomp;
#[macro_use]
extern crate log;

use std::io;

use std::io::Write;

pub mod ctrl;
pub mod color;

mod grid;

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

pub use char_mode::CharMode;

#[derive(Copy, Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Style {
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
    pub flags Mode: u32 {
        const NEW_LINE = 0b00000001,
        const INSERT   = 0b00000010,


        /// If the cursor should blink
        // TODO: Implement
        const BLINK       = 0b00000100,
        /// If the cursor should be visible
        // TODO: Implement
        const SHOW_CURSOR = 0b00001000,
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
        for c in self.grid.cells().filter(|c| c.0 != 0) {
            f(c.0, c.1.attrs)
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

        for ((row, col), c) in self.grid.cells().coords().filter(|&(_, c)| c.0 != 0) {
            f(&C{
                col:   col,
                row:   row,
                glyph: c.0,
                attrs: c.1.attrs,
                fg:    c.1.fg,
                bg:    c.1.bg,
            })
        }
    }
}

use grid::{Cursor, Grid, Movement};

#[derive(Debug)]
pub struct Term {
    /// Terminal cell grid
    grid:    Grid<(usize, Style)>,
    /// Output buffer
    out_buf: Vec<u8>,
    /// Style used to populate cells at cursor
    style:  Style,
    cursor: Cursor,
    /// Window title
    title:  String,
    /// Terminal mode
    mode:   Mode,
}

impl Term {
    pub fn new_with_size(width: usize, height: usize) -> Self {
        Term {
            grid:    Grid::new(width, height),
            out_buf: Vec::new(),
            cursor:  Cursor::default(),
            title:   String::new(),
            style:   Style::default(),
            mode:    Mode::default(),
        }
    }

    /// Resizes to (width, height)
    pub fn resize(&mut self, size: (usize, usize)) {
        if size != self.grid.size() {
            self.grid.resize(size.0, size.1);
        }

        // self.pos  = self.pos.limit_within(self.size);
        // TODO: Limit cursor to within, mainly for display purposes
    }

    fn put_char(&mut self, c: usize) {
        self.grid.put(&mut self.cursor, (c, self.style))
    }

    fn move_cursor<M: Movement>(&mut self, m: M) {
        self.grid.move_cursor(&mut self.cursor, m)
    }

    pub fn handle<W: Write>(&mut self, item: ctrl::Seq, mut out: W) -> io::Result<()> {
        use self::char_mode::*;

        use ctrl::Seq::*;
        use ctrl::CharAttr::*;
        use ctrl::CharType;
        use ctrl::PrivateMode;
        use ctrl::EraseInLine as EIL;
        use ctrl::EraseInDisplay as EID;

        use self::grid::Line::*;
        use self::grid::Column::*;
        use self::grid::Unbounded;

        match item {
            SetWindowTitle(title) => self.title = title,
            Unicode(c)        => self.put_char(c as usize),
            CharAttr(list)    => {
                for a in list {
                    match a {
                        Reset                       => self.style = Style::default(),
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
            EraseInDisplay(EID::Below)  => self.grid.erase_in_display_below(&self.cursor),
            EraseInLine(EIL::Right)     => self.grid.erase_in_line_right(&self.cursor),
            EraseInDisplay(EID::All)    => self.grid.erase_in_display_all(),
            CursorPosition(row, col)    => self.move_cursor((Line(row), Column(col))),
            CursorUp(n)                 => self.move_cursor(Up(n)),
            CursorDown(n)               => self.move_cursor(Down(n)),
            CursorForward(n)            => self.move_cursor(Right(n)),
            CursorBackward(n)           => self.move_cursor(Left(n)),
            CursorNextLine(n)           => self.move_cursor((Down(n), Column(0))),
            CursorPreviousLine(n)       => self.move_cursor((Up(n), Column(0))),
            LinePositionAbsolute(n)     => self.move_cursor(Line(n)),
            CursorHorizontalAbsolute(n) => self.move_cursor(Column(n)),
            CarriageReturn              => self.move_cursor(Column(0)),
            Backspace                   => self.move_cursor(Left(1)),
            Index                       => self.move_cursor(Unbounded(Down(1))),
            ReverseIndex                => self.move_cursor(Unbounded(Up(1))),
            NextLine                    => self.move_cursor((Unbounded(Down(1)), Column(0))),
            LineFeed                    => {
                // The reset state causes the interpretation of the line feed (LF), defined in ANSI Standard X3.4-1977, to imply only vertical movement of the active position and causes the RETURN key (CR) to send the single code CR. The set state causes the LF to imply movement to the first position of the following line and causes the RETURN key to send the two codes (CR, LF). This is the New Line (NL) option.
                if self.mode.contains(NEW_LINE) {
                    self.move_cursor((Unbounded(Down(1)), Column(0)));
                } else {
                    self.move_cursor(Unbounded(Down(1)));
                }
            },
            SendPrimaryDeviceAttributes => {
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
                return write!(out, "\x1B[?64;1;2;6;7;8;9;12;15;18;21;23;24;42;44;45;46c");
            },
            SendSecondaryDeviceAttributes => {
                // we pretend to be a VT525 here, version 2.0
                return write!(out, "\x1B[>65;20;1c");
            },
            CursorPositionReport => {
                // CSI [ line ; col R
                return write!(out, "\x1B[{};{}R", self.cursor.row() + 1, self.cursor.col() + 1);
            },
            ModeSet(modes) => {
                for m in modes {
                    match m {
                        _ => error!("Unknown mode (set): {:?}", m),
                    }
                }
            },
            ModeReset(modes) => {
                for m in modes {
                    match m {
                        _ => error!("Unknown mode (reset): {:?}", m),
                    }
                }
            },
            PrivateModeSet(modes) => {
                for m in modes {
                    match m {
                        PrivateMode::ShowCursor => self.mode.insert(SHOW_CURSOR),
                        _                       => error!("Unknown private mode (set): {:?}", m),
                    }
                }
            },
            PrivateModeReset(modes) => {
                for m in modes {
                    match m {
                        PrivateMode::ShowCursor => self.mode.remove(SHOW_CURSOR),
                        _                       => error!("Unknown private mode (reset): {:?}", m),
                    }
                }
            },
            _                                           => {
                error!("Unknown seq: {:?}", item);
            },
        }

        Ok(())

        // TODO: Propagate focus information
        // \x1B[I for focus in and \x1B[O for focus out
    }

    pub fn get_title(&self) -> &str {
        &self.title
    }
}
