//! Event loop management for child-process.

#[macro_use]
extern crate log;
extern crate mio;
extern crate chomp;
extern crate glutin;
extern crate cu2o_term;
extern crate cu2o_system;

use std::sync::{Arc, Mutex};
use std::io::{self, Write};
use std::thread;
use std::ptr;

use chomp::buffer::{FixedSizeBuffer, Source, Stream, StreamError};
use chomp::buffer::data_source::ReadDataSource;

use mio::{EventLoop, EventLoopConfig, EventSet, Handler, PollOpt, Sender, Token, Timeout};
use mio::unix::PipeReader;

use glutin::WindowProxy;

use cu2o_term::{ctrl, Term};
use cu2o_system::{kill, ProcessId, Pty, Signal};

const INPUT: Token = Token(0);
const EXIT:  Token = Token(1);

#[derive(Clone, Copy, Debug)]
pub enum Message {
    /// Received resize
    Resize {
        /// Terminal width in columns
        width:  u32,
        /// Terminal height in columns
        height: u32,
        /// Window with in pixels
        x:      u32,
        /// Window height in pixels
        y:      u32,
    },
    /// Exit event loop
    Exit,
    /// Received character
    Character(char),
    /// Terminal received/lost focus
    Focus(bool),
}

struct TermHandler {
    shell:       Pty,
    /// Process id (and process group id) of the stream found in `shell`
    child_pid:   ProcessId,
    /// Pipe for self-pipe sending control data
    ///
    /// Never read from, but here to keep it alive until the TermHandler is destroyed.
    _exit_pipe:  Option<PipeReader>,
    /// Parser buffer over `shell`
    buf:         Source<ReadDataSource<Pty>, FixedSizeBuffer<u8>>,
    /// Terminal data
    term:        Arc<Mutex<Term>>,
    win:         WindowProxy,
    /// Output buffer with data to write to the process
    out_buf:     Vec<u8>,
    /// Timeout object for the window event loop wakeup
    win_timeout: Option<Timeout>,
}

impl TermHandler {
    fn write_out(&mut self) -> io::Result<usize> {
        if self.out_buf.is_empty() {
            return Ok(0);
        }

        self.shell.write(&self.out_buf).map(|n| {
            debug_assert!(n <= self.out_buf.len());

            unsafe {
                let new_len = self.out_buf.len() - n;
                let buf     = self.out_buf.as_mut_ptr();

                ptr::copy(buf.offset(n as isize), buf, new_len);

                self.out_buf.truncate(new_len);
            }

            n
        })
    }

    /// Parses control codes, returns true if codes were parsed.
    fn parse(&mut self) -> bool {
        // If we need to update
        let mut dirty = false;
        let mut t     = self.term.lock().expect("term::Term mutex poisoned");

        loop {
            match self.buf.parse(ctrl::parser) {
                Ok(s) => {
                    // trace!("{:?}", s);

                    match s {
                        // Nothing to do
                        ctrl::Seq::SetIconName(_) => {}
                        // TODO: Implement
                        ctrl::Seq::Bell => {}
                        s => {
                            // TODO: can we do something better here to determine if we actually
                            // need to update?
                            dirty = true;

                            t.handle(s, &mut self.out_buf).unwrap();
                        }
                    }
                },
                Err(StreamError::Retry)            => break,
                Err(StreamError::EndOfInput)       => break,
                // Buffer has tried to load but failed to get a complete parse anyway,
                // skip and render frame, wait until next frame to continue parse:
                Err(StreamError::Incomplete(_))    => break,
                Err(StreamError::IoError(e))       => {
                    error!("IoError: {:?}", e);

                    break;
                },
                Err(StreamError::ParseError(b, e)) => {
                    error!("{:?} at {:?}", e, String::from_utf8_lossy(b));
                }
            }
        }

        dirty
    }

    /// Sets the event loop to only listen for readable.
    fn set_read(&self, event_loop: &mut EventLoop<Self>) {
        event_loop.reregister(&self.shell,
                              INPUT,
                              EventSet::readable(),
                              PollOpt::level()).unwrap();
    }

    /// Sets the event loop to listen for both readable and writable events.
    fn set_write(&self, event_loop: &mut EventLoop<Self>) {
        event_loop.reregister(&self.shell,
                              INPUT,
                              EventSet::writable() | EventSet::readable(),
                              PollOpt::level()).unwrap();
    }
}

const FRAME_TIME: u64 = 16;

impl Handler for TermHandler {
    type Timeout = ();
    type Message = Message;

    fn ready(&mut self, event_loop: &mut EventLoop<Self>, token: Token, events: EventSet) {
        if token == EXIT {
            event_loop.shutdown();

            return;
        }

        if events.is_readable() {
            // TODO: Check fill rate, seems like pty buffer size is just 1K for some reason
            self.buf.fill().unwrap();

            let dirty = self.parse();

            if dirty && self.win_timeout.is_none() {
                info!("waking up window event loop");

                // TODO: Wouldn't this imply that it sometimes renders the same frame twice in
                // quick succession? ie. timeout fires and immediately after ready fires
                self.win.wakeup_event_loop();

                self.win_timeout = Some(event_loop.timeout_ms((), FRAME_TIME).unwrap());
            }
        }

        // Usually read implies write
        if events.is_writable() {
            self.write_out().unwrap();
        }

        if self.out_buf.is_empty() {
            self.set_read(event_loop);
        } else {
            info!("queueing up more writes");

            self.set_write(event_loop);
        }
    }

    fn timeout(&mut self, _event_loop: &mut EventLoop<Self>, _timeout: Self::Timeout) {
        info!("waking up window event loop");

        self.win.wakeup_event_loop();

        self.win_timeout = None;
    }

    fn notify(&mut self, event_loop: &mut EventLoop<Self>, msg: Message) {
        use self::Message::*;

        match msg {
            Resize{ width, height, x, y } => {
                self.term.lock().expect("term::Term mutex poisoned").resize((width as usize, height as usize));

                self.shell.set_window_size((width, height), (x, y)).unwrap();

                kill(self.child_pid.process_group(), Signal::SigWinch).unwrap();
            },
            Character(c) => {
                write!(self.out_buf, "{}", c).unwrap();

                self.set_write(event_loop);
            },
            Focus(got_focus) => {
                // Check mode for focus
                if self.term.lock().expect("term::Term mutex poisoned").send_focus_events() {
                    if got_focus {
                        write!(self.out_buf, "\x1B[I").unwrap();
                    } else {
                        write!(self.out_buf, "\x1B[O").unwrap();
                    }

                    self.set_write(event_loop);
                }
            },
            Exit => {
                event_loop.shutdown();
            },
        }
    }
}

// TODO: Make builder
pub fn run(mut m: Pty, child_pid: ProcessId, ctrl: Option<PipeReader>, w: WindowProxy) -> (Arc<Mutex<Term>>, Sender<Message>) {
    let mut ev_cfg  = EventLoopConfig::new();

    // We do not want to block the event loop
    m.set_noblock();
    // Default timer tick is 100 ms which is too long
    ev_cfg.timer_tick_ms(FRAME_TIME);

    let mut ev_loop = EventLoop::configured(ev_cfg).unwrap();
    let t           = Arc::new(Mutex::new(Term::new_with_size(80, 24)));
    let mut buf     = Source::from_read(m.clone(), FixedSizeBuffer::new());

    buf.set_autofill(false);
    ev_loop.register(&m, INPUT, EventSet::readable(), PollOpt::level()).unwrap();

    if let Some(c) = ctrl.as_ref() {
        info!("Registering EXIT pipe");

        ev_loop.register(c, EXIT, EventSet::readable(), PollOpt::level() | PollOpt::oneshot()).unwrap();
    }

    let mut handler = TermHandler {
        shell:       m,
        child_pid:   child_pid,
        _exit_pipe:  ctrl,
        term:        t.clone(),
        buf:         buf,
        win:         w,
        win_timeout: None,
        out_buf:     Vec::new(),
    };

    let msg = ev_loop.channel();

    thread::spawn(move || {
        info!("Starting terminal event loop");

        ev_loop.run(&mut handler).unwrap();

        info!("Event loop thread exiting");

        // TODO: Action here to notify window thread that event loop thread has stopped
    });

    (t, msg)
}
