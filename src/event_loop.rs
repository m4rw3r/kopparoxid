//! Event loop management for child-process.

use std::sync::{Arc, Mutex};
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::io::{self, Write};
use std::thread;
use std::ptr;

use libc;

use chomp::buffer::{FixedSizeBuffer, Source, Stream, StreamError};
use chomp::buffer::data_source::ReadDataSource;

use mio::{EventLoop, EventLoopConfig, EventSet, Handler, PollOpt, Sender, Token, Timeout};
use mio::unix::UnixStream;

use glutin::WindowProxy;

use term::{ctrl, Term};

use pty;

const INPUT: Token = Token(0);

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
    /// Received character
    Character(char),
}

struct TermHandler {
    shell:       UnixStream,
    /// Process id (and process group id) of the stream found in `shell`
    child_pid:   libc::c_int,
    /// Parser buffer over `shell`
    buf:         Source<ReadDataSource<pty::Fd>, FixedSizeBuffer<u8>>,
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
        let mut t     = self.term.lock().unwrap();

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
                    error!("{:?} at {:?}", e, unsafe { ::std::str::from_utf8_unchecked(b) });
                }
            }
        }

        dirty
    }
}

const FRAME_TIME: u64 = 16;

impl Handler for TermHandler {
    type Timeout = ();
    type Message = Message;

    fn ready(&mut self, event_loop: &mut EventLoop<Self>, _token: Token, events: EventSet) {
        if events.is_readable() {
            // TODO: Check fill rate
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
            self.write_out(event_loop).unwrap();
        }

        if self.out_buf.is_empty() {
            event_loop.reregister(&self.shell, INPUT, EventSet::readable(), PollOpt::level()).unwrap();
        } else {
            info!("queueing up more writes");

            event_loop.reregister(&self.shell, INPUT, EventSet::writable() | EventSet::readable(), PollOpt::level()).unwrap();
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
                self.term.lock().unwrap().resize((width as usize, height as usize));

                pty::set_window_size(self.shell.as_raw_fd(), (width, height), (x, y)).unwrap();

                // Message all processes in the child process group
                match unsafe { libc::kill(-self.child_pid, libc::SIGWINCH) } {
                    -1 => panic!("kill(child, SIGWINCH) failed: {:?}", io::Error::last_os_error()),
                    _  => {},
                }
            },
            Character(c) => {
                write!(self.out_buf, "{}", c).unwrap();

                // Turn on writing
                event_loop.reregister(&self.shell, INPUT, EventSet::writable() | EventSet::readable(), PollOpt::level()).unwrap();
            }
        }
    }
}

pub fn run(mut m: pty::Fd, child_pid: libc::c_int, w: WindowProxy) -> (Arc<Mutex<Term>>, Sender<Message>) {
    let mut ev_cfg  = EventLoopConfig::new();

    // We do not want to block the event loop
    m.set_noblock();
    // Default timer tick is 100 ms which is too long
    ev_cfg.timer_tick_ms(FRAME_TIME);

    let mut ev_loop = EventLoop::configured(ev_cfg).unwrap();
    let t           = Arc::new(Mutex::new(Term::new_with_size(80, 24)));
    let shell       = unsafe { UnixStream::from_raw_fd(m.as_raw_fd()) };
    let mut buf     = Source::from_read(m, FixedSizeBuffer::new());

    buf.set_autofill(false);
    ev_loop.register(&shell, INPUT, EventSet::readable(), PollOpt::level()).unwrap();

    let mut handler = TermHandler {
        shell:       shell,
        child_pid:   child_pid,
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
    });

    (t, msg)
}
