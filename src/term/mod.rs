use ctrl;
use std::cmp;

pub mod color;

#[derive(Copy, Clone, Default)]
pub struct Character {
    glyph: usize,
    fg:    ctrl::Color,
    bg:    ctrl::Color,
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
    fn glyphs<F>(&self, mut f: F) where F: Sized + FnMut(usize);
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
      where F: Sized + FnMut(usize) {
        for r in self.data.iter() {
            for c in r.iter().filter(|c| c.glyph != 0) {
                f(c.glyph)
            }
        }
    }

    fn cells<F>(&self, mut f: F)
      where F: Sized + FnMut(&Cell) {
        struct C {
            col:   usize,
            row:   usize,
            glyph: usize,
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
                self.fg
            }

            fn bg(&self) -> ctrl::Color {
                self.bg
            }
        }

        for (row, r) in self.data.iter().enumerate() {
            for (col, c) in r.iter().enumerate().filter(|&(_, c)| c.glyph != 0) {
                f(&C{
                    col:   col,
                    row:   row,
                    glyph: c.glyph,
                    fg:    c.fg,
                    bg:    c.bg,
                })
            }
        }
    }
}

pub struct Term {
    pub data:   Vec<Vec<Character>>,
    /// Terminal size, (columns, rows)
    size:   (usize, usize),
    /// Cursor position (column, row)
    pos:    (usize, usize),
    cur_fg: ctrl::Color,
    cur_bg: ctrl::Color,
    dirty:  bool,
}

impl Term {
    pub fn new() -> Self {
        Term::new_with_size((0, 0))
    }

    pub fn new_with_size(size: (usize, usize)) -> Self {
        let data: Vec<Vec<Character>> = (0..size.1).map(|_| (0..size.0).map(|_| Character::default()).collect()).collect();

        Term {
            size:   size,
            pos:    (0, 0),
            cur_fg: ctrl::Color::Default,
            cur_bg: ctrl::Color::Default,
            dirty:  false,
            data:   data,
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn set_dirty(&mut self, dirty: bool) {
        self.dirty = dirty
    }

    pub fn resize(&mut self, size: (usize, usize)) {
        let (cols, rows) = size;

        if size != self.size {
            self.dirty = true;
        }

        self.data.truncate(rows);

        for r in self.data.iter_mut() {
            r.truncate(cols);

            let size = r.len();

            r.extend((size..cols).map(|_| Character::default()));
        }

        let len = self.data.len();

        self.data.extend((len..rows).map(|_| (0..cols).map(|_| Character::default()).collect()));

        self.size = size;
        self.pos  = (cmp::min(self.size.0 - 1, self.pos.0), cmp::min(self.size.1 - 1, self.pos.1));

        println!("TERMSIZE: width: {}, height: {}", self.size.0, self.size.1);
    }

    fn set(&mut self, c: Character) {
        self.data[self.pos.1][self.pos.0] = c;
    }

    fn set_char(&mut self, c: usize) {
        let ch = Character {
            glyph: c,
            fg:    self.cur_fg,
            bg:    self.cur_bg,
        };

        self.set(ch)
    }

    fn put(&mut self, c: Character) {
        self.data[self.pos.1][self.pos.0] = c;

        self.set_pos_diff((1, 0));
    }

    fn put_char(&mut self, c: usize) {
        let ch = Character {
            glyph: c,
            fg:    self.cur_fg,
            bg:    self.cur_bg,
        };

        self.put(ch)
    }

    fn set_pos_diff(&mut self, (cols, lines): (i32, i32)) {
        self.pos = (cmp::max(0, self.pos.0 as i32 + cols) as usize, cmp::max(0, self.pos.1 as i32 + lines) as usize);

        if self.pos.0 >= self.size.0  {
            self.pos.1 = self.pos.1 + 1;
            self.pos.0 = 0;
        }

        if self.pos.1 >= self.size.1 {
            for i in 0..(self.size.1 - 1) {
                self.data.swap(i, i + 1);
            }

            for c in self.data[self.size.1 - 1].iter_mut() {
                c.glyph = 0;
            }

            self.pos.1 = self.size.1 - 1;
        }
    }

    fn set_pos(&mut self, col: usize, line: usize) {
        self.pos = (cmp::min(col, self.size.0), cmp::min(line, self.size.1))
    }

    fn set_pos_col(&mut self, col: usize) {
        self.pos = (cmp::min(col, self.size.0), self.pos.1)
    }

    fn set_fg(&mut self, fg: ctrl::Color) {
        self.cur_fg = fg;
    }

    fn set_bg(&mut self, bg: ctrl::Color) {
        self.cur_bg = bg;
    }

    fn erase_in_display_below(&mut self) {
        let line = self.pos.1;

        for r in self.data.iter_mut().skip(line) {
            for c in r.iter_mut() {
                c.glyph = 0;
            }
        }
    }

    fn erase_in_line_right(&mut self) {
        let (col, line) = self.pos;

        for c in self.data[line].iter_mut().skip(col) {
            c.glyph = 0;
        }
    }

    pub fn handle(&mut self, item: ctrl::Seq) {
        use ctrl::Seq::*;
        use ctrl::CharAttr::*;
        use ctrl::Color;

        self.dirty = true;

        match item {
            SetWindowTitle(_) => {},
            Unicode(c)        => self.put_char(c as usize),
            CharAttr(list)    => {
                for a in list {
                    match a {
                        Reset      => {
                            self.set_fg(Color::Default);
                            self.set_bg(Color::Default);
                        },
                        FGColor(c) => self.set_fg(c),
                        BGColor(c) => self.set_bg(c),
                        _          => {
                            println!("Unknown char attr: {:?}", a);
                        },
                    }
                }
            },
            EraseInDisplay(ctrl::EraseInDisplay::Below) => self.erase_in_display_below(),
            EraseInLine(ctrl::EraseInLine::Right)       => self.erase_in_line_right(),
            CursorPosition(row, col)                    => self.set_pos(col, row),
            CarriageReturn                              => self.set_pos_col(0),
            Backspace                                   => self.set_pos_diff((-1, 0)),
            LineFeed                                    => {
                self.set_pos_diff((0, 1));
                self.set_pos_col(0)
            },
            _                                           => {
                println!("Unknown seq: {:?}", item);
            },
        }
    }
}
