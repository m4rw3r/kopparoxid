extern crate libc;
extern crate clock_ticks;
extern crate errno;
extern crate glutin;
#[macro_use]
extern crate glium;
extern crate freetype as ft;

use std::collections;
use std::cmp;

mod ctrl;
mod pty;
mod tex;

fn main() {
    use libc;
    use std::io;

    let (m, s) = pty::open().unwrap();

    match unsafe { libc::fork() } {
        -1  => panic!(io::Error::last_os_error()),
        0   => pty::run_sh(m, s),
        pid => {
            println!("master, child pid: {}", pid);

            window(m)
        }
    }
}

fn gl_ortho(left: f32, right: f32, bottom: f32, top: f32, near_val: f32, far_val: f32) -> [[f32; 4]; 4] {
    let r_l: f32 = right   - left;
    let t_b: f32 = top     - bottom;
    let f_n: f32 = far_val - near_val;

    [
        [ 2.0 / r_l, 0.0,         0.0,        -(right + left)       / r_l ],
        [ 0.0,       2.0 / t_b ,  0.0,        -(top   + bottom)     / t_b ],
        [ 0.0,       0.0,        -2.0 / f_n , -(far_val + near_val) / f_n ],
        [ 0.0,       0.0,         0.0,         1.0],
    ]
}

#[derive(Copy, Clone, Debug)]
struct TexturedVertex {
    xy:     [f32; 2],
    fg_rgb: [f32; 3],
    bg_rgb: [f32; 3],
    /* Texture coordinates: */
    st:     [f32; 2],
}

implement_vertex!(TexturedVertex, xy, fg_rgb, bg_rgb, st);

#[derive(Copy, Clone, Debug)]
struct Glyph {
    tex_area:  glium::Rect,
}

impl Glyph {
    fn vertices(&self, p: (f32, f32), s: (f32, f32), tex_size: (u32, u32), fg: [f32; 3], bg: [f32; 3]) -> [TexturedVertex; 6] {
        // vertex positions
        let l = p.0         as f32;
        let r = (p.0 + s.0) as f32;
        let b = p.1         as f32;
        let t = (p.1 + s.1) as f32;

        // texture positions
        let tl = (self.tex_area.left)                                   as f32 / tex_size.0 as f32;
        let tr = (self.tex_area.left + self.tex_area.width)                  as f32 / tex_size.0 as f32;
        let tb = (self.tex_area.bottom)                                  as f32 / tex_size.1 as f32;
        let tt = (self.tex_area.bottom + self.tex_area.height) as f32 / tex_size.1 as f32;

        [
            TexturedVertex { xy: [l, b], fg_rgb: fg, bg_rgb: bg, st: [tl, tt] },
            TexturedVertex { xy: [l, t], fg_rgb: fg, bg_rgb: bg, st: [tl, tb] },
            TexturedVertex { xy: [r, t], fg_rgb: fg, bg_rgb: bg, st: [tr, tb] },

            TexturedVertex { xy: [r, t], fg_rgb: fg, bg_rgb: bg, st: [tr, tb] },
            TexturedVertex { xy: [r, b], fg_rgb: fg, bg_rgb: bg, st: [tr, tt] },
            TexturedVertex { xy: [l, b], fg_rgb: fg, bg_rgb: bg, st: [tl, tt] },
        ]
    }
}

#[derive(Debug)]
struct GlyphMap<'a, F> where F: 'a + glium::backend::Facade {
    ft_face:  ft::Face<'a>,
    glyphs:   collections::BTreeMap<usize, Glyph>,
    atlas:    tex::Atlas<'a, F>
}

impl<'a, F> GlyphMap<'a, F> where F: 'a + glium::backend::Facade {
    fn new(display: &'a F, ft_face: ft::Face<'a>) -> Self {
        GlyphMap::new_with_size(display, ft_face, 1000)
    }

    fn new_with_size(display: &'a F, ft_face: ft::Face<'a>, atlas_size: u32) -> Self {
        GlyphMap {
            ft_face:  ft_face,
            glyphs:   collections::BTreeMap::new(),
            atlas:    tex::Atlas::new(display, atlas_size, atlas_size),
        }
    }

    fn load(&mut self, glyph: usize) -> Glyph {
        use std::borrow::Cow;
        use glium::texture;

        if let Some(g) = self.glyphs.get(&glyph) {
            return *g
        }

        self.ft_face.load_char(glyph, ft::face::RENDER).unwrap();

        let g = self.ft_face.glyph();
        let glyph_bitmap = g.bitmap();

        let height   = self.ft_face.size_metrics().unwrap().height;
        let ascender = self.ft_face.size_metrics().unwrap().ascender;

        let left   = cmp::max(0, g.bitmap_left());
        let top    = cmp::max(0, (ascender >> 6) as i32 - g.bitmap_top());
        let bottom = cmp::max(0, (height >> 6) as i32 - top - glyph_bitmap.rows());
        let right  = cmp::max(0, (g.advance().x >> 6) as i32 - g.bitmap_left() - glyph_bitmap.width());

        let r = self.atlas.add_with_padding(texture::RawImage2d{
            data:   Cow::Borrowed(glyph_bitmap.buffer()),
            width:  glyph_bitmap.width() as u32,
            height: glyph_bitmap.rows() as u32,
            format: texture::ClientFormat::U8
        }, (left as u32, top as u32, right as u32, bottom as u32));

        let g = Glyph {
            tex_area:  r,
        };

        self.glyphs.insert(glyph, g.clone());

        g
    }
    
    fn texture(&self) -> &glium::Texture2d {
        self.atlas.texture()
    }
    
    fn texture_size(&self) -> (u32, u32) {
        self.atlas.texture_size()
    }
}

fn window(mut m: pty::Fd) {
    use clock_ticks;
    use std::io;
    use std::process;
    use std::thread;
    use glium::DisplayBuild;
    use glium::index;
    use glium::Surface;

    m.set_noblock();

    let mut p   = ctrl::new_parser(io::BufReader::with_capacity(100, m));
    let display = glutin::WindowBuilder::new()
        .with_gl(glutin::GlRequest::Specific(glutin::Api::OpenGl, (3, 3)))
        .build_glium()
        .unwrap();

    let ft_lib = ft::Library::init().unwrap();
    let ft_face = ft_lib.new_face("./DejaVuSansMono/DejaVu Sans Mono for Powerline.ttf", 0).unwrap();

    ft_face.set_char_size(16 * 27, 0, 72 * 27, 0).unwrap();

    let mut m = GlyphMap::new(&display, ft_face);

    for c in "Rust".chars() {
        m.load(c as usize);
    }

    let mut vertices: Vec<TexturedVertex> = Vec::new();

    let r = m.load('R' as usize).vertices((-1.0, -1.0), (0.5, 2.0), m.texture_size(), [1.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
    let u = m.load('u' as usize).vertices((-0.5, -1.0), (0.5, 2.0), m.texture_size(), [1.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
    let s = m.load('s' as usize).vertices(( 0.0, -1.0), (0.5, 2.0), m.texture_size(), [1.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
    let t = m.load('t' as usize).vertices(( 0.5, -1.0), (0.5, 2.0), m.texture_size(), [1.0, 0.0, 0.0], [0.0, 0.0, 0.0]);

    for v in r.into_iter() {
        vertices.push(*v);
    }
    for v in u.into_iter() {
        vertices.push(*v);
    }
    for v in s.into_iter() {
        vertices.push(*v);
    }
    for v in t.into_iter() {
        vertices.push(*v);
    }

    let vertex_buffer = glium::VertexBuffer::new(&display, vertices);
    let indices = index::NoIndices(index::PrimitiveType::TrianglesList);
    let program = glium::Program::from_source(&display,
        // vertex shader
        "   #version 410

            in vec2 xy;
            in vec3 fg_rgb;
            in vec3 bg_rgb;
            in vec2 st;

            out vec3 pass_fg_rgb;
            out vec3 pass_bg_rgb;
            out vec2 pass_st;

            void main() {
                gl_Position = vec4(xy, 0, 1);

                pass_fg_rgb = fg_rgb;
                pass_bg_rgb = bg_rgb;
                pass_st     = st;
            }
        ",
        "   #version 410

            uniform sampler2D texture_diffuse;
            
            in vec3 pass_fg_rgb;
            in vec3 pass_bg_rgb;
            in vec2 pass_st;
            
            out vec4 out_color;

            void main() {
                float a = texture(texture_diffuse, pass_st).r;
                out_color = vec4(pass_bg_rgb, 1) * (1 - a) + vec4(pass_fg_rgb, 1) * a;
            }
        ",
        // optional geometry shader
        None
        ).unwrap();

    let uniforms = uniform! {
        matrix: [
            [ 1.0, 0.0, 0.0, 0.0 ],
            [ 0.0, 1.0, 0.0, 0.0 ],
            [ 0.0, 0.0, 1.0, 0.0 ],
            [ 0.0, 0.0, 0.0, 1.0 ],
        ],
        tex: m.texture(),
    };

    let params = glium::DrawParameters::new(&display);

    display.get_window().map(|w| w.set_title("rust_term"));

    unsafe { display.get_window().map(|w| w.make_current()); };

    let mut accumulator    = 0;
    let mut previous_clock = clock_ticks::precise_time_ns();
    let mut has_data       = false;
    let mut prev_win_size  = display.get_window().and_then(|w| w.get_inner_size());

    loop {
        let now = clock_ticks::precise_time_ns();
        accumulator += now - previous_clock;
        previous_clock = now;
        const FIXED_TIME_STAMP: u64 = 16666667;

        while accumulator >= FIXED_TIME_STAMP {
            accumulator -= FIXED_TIME_STAMP;

            loop {
                match p.next() {
                    Some(c) => {
                        has_data = true;

                        match c {
                            ctrl::Seq::SetWindowTitle(ref title) => {
                                display.get_window().map(|w| w.set_title(title));
                            },
                            _                                    =>{} // println!("> {:?}", c)
                        }
                    },
                    None    => break
                }
            }

            if has_data {
                let mut target = display.draw();
                target.clear_color(1.0, 1.0, 1.0, 1.0);
                target.draw(&vertex_buffer, &indices, &program, &uniforms, &params).unwrap();
                target.finish().unwrap();
            }

            for i in display.poll_events() {
                match i {
                    glutin::Event::Closed => process::exit(0),
                    _                     => {} // println!("w {:?}", i)
                }
            }

            let win_size  = display.get_window().and_then(|w| w.get_inner_size());

            // OS X does not fire glutin::Event::Resize from poll_events(), need to check manually
            if win_size != prev_win_size {
                prev_win_size = win_size;
                has_data      = true;
            }
            else {
                has_data = false;
            }
        }
        
        thread::sleep_ms(((FIXED_TIME_STAMP - accumulator) / 1000000) as u32);
    }
}
