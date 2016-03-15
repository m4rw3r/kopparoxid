#[macro_use]
extern crate log;
extern crate env_logger;
extern crate mio;
extern crate cu2o_loop;
extern crate cu2o_window;
extern crate cu2o_gl;
#[macro_use]
extern crate cu2o_system;

use std::env;
use std::process;
use std::io::Write;
use std::mem::transmute;
use std::sync::mpsc::{channel, Sender};

use cu2o_gl::glyph::{FreeTypeConfig, HintMode};
use cu2o_loop::Message;
use cu2o_system::{AtomicPtr, Pty, SignalHandler, ProcessId, Signal};
use cu2o_system::{create_session, create_process_group, execvp, fork, kill, signal};
use cu2o_gl::color;
use cu2o_window::{Action, Font, FontFaces, Window, WindowProxy};
use mio::unix::{pipe, PipeWriter};

/// Writer part of the selfpipe used by exit signal handler.
///
/// Allocated in a Box, `take` and `transmute` to Box to destroy when exiting.
static mut SELFPIPE:  AtomicPtr<PipeWriter> = atomic_ptr_null!();
/// Window loop notification.
static mut WINPROXY:  AtomicPtr<WindowProxy>   = atomic_ptr_null!();
/// Window event queue for external events.
static mut WINQUEUE:  AtomicPtr<Sender<Action>> = atomic_ptr_null!();

// TODO: Move this and statics into separate module
extern fn on_sigterm(s: Signal) {
    // This is probably wildly unsafe in general.
    //
    // * `info!` and `warn!` macros are probably already discouraged if not straight out disallowed
    //   inside of signal handlers.
    // * Using a channel inside of a signal handler is most likely discouraged
    // * And using the WindowProxy is probably wildly unsafe on X11 since it contains a mutex.
    // * Finally destroying the `Sender` and `WindowProxy` might not be good either.
    info!("Got signal: {:?}", s);

    unsafe { SELFPIPE.take() }.map(|pipe| {
        info!("Writing to SELFPIPE");

        let mut writer = unsafe{ transmute::<_, Box<PipeWriter>>(pipe) };

        match writer.write(&[b'e']) {
            Err(e) => warn!("Signal handler: Write to SELFPIPE failed: {:?}", e),
            _      => {},
        }
    });

    unsafe { WINQUEUE.take() }.map(|queue| {
        info!("Messaging window thread");

        match queue.send(Action::Quit) {
            Ok(()) => {},
            Err(e) => warn!("Signal handler: Messaging window thread failed: {:?}", e),
        }
    });

    unsafe { WINPROXY.take() }.map(|proxy| {
        info!("Waking up window event loop");

        let p = unsafe { transmute::<_, Box<WindowProxy>>(proxy) };

        p.wakeup_event_loop();
    });
}

extern fn on_sigint(s: Signal) {
    // TODO: This does not exit process
    println!("Got SIGINT: {:?}", s)
}

/// Helper struct which will automatically send a SIGHUP to the wrapped pid on Drop.
// TODO: Move?
struct DropHup {
    pid: ProcessId,
}

impl Drop for DropHup {
    fn drop(&mut self) {
        // ignore error
        let _ = kill(self.pid, Signal::SigHup);

        info!("sent SIGHUP");
    }
}

const OVERRIDE_ERR: &'static str = "failed to override file descriptors for child";
const SIGNAL_ERR:   &'static str = "failed to reset signal handlers for child";

fn run_sh(m: Pty, s: Pty) -> ! {
    // Get rid of the master fd before running the shell
    drop(m);

    // Needed to make sure that children receive the SIGWINCH signal
    create_session().expect("failed to create session");

    s.override_fd(cu2o_system::STDIN_FILENO).expect(OVERRIDE_ERR);
    s.override_fd(cu2o_system::STDOUT_FILENO).expect(OVERRIDE_ERR);
    s.override_fd(cu2o_system::STDERR_FILENO).expect(OVERRIDE_ERR);

    signal(Signal::SigChld, SignalHandler::Default).expect(SIGNAL_ERR);
    signal(Signal::SigHup,  SignalHandler::Default).expect(SIGNAL_ERR);
    signal(Signal::SigInt,  SignalHandler::Default).expect(SIGNAL_ERR);
    signal(Signal::SigQuit, SignalHandler::Default).expect(SIGNAL_ERR);
    signal(Signal::SigTerm, SignalHandler::Default).expect(SIGNAL_ERR);
    signal(Signal::SigAlrm, SignalHandler::Default).expect(SIGNAL_ERR);

    // Cleanup env
    env::remove_var("COLUMNS");
    env::remove_var("LINES");
    env::remove_var("TERMCAP");

    // TODO: Configurable
    env::set_var("TERM", "xterm-256color");

    // This will never return unless the shell command exits with error
    print!("{}", execvp("zsh", &["-i"][..]));

    process::exit(-1);
}

fn main() {
    let (m, s) = Pty::new().expect("Failed to open pty");

    // Make the current (main) process the group leader to propagate signals to children
    create_process_group().expect("Failed to make process group");

    match fork().expect("failed to fork child") {
        None      => run_sh(m, s),
        Some(pid) => {
            // Make sure we send SIGHUP whenever we exit
            let _child_hup = DropHup { pid: pid };

            signal(Signal::SigTerm, SignalHandler::Handler(on_sigterm)).expect("failed to install SIGTERM");
            signal(Signal::SigInt, SignalHandler::Handler(on_sigint)).expect("failed to install SIGINT");

            // Resource folder path
            let res = env::var("RESOURCES").unwrap_or("./res/Resources".to_owned());

            env_logger::init().expect("Failed to create env_logger");

            info!("master, child pid: {}", pid);

            let size   = 16;
            let config = FreeTypeConfig {
                antialias: true,
                hinting:   Some(HintMode { autohint: true, light: false }),
            };

            let regular     = format!("{}/DejaVuSansMono/DejaVu Sans Mono for Powerline.ttf", res);
            let bold        = format!("{}/DejaVuSansMono/DejaVu Sans Mono Bold for Powerline.ttf", res);
            let italic      = format!("{}/DejaVuSansMono/DejaVu Sans Mono Oblique for Powerline.ttf", res);
            let bold_italic = format!("{}/DejaVuSansMono/DejaVu Sans Mono Bold Oblique for Powerline.ttf", res);

            let faces = FontFaces {
                regular:     Font::new(&regular, size, config),
                bold:        Some(Font::new(&bold, size, config)),
                italic:      Some(Font::new(&italic, size, config)),
                bold_italic: Some(Font::new(&bold_italic, size, config)),
            };

            // Create channel for signals to close window
            let (tx, rx) = channel();

            let mut win = Window::new(faces, color::XtermDefault, rx);

            // Create selfpipe for letting signal handler notify mio event loop to stop
            let (recv_stop, snd_stop) = pipe().unwrap();

            // Store globals for signals
            unsafe { SELFPIPE.swap(Box::new(snd_stop)) };
            unsafe { WINPROXY.swap(Box::new(win.create_proxy())) };
            unsafe { WINQUEUE.swap(Box::new(tx)) };

            // Start terminal
            let (terminal, msg) = cu2o_loop::run(m, pid, Some(recv_stop), win.create_proxy());

            // Run window
            win.run(terminal, msg.clone());

            // If event loop is already destroyed, do nothing
            let _ = msg.send(Message::Exit);

            // Take and destroy the PipeWriter
            if let Some(pipe) = unsafe { SELFPIPE.take() } {
                let _foo = unsafe { transmute::<_, Box<PipeWriter>>(pipe) };
            }
        }
    }
}
