#[macro_use]
extern crate bitflags;
extern crate chomp;
extern crate env_logger;
extern crate errno;
extern crate freetype as ft;
#[macro_use]
extern crate glium;
extern crate glutin;
extern crate libc;
#[macro_use]
extern crate log;
extern crate mio;

extern crate kopparoxid_term as term;

mod pty;
mod gl;
mod event_loop;

use std::io;
use std::process;

use gl::glyph;

use term::color;

use event_loop::Message;

fn main() {
    let (m, s) = pty::open().unwrap();

    match unsafe { libc::fork() } {
        -1  => panic!(io::Error::last_os_error()),
        0   => pty::run_sh(m, s),
        pid => {
            env_logger::init().unwrap();

            info!("master, child pid: {}", pid);

            // TODO: How to set the buffer size of the pipe here?
            //m.set_noblock();

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

    info!("creating window");

    //let mut out = m.clone();
    let display = glutin::WindowBuilder::new()
        .with_gl(glutin::GlRequest::Specific(glutin::Api::OpenGl, (3, 3)))
        .with_srgb(Some(true))
        .build_glium()
        .unwrap();


    let mut ft_lib = ft::Library::init().unwrap();
    let mut f_map  = glyph::Map::new(&display);
    let scale      = display.get_window().map(|w| w.hidpi_factor()).unwrap_or(1.0);
    let proxy      = display.get_window().unwrap().create_window_proxy();

    let faces = [
        (FontStyle::Regular,    "./DejaVuSansMono/DejaVu Sans Mono for Powerline.ttf"),
        (FontStyle::Bold,       "./DejaVuSansMono/DejaVu Sans Mono Bold for Powerline.ttf"),
        (FontStyle::Italic,     "./DejaVuSansMono/DejaVu Sans Mono Oblique for Powerline.ttf"),
        (FontStyle::BoldItalic, "./DejaVuSansMono/DejaVu Sans Mono Bold Oblique for Powerline.ttf"),
    ];

    for &(f, t) in &faces {
        f_map.add_renderer(f, load_font(&mut ft_lib, t, (FONT_SIZE * scale) as u32)).unwrap();
    }

    let cell  = f_map.cell_size();
    let mut g = gl::term::GlTerm::new(&display, color::XtermDefault, f_map).unwrap();

    // Default title
    display.get_window().map(|w| w.set_title("Kopparoxid"));

    unsafe { display.get_window().map(|w| w.make_current()); };

    // Start terminal
    let (t, msg) = event_loop::run(m, child_pid, proxy);

    let mut bufsize = display.get_framebuffer_dimensions();

    msg.send(Message::Resize{
        width:  bufsize.0 / cell.0,
        height: bufsize.1 / cell.1,
        x:      bufsize.0,
        y:      bufsize.1,
    }).unwrap();

    {
        let mut target = display.draw();

        g.draw(&mut target, &t.lock().unwrap(), bufsize, (-1.0, 1.0));

        target.finish().unwrap();
    }

    info!("Window: waiting for events");

    for i in display.wait_events() {
        match i {
            glutin::Event::Closed               => process::exit(0),
            glutin::Event::ReceivedCharacter(c) => msg.send(Message::Character(c)).unwrap(),
            glutin::Event::MouseMoved(_) => {},
            glutin::Event::Awakened => {
                info!("Window: rendering");

                let new_bufsize = display.get_framebuffer_dimensions();

                // OS X does not fire glutin::Event::Resize from poll_events(), need to check manually
                // TODO: Proper resize handling
                if new_bufsize != bufsize {
                    bufsize = new_bufsize;

                    msg.send(Message::Resize{
                        width:  bufsize.0 / cell.0,
                        height: bufsize.1 / cell.1,
                        x:      bufsize.0,
                        y:      bufsize.1,
                    }).unwrap();
                }

                let mut target = display.draw();

                g.draw(&mut target, &t.lock().unwrap(), bufsize, (-1.0, 1.0));

                target.finish().unwrap();
            },
            _                            => {} // println!("w {:?}", i)
        }
    }
}
