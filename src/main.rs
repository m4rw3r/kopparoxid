extern crate libc;
extern crate clock_ticks;
extern crate errno;
extern crate glutin;
#[macro_use]
extern crate glium;
extern crate freetype as ft;

use std::collections;
use std::cmp;
use std::fmt;

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
        let l =  p.0        as f32;
        let r = (p.0 + s.0) as f32;
        let b =  p.1        as f32;
        let t = (p.1 + s.1) as f32;

        // texture positions
        let tl = (self.tex_area.left)                          as f32 / tex_size.0 as f32;
        let tr = (self.tex_area.left + self.tex_area.width)    as f32 / tex_size.0 as f32;
        let tb = (self.tex_area.bottom)                        as f32 / tex_size.1 as f32;
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
enum GlyphError {
    FtError(ft::Error),
    MissingGlyphMetrics(usize)
}

impl From<ft::Error> for GlyphError {
    fn from(err: ft::Error) -> GlyphError {
        GlyphError::FtError(err)
    }
}

impl fmt::Display for GlyphError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            GlyphError::FtError(err) => err.fmt(f),
            GlyphError::MissingGlyphMetrics(glyph) => write!(f, "glyph {} is missing metrics", glyph)
        }
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

    fn load(&mut self, glyph: usize) -> Result<(), GlyphError> {
        use std::borrow::Cow;
        use glium::texture;
        
        if self.glyphs.contains_key(&glyph) {
            return Ok(())
        }

        try!(self.ft_face.load_char(glyph, ft::face::RENDER));

        let g = self.ft_face.glyph();
        let glyph_bitmap = g.bitmap();

        let height   = try!(self.ft_face.size_metrics().ok_or(GlyphError::MissingGlyphMetrics(glyph))).height;
        let ascender = try!(self.ft_face.size_metrics().ok_or(GlyphError::MissingGlyphMetrics(glyph))).ascender;

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

        self.glyphs.insert(glyph, Glyph {
            tex_area:  r,
        });

        Ok(())
    }

    fn get(&self, glyph: usize) -> Option<Glyph> {
        self.glyphs.get(&glyph).map(|g| *g)
    }

    fn texture(&self) -> &glium::Texture2d {
        self.atlas.texture()
    }

    fn texture_size(&self) -> (u32, u32) {
        self.atlas.texture_size()
    }
}

#[derive(Copy, Clone, Default)]
struct Character {
    glyph: usize,
    fg:    ctrl::Color,
    bg:    ctrl::Color,
}

struct Term<'a, F> where F: 'a + glium::backend::Facade {
    glyphs: GlyphMap<'a, F>,
    size:   (usize, usize),
    pos:    (usize, usize),
    colors: collections::HashMap<ctrl::Color, (u8, u8, u8)>,
    data:   Vec<Vec<Character>>,
}

impl<'a, F> Term<'a, F> where F: 'a + glium::backend::Facade {
    fn new(glyph_map: GlyphMap<'a, F>) -> Self {
        Term::new_with_size(glyph_map, (0, 0))
    }

    fn new_with_size(glyph_map: GlyphMap<'a, F>, size: (usize, usize)) -> Self {
        let data: Vec<Vec<Character>> = (0..size.0).map(|_| (0..size.1).map(|_| Character::default()).collect()).collect();

        Term {
            glyphs: glyph_map,
            size:   size,
            pos:    (0, 0),
            colors: collections::HashMap::new(),
            data:   data,
        }
    }

    fn resize(&mut self, size: (usize, usize)) {
        let rows = size.0;
        let cols = size.1;

        self.data.truncate(rows);

        for r in self.data.iter_mut() {
            r.truncate(cols);

            let size = r.len();

            r.extend((size..cols).map(|_| Character::default()));
        }

        let len = self.data.len();

        self.data.extend((len..rows).map(|_| (0..cols).map(|_| Character::default()).collect()));

        self.size = size;
    }

    fn put(&mut self, c: Character) {
        if self.pos.1 >= self.size.1  {
            self.pos.0 = self.pos.0 + 1;
            self.pos.1 = 0;
        }

        if self.pos.0 >= self.size.0 {
            for i in (0..self.size.0 - 2) {
                self.data.swap(i, i + 1);
            }

            for c in self.data[self.size.0 - 1].iter_mut() {
                c.glyph = 0;
            }

            self.pos.0 = self.size.0 - 1;
        }
        
        println!("idx: {:?}", self.pos);

        self.data[self.pos.0][self.pos.1] = c;

        self.pos.1 = self.pos.1 + 1;
    }
    
    fn texture(&self) -> &glium::Texture2d {
        self.glyphs.texture()
    }

    fn vertices(&mut self) -> Vec<TexturedVertex> {
        for r in self.data.iter() {
            for c in r.iter() {
                if c.glyph != 0 {
                    self.glyphs.load(c.glyph).unwrap();
                }
            }
        }
            
        let h = self.size.0 as f32 / 2.0;
        let w = self.size.1 as f32 / 2.0;
        let h_offset: i32 = (self.size.1 / 2) as i32;
        let v_offset: i32 = (self.size.0 / 2) as i32;

        let tsize = self.glyphs.texture_size();

        let mut d = Vec::new();

        let glyph_map = &mut self.glyphs;

        for vs in self.data.iter().enumerate().flat_map(|(i, r)| r.iter().enumerate().filter(|&(_, c)| c.glyph != 0).filter_map(|(j, c)| glyph_map.get(c.glyph).map(|l| l.vertices((((j as i32) - h_offset) as f32 / w, (-(i as i32 + 1) + v_offset) as f32 / h), (1.0 / w, 1.0 / h), tsize, [1.0, 1.0, 1.0], [0.0, 0.0, 0.0]))).collect::<Vec<[TexturedVertex; 6]>>()) {
            for v in vs.iter() {
                d.push(*v);
            }
        }
        
        d
    }
}

const WIDTH: u32 = 18;
const HEIGHT: u32 = 24;

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

    let mut t = Term::new_with_size(GlyphMap::new(&display, ft_face), (10, 10));

    for c in "Rust is really awesome here!!!".chars() {
        t.put(Character{
           glyph: c as usize,
           fg:    ctrl::Color::Default,
           bg:    ctrl::Color::Default,
        });
    }

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

    let params = glium::DrawParameters::new(&display);

    display.get_window().map(|w| w.set_title("rust_term"));

    unsafe { display.get_window().map(|w| w.make_current()); };

    let mut accumulator    = 0;
    let mut previous_clock = clock_ticks::precise_time_ns();
    let mut has_data       = false;
    let mut prev_win_size  = display.get_window().and_then(|w| w.get_inner_size());
    
    match prev_win_size {
        Some((w, h)) => t.resize(((h / HEIGHT) as usize, (w / WIDTH) as usize)),
        None  => {}
    }

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

            for i in display.poll_events() {
                match i {
                    glutin::Event::Closed               => process::exit(0),
                    glutin::Event::ReceivedCharacter(c) => {
                        t.put(Character {
                            glyph: c as usize,
                            fg:    ctrl::Color::Default,
                            bg:    ctrl::Color::Default,
                        });

                        has_data = true;
                    },
                    _                                   => println!("w {:?}", i)
                }
            }

            let win_size = display.get_window().and_then(|w| w.get_inner_size());

            // OS X does not fire glutin::Event::Resize from poll_events(), need to check manually
            if win_size != prev_win_size {
                prev_win_size = win_size;
                has_data      = true;
                
                println!("Resize: {:?}", prev_win_size);

                match prev_win_size {
                    Some((w, h)) => {
                        println!("Asked for: {:?}", ((h / HEIGHT) as usize, (w / WIDTH) as usize));
                        t.resize(((h / HEIGHT) as usize, (w / WIDTH) as usize));
                    },
                    None  => {}
                }
                
                println!("Term: {:?}", t.size);
            }

            let vertex_buffer = glium::VertexBuffer::new(&display, t.vertices());
            let uniforms = uniform! {
                matrix: [
                    [ 1.0, 0.0, 0.0, 0.0 ],
                    [ 0.0, 1.0, 0.0, 0.0 ],
                    [ 0.0, 0.0, 1.0, 0.0 ],
                    [ 0.0, 0.0, 0.0, 1.0 ],
                ],
                tex: t.texture(),
            };

            if has_data {
                let mut target = display.draw();
                target.clear_color(1.0, 1.0, 1.0, 1.0);
                target.draw(&vertex_buffer, &indices, &program, &uniforms, &params).unwrap();
                target.finish().unwrap();
            }
            
            has_data = false;
        }
        
        thread::sleep_ms(((FIXED_TIME_STAMP - accumulator) / 1000000) as u32);
    }
}
