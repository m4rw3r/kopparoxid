#[macro_use]
extern crate log;
extern crate libc;
extern crate env_logger;
extern crate kopparoxid;
extern crate kopparoxid_term as term;

use kopparoxid::gl::glyph::{FreeTypeConfig, HintMode};
use kopparoxid::window::{Font, FontFaces, Window};
use kopparoxid::{pty, event_loop};
use term::color;

use std::io;

/// Helper struct which will automatically send a SIGHUP to the wrapped pid on Drop.
struct DropHup {
    pid: libc::c_int,
}

impl Drop for DropHup {
    fn drop(&mut self) {
        unsafe {
            // ignore error
            libc::kill(self.pid, libc::SIGHUP);

            info!("sent SIGHUP");
        }
    }
}

fn main() {
    let (m, s) = pty::open().expect("Failed to open pty");

    // Make the current (main) process the group leader to propagate signals to children
    pty::create_process_group().expect("Failed to make process group");

    match unsafe { libc::fork() } {
        -1  => panic!(io::Error::last_os_error()),
        0   => pty::run_sh(m, s),
        pid => {
            // Make sure we send SIGHUP whenever we exit
            let _child_hup = DropHup { pid: pid };

            env_logger::init().expect("Failed to create env_logger");

            info!("master, child pid: {}", pid);

            let size   = 16;
            let config = FreeTypeConfig {
                antialias: true,
                hinting:   Some(HintMode { autohint: true, light: false }),
            };

            let faces = FontFaces {
                regular:     Font::new("./DejaVuSansMono/DejaVu Sans Mono for Powerline.ttf", size, config),
                bold:        Some(Font::new("./DejaVuSansMono/DejaVu Sans Mono Bold for Powerline.ttf", size, config)),
                italic:      Some(Font::new("./DejaVuSansMono/DejaVu Sans Mono Oblique for Powerline.ttf", size, config)),
                bold_italic: Some(Font::new("./DejaVuSansMono/DejaVu Sans Mono Bold Oblique for Powerline.ttf", size, config)),
            };

            let mut win = Window::new(faces, color::XtermDefault);

            // Start terminal
            let (terminal, msg) = event_loop::run(m, pid, win.create_proxy());

            win.run(terminal, msg);
        }
    }
}
