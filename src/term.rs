use tex;
use ctrl;
use glium;
use std::cmp;
use std::collections::HashMap;
use std::rc::Rc;

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

pub struct Term {
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

    pub fn pump<T>(&mut self, iter: T) where T: Iterator<Item=ctrl::Seq> {
        for i in iter {
            use ctrl::Seq::*;
            use ctrl::CharAttr::*;
            use ctrl::Color;

            self.dirty = true;

            match i {
                SetWindowTitle(_) => {},
                Unicode(c)                           => self.put_char(c as usize),
                CharAttr(list) => {
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
                    println!("Unknown seq: {:?}", i);
                },
            }
        }
    }
}

#[derive(Copy, Clone, Debug)]
struct ColoredVertex {
    /// Vertex coordinates [left, bottom]
    xy:  [f32; 2],
    /// Vertex color
    rgb: [f32; 3],
}

implement_vertex!(ColoredVertex, xy, rgb);

/// Structure used to render a Term instance onto a GL surface
pub struct GlTerm<'a, F, R>
  where F: 'a + glium::backend::Facade, R: 'a + tex::GlyphRenderer<u8> {
    context:   Rc<glium::backend::Context>,
    glyphs:    tex::GlyphMap<'a, F, R>,
    /// Vertex buffer for foreground text cells
    fg_buffer: Vec<tex::TexturedVertex>,
    /// Vertex buffer for background cells
    bg_buffer: Vec<ColoredVertex>,
    /// Shader for rendering foreground text
    fg_shader: glium::Program,
    /// Shader for rendering background vertices
    bg_shader: glium::Program,
    /// Cellsize is the pixel-size of a cell
    cellsize:  (f32, f32),
}

impl<'a, F, R> GlTerm<'a, F, R>
  where F: 'a + glium::backend::Facade, R: 'a + tex::GlyphRenderer<u8> {
    pub fn new(display: &'a F, glyph_renderer: R) -> Result<Self, glium::ProgramCreationError> {
        let fg_shader = try!(glium::Program::from_source(display,
            // vertex shader
            "   #version 410

                in vec2 xy;
                in vec3 rgb;
                in vec2 st;

                out vec3 pass_rgb;
                out vec2 pass_st;

                void main() {
                    gl_Position = vec4(xy, 0, 1);

                    pass_rgb = rgb;
                    pass_st  = st;
                }
            ",
            "   #version 410

                uniform sampler2D tex;

                in vec3 pass_rgb;
                in vec2 pass_st;

                out vec4 out_color;

                void main() {
                    out_color = vec4(pass_rgb, texture(tex, pass_st).r);
                }
            ",
            None
        ));
        let bg_shader = try!(glium::Program::from_source(display,
            // vertex shader
            "   #version 410

                in vec2 xy;
                in vec3 rgb;

                out vec3 pass_rgb;

                void main() {
                    gl_Position = vec4(xy, 0, 1);

                    pass_rgb = rgb;
                }
            ",
            "   #version 410

                in vec3 pass_rgb;

                out vec4 out_color;

                void main() {
                    out_color = vec4(pass_rgb, 1);
                }
            ",
            None
        ));

        let cellsize = glyph_renderer.glyph_size();

        Ok(GlTerm {
            context:   display.get_context().clone(),
            glyphs:    tex::GlyphMap::new(display, glyph_renderer),
            fg_buffer: Vec::new(),
            bg_buffer: Vec::new(),
            fg_shader: fg_shader,
            bg_shader: bg_shader,
            cellsize:  (cellsize.0 as f32, cellsize.1 as f32),
        })
    }

    fn load_glyphs(&mut self, t: &Term) {
        for r in t.data.iter() {
            for c in r.iter() {
                if c.glyph != 0 {
                    self.glyphs.load(c.glyph).unwrap();
                }
            }
        }
    }

    fn load_bg_vertices(&mut self, t: &Term, scale: (f32, f32), offset: (f32, f32)) {
        let cellsize = self.cellsize;

        self.bg_buffer.truncate(0);

        for (row, r) in t.data.iter().enumerate() {
            for (col, c) in r.iter().enumerate().filter(|&(_, c)| c.glyph != 0) {
                let left   = offset.0 + col as f32 * cellsize.0 * scale.0;
                let right  = left + cellsize.0 * scale.0;
                let bottom = offset.1 - (row + 1) as f32 * cellsize.1 * scale.1;
                let top    = bottom + cellsize.1 * scale.1;
                let rgb    = c.get_bg();

                self.bg_buffer.push(ColoredVertex { xy: [left,  bottom], rgb: rgb });
                self.bg_buffer.push(ColoredVertex { xy: [left,  top],    rgb: rgb });
                self.bg_buffer.push(ColoredVertex { xy: [right, top],    rgb: rgb });

                self.bg_buffer.push(ColoredVertex { xy: [right, top],    rgb: rgb });
                self.bg_buffer.push(ColoredVertex { xy: [right, bottom], rgb: rgb });
                self.bg_buffer.push(ColoredVertex { xy: [left,  bottom], rgb: rgb });
            }
        }
    }

    fn load_fg_vertices(&mut self, t: &Term, scale: (f32, f32), offset: (f32, f32)) {
        let cellsize = self.cellsize;

        self.fg_buffer.truncate(0);

        for (row, r) in t.data.iter().enumerate() {
            for (col, c) in r.iter().enumerate().filter(|&(_, c)| c.glyph != 0) {
                if let Some(g) = self.glyphs.get(c.glyph) {
                    let left     = offset.0 + col as f32 * cellsize.0 * scale.0;
                    let bottom   = offset.1 - (row + 1) as f32 * cellsize.1 * scale.1;
                    let charsize = (g.width as f32 * scale.0, g.height as f32 * scale.1);
                    let vs       = g.vertices((left, bottom), charsize, c.get_fg());

                    for v in vs.into_iter() {
                        self.fg_buffer.push(*v);
                    }
                }
            }
        }
    }

    /// Draws the terminal onto ``target``.
    /// 
    ///  * ``t`` is the terminal data to draw.
    ///  * ``fb_dim`` is the framebuffer dimensions in pixels which is needed to avoid blurry
    ///    text and/or stretching. 
    ///  * ``offset`` is the gl-offset to render at.
    pub fn draw<T>(&mut self, target: &mut T, t: &Term, fb_dim: (u32, u32), offset: (f32, f32))
      where T: glium::Surface {
        use glium::index;
        use glium::draw_parameters::BlendingFunction;
        use glium::draw_parameters::LinearBlendingFactor;

        let indices = index::NoIndices(index::PrimitiveType::TrianglesList);
        // This assumes that the framebuffer coordinates are [-1, -1] to [1, 1]
        let scale   = (2.0 / fb_dim.0 as f32, 2.0 / fb_dim.1 as f32);

        self.load_glyphs(t);

        self.load_bg_vertices(t, scale, offset);
        self.load_fg_vertices(t, scale, offset);

        let uniforms = uniform! {
            tex: glium::uniforms::Sampler::new(self.glyphs.texture())
                .magnify_filter(glium::uniforms::MagnifySamplerFilter::Nearest),
        };
        let params = glium::DrawParameters {
            blending_function: Some(BlendingFunction::Addition {
                source:      LinearBlendingFactor::SourceAlpha,
                destination: LinearBlendingFactor::OneMinusSourceAlpha,
            }),
            ..Default::default()
        };

        // TODO: Can this be reused?
        let bg_buffer = glium::VertexBuffer::new(&self.context, &self.bg_buffer).unwrap();
        let fg_buffer = glium::VertexBuffer::new(&self.context, &self.fg_buffer).unwrap();

        // TODO: Use proper background setting from terminal
        target.clear_color(1.0, 1.0, 1.0, 1.0);

        target.draw(&bg_buffer, &indices, &self.bg_shader, &uniforms, &params).unwrap();
        target.draw(&fg_buffer, &indices, &self.fg_shader, &uniforms, &params).unwrap();
    }
}
