#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate chomp;
extern crate errno;
extern crate freetype as ft;
#[macro_use]
extern crate glium;
extern crate glutin;
extern crate libc;
extern crate num;
extern crate time;

mod ctrl;
mod pty;
mod term;
mod gl;
mod util;

use std::io;
use std::process;
use std::thread;
use std::time::Duration;

use chomp::buffer::StreamError;
use chomp::buffer::Source;
use chomp::buffer::Stream;

use gl::glyph;
use term::color;
use util::Coord;

fn main() {
    let (mut m, s) = pty::open().unwrap();

    match unsafe { libc::fork() } {
        -1  => panic!(io::Error::last_os_error()),
        0   => pty::run_sh(m, s),
        pid => {
            println!("master, child pid: {}", pid);

            // TODO: How to set the buffer size of the pipe here?
            m.set_noblock();

            window(m, pid)
        }
    }
}

const FONT_SIZE: f32 = 16.0;

fn load_font(f: &mut ft::Library, path: &str, size: u32) -> Box<glyph::Renderer<u8>> {
    let ft_face = f.new_face(path, 0).unwrap();

    ft_face.set_pixel_sizes(0, size).unwrap();

    // TODO: Antialiasing settings
    Box::new(glyph::FreeType::new(ft_face, glyph::FreeTypeMode::Greyscale))
}

fn window(m: pty::Fd, child_pid: libc::c_int) {
    use gl::glyph::Renderer;
    use glium::DisplayBuild;
    use gl::term::FontStyle;

    let mut out = m.clone();
    let display = glutin::WindowBuilder::new()
        .with_gl(glutin::GlRequest::Specific(glutin::Api::OpenGl, (3, 3)))
        .with_srgb(Some(true))
        .build_glium()
        .unwrap();

    let scale = display.get_window().map(|w| w.hidpi_factor()).unwrap_or(1.0);

    let faces = [
        (FontStyle::Regular,    "./DejaVuSansMono/DejaVu Sans Mono for Powerline.ttf"),
        (FontStyle::Bold,       "./DejaVuSansMono/DejaVu Sans Mono Bold for Powerline.ttf"),
        (FontStyle::Italic,     "./DejaVuSansMono/DejaVu Sans Mono Oblique for Powerline.ttf"),
        (FontStyle::BoldItalic, "./DejaVuSansMono/DejaVu Sans Mono Bold Oblique for Powerline.ttf"),
    ];

    let mut ft_lib = ft::Library::init().unwrap();
    let mut f_map  = glyph::Map::new(&display);

    for &(f, t) in &faces {
        f_map.add_renderer(f, load_font(&mut ft_lib, t, (FONT_SIZE * scale) as u32)).unwrap();
    }

    let cell = f_map.cell_size();

    let mut t = term::Term::new_with_size(Coord { col: 10, row: 10});
    let mut g = gl::term::GlTerm::new(&display, color::XtermDefault, f_map).unwrap();

    display.get_window().map(|w| w.set_title("Kopparoxid"));

    unsafe { display.get_window().map(|w| w.make_current()); };

    let mut accumulator    = 0;
    let mut previous_clock = time::precise_time_ns();
    let mut prev_bufsize   = display.get_framebuffer_dimensions();

    let tsize = Coord {
        col: (prev_bufsize.0 / cell.0) as usize,
        row: (prev_bufsize.1 / cell.1) as usize,
    };

    // Merge with set window size? (in separate function/method)
    t.resize(tsize);

    // TODO: Merge with SIGWINCH
    out.set_window_size(tsize, Coord {
        col: prev_bufsize.0 as usize,
        row: prev_bufsize.1 as usize,
    }).unwrap();

    // Message all processes in the child process group
    match unsafe { libc::kill(-child_pid, libc::SIGWINCH) } {
        -1 => panic!("kill(child, SIGWINCH) failed: {:?}", io::Error::last_os_error()),
        _  => {},
    }

    let mut buf = Source::from_read(m, chomp::buffer::FixedSizeBuffer::new());

    buf.set_autofill(false);

    loop {
        let now = time::precise_time_ns();
        accumulator += now - previous_clock;
        previous_clock = now;
        const FIXED_TIME_STAMP: u64 = 16666667;

        while accumulator >= FIXED_TIME_STAMP {
            accumulator -= FIXED_TIME_STAMP;

            buf.fill().unwrap();

            loop {
                match buf.parse(ctrl::parser) {
                    Ok(s) => {
                        /*if let ctrl::Seq::CharAttr(_) = s {}
                        else if let ctrl::Seq::PrivateModeSet(_) = s {}
                        else if let ctrl::Seq::PrivateModeReset(_) = s {}
                        else if let ctrl::Seq::Unicode(c) = s {
                            print!("{}", ::std::char::from_u32(c).unwrap());
                        }
                        else {
                            println!("{:?}", s);
                        }*/

                        if let ctrl::Seq::SetWindowTitle(ref title) = s {
                            display.get_window().map(|w| w.set_title(title));
                        }

                        t.handle(s);
                    },
                    Err(StreamError::Retry)            => break,
                    Err(StreamError::EndOfInput)       => break,
                    // Buffer has tried to load but failed to get a complete parse anyway,
                    // skip and render frame, wait until next frame to continue parse:
                    Err(StreamError::Incomplete(_))    => break,
                    Err(StreamError::IoError(e))       => {
                        println!("IoError: {:?}", e);

                        break;
                    },
                    Err(StreamError::ParseError(b, e)) => {
                        println!("{:?} at {:?}", e, unsafe { ::std::str::from_utf8_unchecked(b) });
                    }
                }
            }

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

                let tsize = Coord {
                    col: (buf_size.0 / cell.0) as usize,
                    row: (buf_size.1 / cell.1) as usize,
                };

                t.resize(tsize);

                out.set_window_size(tsize, Coord {
                    col: buf_size.0 as usize,
                    row: buf_size.1 as usize,
                }).unwrap();

                // Message all processes in the child process group
                match unsafe { libc::kill(-child_pid, libc::SIGWINCH) } {
                    -1 => panic!("kill(child, SIGWINCH) failed: {:?}", io::Error::last_os_error()),
                    _  => {},
                }
            }

            if t.is_dirty() {
                t.write_output(&mut out).unwrap();

                let mut target = display.draw();

                g.draw(&mut target, &t, buf_size, (-1.0, 1.0));

                target.finish().unwrap();

                t.set_dirty(false);
            }
        }

        thread::sleep(Duration::from_millis((FIXED_TIME_STAMP - accumulator) / 1000000));
    }
}

struct Window {
    child_pid: libc::c_int,
}
