use gl::glyph;
use glium::backend::Context;
use glium;
use std::rc::Rc;

use term::{self, Cell, CharMode, Display};
use term::color::Manager;

#[derive(Copy, Clone, Debug)]
struct ColoredVertex {
    /// Vertex coordinates [left, bottom]
    xy:  [f32; 2],
    /// Vertex color
    rgb: [f32; 3],
}

implement_vertex!(ColoredVertex, xy, rgb);

#[derive(Debug, Clone, Copy, Hash, Ord, PartialOrd, Eq, PartialEq)]
pub enum FontStyle {
    Regular,
    Bold,
    Italic,
    BoldItalic,
}

impl From<CharMode> for FontStyle {
    fn from(m: CharMode) -> Self {
        use term::char_mode::*;

        match (m.contains(BOLD), m.contains(ITALIC)) {
            (true, true)   => FontStyle::BoldItalic,
            (true, false)  => FontStyle::Bold,
            (false, true)  => FontStyle::Italic,
            (false, false) => FontStyle::Regular,
        }
    }
}

pub type Renderer<'a> = glyph::Renderer<u8> + 'a;

/// Structure used to render a Term instance onto a GL surface
pub struct GlTerm<C: Manager> {
    /// OpenGl render context
    context:   Rc<glium::backend::Context>,
    /// Map and texture for storing rendered regular glyphs
    glyphs:    glyph::Map<FontStyle>,
    /// Vertex buffer for foreground text cells
    fg_buffer: Vec<glyph::TexturedVertex>,
    /// Vertex buffer for background cells
    bg_buffer: Vec<ColoredVertex>,
    /// Shader for rendering foreground text
    fg_shader: glium::Program,
    /// Shader for rendering background vertices
    bg_shader: glium::Program,
    /// Color code converter
    colors:    C,
    /// Cellsize is the pixel-size of a cell
    cellsize:  (f32, f32),
}

impl<C: Manager> GlTerm<C> {
    pub fn new(context:   Rc<Context>,
               colors:    C,
               glyph_map: glyph::Map<FontStyle>)
        -> Result<Self, glium::program::ProgramChooserCreationError> {
        let fg_shader = try!(program!(&context,
            410 => {
                outputs_srgb: true,
                vertex: "   #version 410

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
                fragment: "   #version 410

                    uniform sampler2D tex;

                    in vec3 pass_rgb;
                    in vec2 pass_st;

                    out vec4 out_color;

                    void main() {
                        out_color = vec4(pass_rgb, texture(tex, pass_st).r);
                    }
                ",
            },
        ));
        let bg_shader = try!(program!(&context,
            410 => {
                outputs_srgb: true,
                vertex: "   #version 410

                    in vec2 xy;
                    in vec3 rgb;

                    out vec3 pass_rgb;

                    void main() {
                        gl_Position = vec4(xy, 0, 1);

                        pass_rgb = rgb;
                    }
                ",
                fragment: "   #version 410

                    in vec3 pass_rgb;

                    out vec4 out_color;

                    void main() {
                        out_color = vec4(pass_rgb, 1);
                    }
                ",
            },
        ));

        let cellsize = glyph_map.cell_size();

        Ok(GlTerm {
            context:   context,
            glyphs:    glyph_map,
            fg_buffer: Vec::new(),
            bg_buffer: Vec::new(),
            fg_shader: fg_shader,
            bg_shader: bg_shader,
            colors:    colors,
            cellsize:  (cellsize.0 as f32, cellsize.1 as f32),
        })
    }

    #[inline]
    fn load_glyphs(&mut self, t: &term::Term) {
        t.glyphs(|g, m| {
            let t = m.into();

            self.glyphs.load(t, g).or_else(|e|
                if t != FontStyle::Regular {
                    info!("No font style {:?} found, dalling back to regular font", t);

                    self.glyphs.load(FontStyle::Regular, g)
                } else {
                    Err(e)
                }).unwrap()
        })
    }

    #[inline]
    fn get_glyph(&self, f: FontStyle, chr: usize) -> Option<glyph::Glyph> {
        self.glyphs.get(f, chr).or_else(|| self.glyphs.get(FontStyle::Regular, chr))
    }

    fn load_bg_vertices(&mut self, t: &term::Term, scale: (f32, f32), offset: (f32, f32)) {
        let cellsize = self.cellsize;

        self.bg_buffer.truncate(0);

        t.cells(|c| {
            let left   = offset.0 + c.col() as f32 * cellsize.0 * scale.0;
            let right  = left + cellsize.0 * scale.0;
            let bottom = offset.1 - (c.row() + 1) as f32 * cellsize.1 * scale.1;
            let top    = bottom + cellsize.1 * scale.1;
            let rgb    = self.colors.bg(c.bg());

            self.bg_buffer.push(ColoredVertex { xy: [left,  bottom], rgb: rgb });
            self.bg_buffer.push(ColoredVertex { xy: [left,  top],    rgb: rgb });
            self.bg_buffer.push(ColoredVertex { xy: [right, top],    rgb: rgb });

            self.bg_buffer.push(ColoredVertex { xy: [right, top],    rgb: rgb });
            self.bg_buffer.push(ColoredVertex { xy: [right, bottom], rgb: rgb });
            self.bg_buffer.push(ColoredVertex { xy: [left,  bottom], rgb: rgb });
        })
    }

    fn load_fg_vertices(&mut self, t: &term::Term, scale: (f32, f32), offset: (f32, f32)) {
        use term::char_mode::BOLD;

        let cellsize = self.cellsize;

        self.fg_buffer.truncate(0);

        t.cells(|c| {
            self.get_glyph(c.attrs().into(), c.glyph())
                .map(|g| {
                    // No bold mapping
                    // let fg       = self.colors.fg(c.fg());
                    // TODO: Configuration for bold => bright
                    let fg = self.colors.fg(if c.attrs().contains(BOLD) {
                        use term::ctrl::Color::*;

                        match c.fg() {
                            Black   => Palette(8),
                            Red     => Palette(9),
                            Green   => Palette(10),
                            Yellow  => Palette(11),
                            Blue    => Palette(12),
                            Magenta => Palette(13),
                            Cyan    => Palette(14),
                            White   => Palette(15),
                            fg      => fg
                        }
                    } else {
                        c.fg()
                    });

                    let left     = offset.0 + (c.col() as f32 * cellsize.0 + g.metrics.padding.left as f32) * scale.0;
                    let bottom   = offset.1 - ((c.row() + 1) as f32 * cellsize.1 - g.metrics.padding.bottom as f32) * scale.1;
                    let charsize = (g.metrics.width as f32 * scale.0, g.metrics.height as f32 * scale.1);

                    g.vertices((left, bottom), charsize, fg)
                }).map(|vs| {
                    for v in vs.into_iter() {
                        self.fg_buffer.push(*v);
                    }
                });
        })
    }

    /// Draws the terminal onto ``target``.
    ///
    ///  * ``t`` is the terminal data to draw.
    ///  * ``fb_dim`` is the framebuffer dimensions in pixels which is needed to avoid blurry
    ///    text and/or stretching.
    ///  * ``offset`` is the gl-offset to render at.
    pub fn draw<T>(&mut self, target: &mut T, t: &term::Term, fb_dim: (u32, u32), offset: (f32, f32))
      where T: glium::Surface {
        use glium::index;
        use glium::draw_parameters::Blend;
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
            blend: Blend {
                color: BlendingFunction::Addition {
                    source:      LinearBlendingFactor::SourceAlpha,
                    destination: LinearBlendingFactor::OneMinusSourceAlpha,
                },
                ..Default::default()
            },
            ..Default::default()
        };

        // TODO: Can this be reused?
        let bg_buffer = glium::VertexBuffer::new(&self.context, &self.bg_buffer).unwrap();
        let fg_buffer = glium::VertexBuffer::new(&self.context, &self.fg_buffer).unwrap();

        let rgb = self.colors.fill();
        let r   = rgb[0];
        let g   = rgb[1];
        let b   = rgb[2];

        target.clear_color_srgb(r, g, b, 1.0);

        target.draw(&bg_buffer, &indices, &self.bg_shader, &uniforms, &params).unwrap();
        target.draw(&fg_buffer, &indices, &self.fg_shader, &uniforms, &params).unwrap();
    }

    pub fn cell_size(&self) -> (u32, u32) {
        self.glyphs.cell_size()
    }
}
