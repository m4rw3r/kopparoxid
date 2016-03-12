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

use glutin::{Event, GlRequest, WindowBuilder};

use gl::glyph::{self, FreeType, Renderer};
use gl::term::{GlTerm, FontStyle};

use glium::DisplayBuild;

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

            window(m, pid)
        }
    }
}

const FONT_SIZE: f32 = 16.0;

fn load_font(f: &mut ft::Library, path: &str, size: u32) -> Box<Renderer<u8>> {
    let ft_face = f.new_face(path, 0).unwrap();

    ft_face.set_pixel_sizes(0, size).unwrap();

    // TODO: Antialiasing and hinting settings
    Box::new(FreeType::new(ft_face, glyph::FreeTypeMode::Greyscale))
}

fn window(m: pty::Fd, child_pid: libc::c_int) {

    info!("creating window");

    //let mut out = m.clone();
    let display = WindowBuilder::new()
        .with_gl(GlRequest::Specific(glutin::Api::OpenGl, (3, 3)))
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
    let mut g = GlTerm::new(&display, color::XtermDefault, f_map).unwrap();

    unsafe { display.get_window().map(|w| w.make_current()); };

    // Start terminal
    let (terminal, msg) = event_loop::run(m, child_pid, proxy);

    let mut bufsize = display.get_framebuffer_dimensions();

    msg.send(Message::Resize{
        width:  bufsize.0 / cell.0,
        height: bufsize.1 / cell.1,
        x:      bufsize.0,
        y:      bufsize.1,
    }).unwrap();

    info!("Window: starting event loop");

    for i in display.wait_events() {
        match i {
            Event::Closed               => process::exit(0),
            // TODO: Proper keyboard handling
            Event::ReceivedCharacter(c) => msg.send(Message::Character(c)).unwrap(),
            Event::MouseMoved(_)        => {},
            Event::Awakened             => {
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

                {
                    let t = terminal.lock().unwrap();

                    display.get_window().map(|w| w.set_title(t.get_title()));

                    g.draw(&mut target, &t, bufsize, (-1.0, 1.0));
                }

                target.finish().unwrap();
            },
            // TODO: More events
            _ => {} // println!("w {:?}", i)
        }
    }
}
