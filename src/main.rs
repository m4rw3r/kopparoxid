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
extern crate time;

extern crate kopparoxid_term as term;

mod pty;
mod gl;
mod event_loop;
mod window;

use window::{Font, FontFaces, Window};

use std::io;

use term::color;

const FONT_SIZE: u32 = 16;

fn main() {
    let (m, s) = pty::open().expect("Failed to open pty");

    // Make the current (main) process the group leader to propagate signals to children
    pty::create_process_group().expect("Failed to make process group");

    match unsafe { libc::fork() } {
        -1  => panic!(io::Error::last_os_error()),
        0   => pty::run_sh(m, s),
        pid => {
            env_logger::init().expect("Failed to create env_logger");

            info!("master, child pid: {}", pid);

            let faces = FontFaces {
                regular:     Font::new("./DejaVuSansMono/DejaVu Sans Mono for Powerline.ttf", FONT_SIZE),
                bold:        Some(Font::new("./DejaVuSansMono/DejaVu Sans Mono Bold for Powerline.ttf", FONT_SIZE)),
                italic:      Some(Font::new("./DejaVuSansMono/DejaVu Sans Mono Oblique for Powerline.ttf", FONT_SIZE)),
                bold_italic: Some(Font::new("./DejaVuSansMono/DejaVu Sans Mono Bold Oblique for Powerline.ttf", FONT_SIZE)),
            };

            let mut win = Window::new(faces, color::XtermDefault);

            // Start terminal
            let (terminal, msg) = event_loop::run(m, pid, win.create_proxy());

            win.run(terminal, msg);
        }
    }
}
