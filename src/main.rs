extern crate libc;
extern crate clock_ticks;
extern crate errno;
extern crate glutin;
#[macro_use]
extern crate chomp;
#[macro_use]
extern crate glium;
extern crate freetype as ft;

mod ctrl;
mod parser;
mod pty;
mod term;
mod gl;

use gl::glyph;
use std::io;
use std::process;
use std::thread;
use term::color;

fn main() {

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

const FONT_SIZE: u32 = 16;

fn window(mut m: pty::Fd) {
    use gl::glyph::Renderer;
    use glium::DisplayBuild;

    m.set_noblock();

    let mut out = m.clone();
    let display = glutin::WindowBuilder::new()
        .with_gl(glutin::GlRequest::Specific(glutin::Api::OpenGl, (3, 3)))
        .build_glium()
        .unwrap();

    let ft_lib  = ft::Library::init().unwrap();
    let ft_face = ft_lib.new_face("./DejaVuSansMono/DejaVu Sans Mono for Powerline.ttf", 0).unwrap();

    ft_face.set_pixel_sizes(0, FONT_SIZE).unwrap();

    let glyph_renderer = glyph::FreeType::new(ft_face, glyph::FreeTypeMode::Greyscale);
    let cell           = glyph_renderer.cell_size();

    let mut t = term::Term::new_with_size((10, 10));
    let mut g = gl::term::GlTerm::new(&display, color::XtermDefault, glyph_renderer).unwrap();

    display.get_window().map(|w| w.set_title("Kopparoxid"));

    unsafe { display.get_window().map(|w| w.make_current()); };

    let mut accumulator    = 0;
    let mut previous_clock = clock_ticks::precise_time_ns();
    let mut prev_bufsize   = display.get_framebuffer_dimensions();

    t.resize(((prev_bufsize.0 / cell.0) as usize, (prev_bufsize.1 / cell.1) as usize));

    let mut buf = parser::Buffer::new(m, 2048, 4096);

    loop {
        let now = clock_ticks::precise_time_ns();
        accumulator += now - previous_clock;
        previous_clock = now;
        const FIXED_TIME_STAMP: u64 = 16666667;

        while accumulator >= FIXED_TIME_STAMP {
            accumulator -= FIXED_TIME_STAMP;

            t.pump(buf.iter(ctrl::parser)
                      .limit_bytes(100000)
                      // .inspect(|i| println!("{:?}", i))
                      .inspect(|i| if let &parser::IterResult::Error(ref err)   = i { println!("Error: {}", err); })
                      .inspect(|i| if let &parser::IterResult::IoError(ref err) = i { println!("IoError: {}", err); })
                      .filter_map(|i| i.data())
                      .inspect(|i| if let &ctrl::Seq::SetWindowTitle(ref title) = i {
                          display.get_window().map(|w| w.set_title(title));
                      }));

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

                t.resize(((buf_size.0 / cell.0) as usize, ((buf_size.1 / cell.1) as usize)));
            }

            if t.is_dirty() {
                let mut target = display.draw();

                g.draw(&mut target, &t, buf_size, (-1.0, 1.0));

                target.finish().unwrap();

                t.set_dirty(false);
            }
        }

        thread::sleep_ms(((FIXED_TIME_STAMP - accumulator) / 1000000) as u32);
    }
}
