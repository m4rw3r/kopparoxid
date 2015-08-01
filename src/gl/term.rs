use gl::glyph;
use glium;
use std::rc::Rc;
use term;

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
  where F: 'a + glium::backend::Facade, R: 'a + glyph::Renderer<u8> {
    context:   Rc<glium::backend::Context>,
    glyphs:    glyph::Map<'a, F, R>,
    /// Vertex buffer for foreground text cells
    fg_buffer: Vec<glyph::TexturedVertex>,
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
  where F: 'a + glium::backend::Facade, R: 'a + glyph::Renderer<u8> {
    pub fn new(display: &'a F, glyph_renderer: R) -> Result<Self, glium::ProgramCreationError> {
        use gl::glyph::Renderer;

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

        let cellsize = glyph_renderer.cell_size();

        Ok(GlTerm {
            context:   display.get_context().clone(),
            glyphs:    glyph::Map::new(display, glyph_renderer),
            fg_buffer: Vec::new(),
            bg_buffer: Vec::new(),
            fg_shader: fg_shader,
            bg_shader: bg_shader,
            cellsize:  (cellsize.0 as f32, cellsize.1 as f32),
        })
    }

    fn load_glyphs(&mut self, t: &term::Term) {
        for r in t.data.iter() {
            for c in r.iter() {
                if c.glyph != 0 {
                    self.glyphs.load(c.glyph).unwrap();
                }
            }
        }
    }

    fn load_bg_vertices(&mut self, t: &term::Term, scale: (f32, f32), offset: (f32, f32)) {
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

    fn load_fg_vertices(&mut self, t: &term::Term, scale: (f32, f32), offset: (f32, f32)) {
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
    pub fn draw<T>(&mut self, target: &mut T, t: &term::Term, fb_dim: (u32, u32), offset: (f32, f32))
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