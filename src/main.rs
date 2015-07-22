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
mod term;

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


const FONT_SIZE: isize = 18;
const DISPLAY_PPI: u32 = 72;

fn window(mut m: pty::Fd) {
    use clock_ticks;
    use std::io;
    use std::process;
    use std::thread;
    use glium::DisplayBuild;
    use glium::index;
    use glium::Surface;

    m.set_noblock();

    let mut out = m.clone();
    let mut p   = ctrl::new_parser(io::BufReader::with_capacity(100, m));
    let display = glutin::WindowBuilder::new()
        .with_gl(glutin::GlRequest::Specific(glutin::Api::OpenGl, (3, 3)))
        .build_glium()
        .unwrap();

    let ft_lib = ft::Library::init().unwrap();
    let ft_face = ft_lib.new_face("./DejaVuSansMono/DejaVu Sans Mono for Powerline.ttf", 0).unwrap();

    ft_face.set_char_size(FONT_SIZE << 6, 0, DISPLAY_PPI, 0).unwrap();

    let ft_metrics = ft_face.size_metrics().expect("Could not load size metrics from font face");
    let c_width    = (ft_metrics.max_advance >> 6) as u32 + 1;
    let c_height   = (ft_metrics.height >> 6) as u32 + 1;

    let mut t = term::Term::new_with_size(tex::GlyphMap::new(&display, ft_face), (10, 10));

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

    display.get_window().map(|w| w.set_title("RuSt Based openGL Virtual Terminal"));

    unsafe { display.get_window().map(|w| w.make_current()); };

    let mut accumulator    = 0;
    let mut previous_clock = clock_ticks::precise_time_ns();
    let mut prev_bufsize  = display.get_framebuffer_dimensions();

    t.resize(((prev_bufsize.0 / c_width) as usize, (prev_bufsize.1 / c_height) as usize));

    loop {
        let now = clock_ticks::precise_time_ns();
        accumulator += now - previous_clock;
        previous_clock = now;
        const FIXED_TIME_STAMP: u64 = 16666667;

        while accumulator >= FIXED_TIME_STAMP {
            accumulator -= FIXED_TIME_STAMP;
            
            // FIXME: Implement proper reusable parser instance
            // Should preferably allow more reuse of allocated memory
            let mut data = Vec::new();
            
            while let Some(i) = p.next() {
                match i {
                    Ok(c) => if let ctrl::Seq::SetWindowTitle(ref title) = c {
                        display.get_window().map(|w| w.set_title(title));
                    }
                    else {
                        data.push(c);
                    },
                    Err(err) => println!("{}", err),
                }
            }

            t.pump(data.into_iter());

            for i in display.poll_events() {
                match i {
                    glutin::Event::Closed               => process::exit(0),
                    glutin::Event::ReceivedCharacter(c) => {
                        use std::io::Write;
                        let mut s = String::with_capacity(c.len_utf8());

                        s.push(c);

                        out.write(s.as_ref()).unwrap();
                    },
                    glutin::Event::MouseMoved(_) => {},
                    _                                   => {} // println!("w {:?}", i)
                }
            }

            let buf_size = display.get_framebuffer_dimensions();

            // OS X does not fire glutin::Event::Resize from poll_events(), need to check manually
            if buf_size != prev_bufsize {
                prev_bufsize = buf_size;

                t.resize(((prev_bufsize.0 / c_width) as usize, ((prev_bufsize.1 / c_height) as usize)));
            }

            if t.is_dirty() {
                t.set_dirty(false);

                // TODO: Reuse?
                let vertices = t.vertices();
                let vertex_buffer = glium::VertexBuffer::new(&display, &vertices).unwrap();
                let uniforms = uniform! {
                    matrix: [
                        [ 1.0, 0.0, 0.0, 0.0 ],
                        [ 0.0, 1.0, 0.0, 0.0 ],
                        [ 0.0, 0.0, 1.0, 0.0 ],
                        [ 0.0, 0.0, 0.0, 1.0 ],
                    ],
                    tex: t.texture(),
                };

                let mut target = display.draw();
                target.clear_color(1.0, 1.0, 1.0, 1.0);
                target.draw(&vertex_buffer, &indices, &program, &uniforms, &params).unwrap();
                target.finish().unwrap();
                println!("{:?}", buf_size);
            }
        }

        thread::sleep_ms(((FIXED_TIME_STAMP - accumulator) / 1000000) as u32);
    }
}
