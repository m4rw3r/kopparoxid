use std::sync::{Arc, Mutex};
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::io;
use std::thread;

use libc;

use chomp::buffer::{FixedSizeBuffer, Source, Stream, StreamError};
use chomp::buffer::data_source::ReadDataSource;

use mio::{EventLoop, EventSet, Handler, PollOpt, Sender, Token};
use mio::unix::UnixStream;

use glutin::WindowProxy;

use term::{ctrl, Term};

use pty;

const INPUT: Token = Token(0);
const WRITE: Token = Token(1);

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
    shell:     UnixStream,
    /// Process id (and process group id) of the stream found in `shell`
    child_pid: libc::c_int,
    /// Parser buffer over `shell`
    buf:       Source<ReadDataSource<pty::Fd>, FixedSizeBuffer<u8>>,
    /// Terminal data
    term:      Arc<Mutex<Term>>,
    win:       WindowProxy,
}

impl Handler for TermHandler {
    type Timeout = ();
    type Message = Message;

    fn ready(&mut self, event_loop: &mut EventLoop<Self>, token: Token, events: EventSet) {
        info!("shell ready: {:?} {:?}", token, events);

        match token {
            INPUT => {
                assert!(events.is_readable());

                self.buf.fill().unwrap();

                // If we need to update
                let mut dirty = false;
                let mut t     = self.term.lock().unwrap();

                loop {
                    match self.buf.parse(ctrl::parser) {
                        Ok(s) => {
                            /*if let ctrl::Seq::CharAttr(_) = s {}
                            else if let ctrl::Seq::PrivateModeSet(_) = s {}
                            else if let ctrl::Seq::PrivateModeReset(_) = s {}
                            else if let ctrl::Seq::Unicode(c) = s {
                                trace!("{}", ::std::char::from_u32(c).unwrap());
                            }
                            else {
                            }*/
                            trace!("{:?}", s);

                            match s {
                                // Nothing to do
                                ctrl::Seq::SetIconName(_) => {}
                                ctrl::Seq::SetWindowTitle(ref title) => {
                                    // TODO: Send to main thread
                                    // display.get_window().map(|w| w.set_title(title));

                                    continue;
                                },
                                s => {
                                    dirty = true;

                                    t.handle(s);
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

                if t.has_output() {
                    // We can write here too, READ implies WRITE for short delays
                    info!("wrote {}", t.write_output(&mut self.shell).unwrap());

                    if t.has_output() {
                        info!("queueing up more writes");

                        event_loop.register(&self.shell, WRITE, EventSet::writable(), PollOpt::oneshot()).unwrap();
                    }
                }

                if dirty {
                    info!("waking up window event loop");

                    self.win.wakeup_event_loop();
                }
            },
            WRITE => {
                let mut t = self.term.lock().unwrap();

                let n = t.write_output(&mut self.shell).unwrap();

                info!("wrote {}", n);

                if !t.has_output() {
                    info!("writes done");

                    event_loop.deregister(&self.shell).unwrap();

                    event_loop.register(&self.shell, INPUT, EventSet::readable(), PollOpt::level()).unwrap();
                }
            },
            _ => unreachable!(),
        }
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
                self.term.lock().unwrap().queue_character(c);

                event_loop.register(&self.shell, WRITE, EventSet::writable(), PollOpt::oneshot()).unwrap();
            }
        }
    }
}

pub fn run(m: pty::Fd, child_pid: libc::c_int, w: WindowProxy) -> (Arc<Mutex<Term>>, Sender<Message>) {
    let mut ev_loop = EventLoop::new().unwrap();
    let shell       = unsafe { UnixStream::from_raw_fd(m.as_raw_fd()) };

    let t = Arc::new(Mutex::new(Term::new_with_size(10, 10)));

    ev_loop.register(&shell, INPUT, EventSet::readable(), PollOpt::level()).unwrap();

    let mut buf = Source::from_read(m, FixedSizeBuffer::new());

    buf.set_autofill(false);

    let mut handler = TermHandler {
        shell:     shell,
        child_pid: child_pid,
        term:      t.clone(),
        buf:       buf,
        win:       w,
    };

    let msg = ev_loop.channel();

    thread::spawn(move || {
        info!("Starting terminal event loop");
        ev_loop.run(&mut handler).unwrap();
    });

    (t, msg)
}
