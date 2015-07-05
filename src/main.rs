extern crate libc;
extern crate clock_ticks;
extern crate errno;
extern crate glutin;
#[macro_use]
extern crate log;
#[macro_use]
extern crate glium;
extern crate freetype as ft;

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

#[derive(Copy, Clone)]
struct TexturedVertex {
    xy:     [f32; 2],
    fg_rgb: [f32; 3],
    bg_rgb: [f32; 3],
    st:     [f32; 2],
}

implement_vertex!(TexturedVertex, xy, fg_rgb, bg_rgb, st);

fn window(mut m: pty::Fd) {
    use clock_ticks;
    use std::io;
    use std::process;
    use std::thread;
    use glium::DisplayBuild;
    use glium::index;
    use glium::texture;
    use glium::Surface;
    use std::borrow::Cow;
    
    m.set_noblock();
    
    println!("Starting");
    
    let mut p   = ctrl::new_parser(io::BufReader::with_capacity(100, m));
    let display = glutin::WindowBuilder::new()
        .with_gl(glutin::GlRequest::Specific(glutin::Api::OpenGl, (3, 3)))
        .build_glium()
        .unwrap();
    
    let ft_lib = ft::Library::init().unwrap();
    let ft_face = ft_lib.new_face("./DejaVuSansMono/DejaVu Sans Mono for Powerline.ttf", 0).unwrap();
    
    ft_face.set_char_size(16 * 27, 0, 72 * 27, 0).unwrap();
    
    let mut a = tex::Atlas::new(&display, 200, 100);
    
    for c in ['R', 'u', 's', 't'].into_iter() {
        ft_face.load_char(*c as usize, ft::face::RENDER).unwrap();
        
        let glyph_bitmap = ft_face.glyph().bitmap();
        let r = a.add(texture::RawImage2d{
            data:   Cow::Borrowed(glyph_bitmap.buffer()),
            width:  glyph_bitmap.width() as u32,
            height: glyph_bitmap.rows() as u32,
            format: texture::ClientFormat::U8
        });
        
        println!("{:?}: {:?}", c, r);
    }

    let vertex_buffer = glium::VertexBuffer::new(&display, vec![
        TexturedVertex { xy: [-0.5f32,  0.5f32], fg_rgb: [1f32, 0f32, 0f32], bg_rgb: [1.0, 0.0, 0.0], st: [0f32, 0f32] },
        TexturedVertex { xy: [-0.5f32, -0.5f32], fg_rgb: [0f32, 1f32, 0f32], bg_rgb: [1.0, 0.0, 0.0], st: [0f32, 1f32] },
        TexturedVertex { xy: [ 0.5f32, -0.5f32], fg_rgb: [0f32, 0f32, 1f32], bg_rgb: [1.0, 0.0, 0.0], st: [1f32, 1f32] },

        TexturedVertex { xy: [ 0.5f32, -0.5f32], fg_rgb: [0f32, 0f32, 1f32], bg_rgb: [1.0, 0.0, 0.0], st: [1f32, 1f32] },
        TexturedVertex { xy: [ 0.5f32,  0.5f32], fg_rgb: [1f32, 1f32, 1f32], bg_rgb: [1.0, 0.0, 0.0], st: [1f32, 0f32] },
        TexturedVertex { xy: [-0.5f32,  0.5f32], fg_rgb: [1f32, 0f32, 0f32], bg_rgb: [1.0, 0.0, 0.0], st: [0f32, 0f32] },
    ]);
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
        tex: a.texture(),
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
                            c                                    =>{} // println!("> {:?}", c)
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
