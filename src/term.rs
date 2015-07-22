use tex;
use ctrl;
use glium;
use std::cmp;
use std::collections::HashMap;

#[derive(Copy, Clone, Default)]
struct Character {
    glyph: usize,
    fg:    ctrl::Color,
    bg:    ctrl::Color,
}

fn color(c: ctrl::Color, default: [f32; 3]) -> [f32; 3] {
    use ctrl::Color::*;

    match c {
        Black        => [0.0, 0.0, 0.0],
        Red          => [1.0, 0.0, 0.0],
        Green        => [0.0, 1.0, 0.0],
        Yellow       => [1.0, 1.0, 0.0],
        Blue         => [0.0, 0.0, 1.0],
        Magenta      => [1.0, 0.0, 1.0],
        Cyan         => [0.0, 1.0, 1.0],
        White        => [1.0, 1.0, 1.0],
        Default      => default,
        /* FIXME: Use color palette */
        Palette(_)   => default,
        RGB(r, g, b) => [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0],
    }
}

impl Character {
    fn get_fg(&self) -> [f32; 3] {
        color(self.fg, [1.0, 1.0, 1.0])
    }

    fn get_bg(&self) -> [f32; 3] {
        color(self.bg, [0.0, 0.0, 0.0])
    }
}

pub struct Term<'a, F> where F: 'a + glium::backend::Facade {
    glyphs: tex::GlyphMap<'a, F>,
    /// Terminal size, (columns, rows)
    size:   (usize, usize),
    /// Cursor position (column, row)
    pos:    (usize, usize),
    cur_fg: ctrl::Color,
    cur_bg: ctrl::Color,
    dirty:  bool,
    colors: HashMap<ctrl::Color, (u8, u8, u8)>,
    data:   Vec<Vec<Character>>,
}

impl<'a, F> Term<'a, F> where F: 'a + glium::backend::Facade {
    pub fn new(glyph_map: tex::GlyphMap<'a, F>) -> Self {
        Term::new_with_size(glyph_map, (0, 0))
    }

    pub fn new_with_size(glyph_map: tex::GlyphMap<'a, F>, size: (usize, usize)) -> Self {
        let data: Vec<Vec<Character>> = (0..size.1).map(|_| (0..size.0).map(|_| Character::default()).collect()).collect();

        Term {
            glyphs: glyph_map,
            size:   size,
            pos:    (0, 0),
            cur_fg: ctrl::Color::Default,
            cur_bg: ctrl::Color::Default,
            colors: HashMap::new(),
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

    pub fn texture(&self) -> &glium::Texture2d {
        self.glyphs.texture()
    }

    pub fn vertices(&mut self) -> Vec<tex::TexturedVertex> {
        for r in self.data.iter() {
            for c in r.iter() {
                if c.glyph != 0 {
                    self.glyphs.load(c.glyph).unwrap();
                }
            }
        }

        let w     = self.size.0 as f32 / 2.0;
        let h     = self.size.1 as f32 / 2.0;
        let tsize = self.glyphs.texture_size();

        let mut d = Vec::new();

        let glyph_map = &mut self.glyphs;

        for vs in self.data.iter().enumerate()
            .flat_map(|(i, r)|
                      r.iter().enumerate()
                          .filter(|&(_, c)| c.glyph != 0)
                          .filter_map(|(j, c)|
                                      glyph_map.get(c.glyph)
                                      .map(|l|
                                           l.vertices(((j as f32 - w) / w, (h - 1.0 - i as f32) / h), (1.0 / w, 1.0 / h), tsize, c.get_fg(), c.get_bg()))).collect::<Vec<[tex::TexturedVertex; 6]>>()) {
            for v in vs.iter() {
                d.push(*v);
            }
        }

        d
    }
    
    pub fn pump<T>(&mut self, iter: T) where T: Iterator<Item=ctrl::Seq> {
        for i in iter {
            use ctrl::Seq::*;
            use ctrl::CharAttr::*;
            use ctrl::Color;

            self.dirty = true;

            match i {
                SetWindowTitle(_) => {},
                Unicode(c)                           => self.put_char(c as usize),
                CharAttr(FGColor(c)) => self.set_fg(c),
                CharAttr(BGColor(c)) => self.set_bg(c),
                CharAttr(Reset)      => {
                    self.set_fg(Color::Default);
                    self.set_bg(Color::Default);
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
                _                                                      => println!("> {:?}", i)
            }
        }
    }
}
