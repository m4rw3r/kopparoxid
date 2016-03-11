use std::cmp;
use std::fmt;

bitflags! {
    flags CursorState: u32 {
        // TODO: Implement
        const AUTOREPEAT  = 0b00000001,
        /// If set, cursor should automatically move to next line if moving past the end of line,
        /// if not set cursor should overwrite the current character.
        const AUTOWRAP    = 0b00000010,
        /// If to wrap on the next attempt to write at the end of line
        const WRAP_NEXT   = 0b00010000,
    }
}

impl Default for CursorState {
    fn default() -> Self {
        AUTOREPEAT | AUTOWRAP
    }
}

#[derive(Copy, Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Cursor {
    row:   usize,
    col:   usize,
    state: CursorState,
}

impl Cursor {
    pub fn row(&self) -> usize {
        self.row
    }

    pub fn col(&self) -> usize {
        self.col
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Grid<T: Copy + Default> {
    width:  usize,
    height: usize,
    /// Data stored in lines, columns
    data:   Vec<Vec<T>>,
}

impl<T: Copy + Default> Grid<T> {
    pub fn new(width: usize, height: usize) -> Self {
        let data: Vec<Vec<T>> = (0..height)
            .map(|_| (0..width)
                 .map(|_| Default::default()).collect())
            .collect();

        Grid {
            width:  width,
            height: height,
            data:   data,
        }
    }

    pub fn resize(&mut self, width: usize, height: usize) {
        self.data.truncate(height);

        for row in &mut self.data {
            row.truncate(width);

            let cols = row.len();

            row.extend((cols..width).map(|_| <T>::default()));
        }

        let len = self.data.len();

        self.data.extend((len..height).map(|_| (0..width).map(|_| Default::default()).collect()));

        self.width  = width;
        self.height = height;

        info!("Resized to: ({}, {})", width, height);
    }

    /// Returns width and height in cells
    pub fn size(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    /// Scrolls the grid `rows` downwards, clearing all new lines.
    pub fn scroll(&mut self, rows: isize) {
        if rows > 0 {
            // Saturate so we do nothing if we scroll more
            for i in 0..(self.height.saturating_sub(rows as usize)) {
                self.data.swap(i, i + rows as usize);
            }

            // Saturate to clear whole screen if more is scrolled
            for row in self.data[self.height.saturating_sub(rows as usize)..].iter_mut() {
                for c in row.iter_mut() {
                    *c = Default::default();
                }
            }
        }
        else if rows < 0 {
            let last_row = self.data.len() - 1;
            let keep     = cmp::min(-rows as usize, last_row);

            for i in (keep..last_row).rev() {
                self.data.swap(i - keep, i);
            }

            for row in self.data[..keep].iter_mut() {
                for c in row.iter_mut() {
                    *c = Default::default();
                }
            }
        }
    }

    pub fn put(&mut self, cursor: &mut Cursor, data: T) {
        // Recheck to make sure WRAP_NEXT still holds
        if cursor.state.contains(AUTOWRAP | WRAP_NEXT) && cursor.col + 1 >= self.width {
            // Move cursor row down one and scroll if needed
            if cursor.row + 1 >= self.height {
                self.scroll(1);
            } else {
                cursor.row += 1;
            }

            cursor.col = 0;
        }

        let row = cmp::min(cursor.row, self.height - 1);
        let col = cmp::min(cursor.col, self.width - 1);

        self.data[row][col] = data;

        if cursor.col + 1 >= self.width {
            cursor.state.insert(WRAP_NEXT);
        } else {
            cursor.col += 1;

            cursor.state.remove(WRAP_NEXT);
        }
    }

    pub fn move_cursor<M: Movement>(&mut self, cursor: &mut Cursor, direction: M) {
        info!("Moving cursor from (l: {}, r: {}): {:?}", cursor.row, cursor.col, direction);

        Movement::move_cursor(&direction, self, cursor)
    }

    // TODO: Move to trait
    pub fn erase_in_display_below(&mut self, c: &Cursor) {
        // Erase everything to the right of the current position
        self.erase_in_line_right(c);

        // Do not erase current line
        for r in self.data.iter_mut().skip(c.row + 1) {
            for c in r.iter_mut() {
                *c = Default::default();
            }
        }
    }

    // TODO: Move to trait
    pub fn erase_in_display_all(&mut self) {
        for r in self.data.iter_mut() {
            for c in r.iter_mut() {
                *c = Default::default();
            }
        }
    }

    // TODO: Move to trait
    pub fn erase_in_line_right(&mut self, c: &Cursor) {
        for c in self.data[cmp::min(c.row, self.height - 1)].iter_mut().skip(c.col) {
            *c = Default::default();
        }
    }

    pub fn cells(&self) -> Cells<T> {
        Cells(self, 0)
    }
}

pub struct Cells<'a, T: 'a + Copy + Default>(&'a Grid<T>, usize);

impl<'a, T: 'a + Copy + Default> Cells<'a, T> {
    pub fn coords(self) -> CellsWCoords<'a, T> {
        CellsWCoords(self.0, self.1)
    }
}

impl<'a, T: 'a + Copy + Default> Iterator for Cells<'a, T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        if self.1 >= self.0.height * self.0.width {
            None
        } else {
            let n = self.1;

            self.1 += 1;

            Some(self.0.data[n / self.0.width][n % self.0.width])
        }
    }
}

/// Iterator over the cells in a `Grid` yielding `((line, column), T)`.
pub struct CellsWCoords<'a, T: 'a + Copy + Default>(&'a Grid<T>, usize);

impl<'a, T: 'a + Copy + Default> Iterator for CellsWCoords<'a, T> {
    type Item = ((usize, usize), T);

    fn next(&mut self) -> Option<Self::Item> {
        if self.1 >= self.0.height * self.0.width {
            None
        } else {
            let line   = self.1 / self.0.width;
            let column = self.1 % self.0.width;

            self.1 += 1;

            Some(((line, column), self.0.data[line][column]))
        }
    }
}

pub trait Movement: fmt::Debug {
    fn move_cursor<T: Copy + Default>(&self, &mut Grid<T>, &mut Cursor);
}

impl<A: Movement, B: Movement> Movement for (A, B) {
    fn move_cursor<T: Copy + Default>(&self, g: &mut Grid<T>, c: &mut Cursor) {
        Movement::move_cursor(&self.0, g, c);
        Movement::move_cursor(&self.1, g, c);
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Line {
    /// Moves the cursor up one line, stopping at the top margin
    Up(usize),
    /// Moves the cursor down one line, stopping at the bottom margin
    Down(usize),
    /// Moves the cursor to the line n, if outside the grid it will be placed at the bottom margin
    Line(usize),
}

impl Movement for Line {
    fn move_cursor<T: Copy + Default>(&self, g: &mut Grid<T>, c: &mut Cursor) {
        match *self {
            Line::Up(n)   => c.row = c.row.saturating_sub(n),
            Line::Down(n) => c.row = cmp::min(c.row + n, g.height - 1),
            Line::Line(n) => c.row = cmp::min(n, g.height - 1),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Column {
    /// Moves the cursor n cells to the left, stopping at the left margin
    Left(usize),
    /// Moves the cursor n cells to the right, stopping at the right margin
    Right(usize),
    /// Moves the cursor to the column n, if outside the grid it will be placed at the right margin
    Column(usize),
}

impl Movement for Column {
    fn move_cursor<T: Copy + Default>(&self, g: &mut Grid<T>, c: &mut Cursor) {
        match *self {
            Column::Left(n)   => c.col = c.col.saturating_sub(n),
            Column::Right(n)  => c.col = cmp::min(c.col + n, g.width - 1),
            Column::Column(n) => c.col = cmp::min(n, g.width - 1),
        }

        c.state.remove(WRAP_NEXT);
    }
}

/// This wrapper causes scrolling to happen if a `Line` movement is outside of the grid.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Unbounded(pub Line);

impl Movement for Unbounded {
    fn move_cursor<T: Copy + Default>(&self, g: &mut Grid<T>, c: &mut Cursor) {
        use self::Line::*;

        match self.0 {
            Up(n)    => {
                g.scroll(cmp::min(0, c.row as isize - n as isize));

                c.row = c.row.saturating_sub(n);
            },
            Down(n)  => {
                let last_row = (g.height - 1) as isize;

                g.scroll(cmp::max(0, n as isize + c.row as isize - last_row));

                c.row = cmp::min(c.row + n, g.height - 1);
            },
            Line(n) => c.row = cmp::min(n, g.height - 1),
        }
    }
}
