use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};
use std::io::{Error, Read, Result, Write};
use std::ptr;

use libc;
use errno;

const EAGAIN: libc::c_int = libc::EAGAIN as libc::c_int;

// OS X:
#[cfg(any(target_os = "macos", target_os = "ios"))]
const TIOCSWINSZ: libc::c_ulong = 0x80087467;
#[cfg(not(any(target_os ="macos", target_os = "ios")))]
use libc::TIOCSWINSZ;

/// Pseudoterminal
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Pty {
   fd: RawFd
}

impl Pty {
    /// Creates a new pseudoterminal with unspecified dimensions, first value is master and second
    /// is slave.
    pub fn new() -> Result<(Self, Self)> {
        Self::new_with_size((0, 0), (0, 0))
    }

    /// Creates a new pseudoterminal with the specified dimensions (width X height), first value is
    /// the master and the second the slave.
    pub fn new_with_size(term: (u32, u32), pixels: (u32, u32)) -> Result<(Self, Self)> {
        let mut m: RawFd = 0;
        let mut s: RawFd = 0;

        let mut ws = libc::winsize {
            ws_row:    term.1 as libc::c_ushort,
            ws_col:    term.0 as libc::c_ushort,
            ws_xpixel: pixels.0 as libc::c_ushort,
            ws_ypixel: pixels.1 as libc::c_ushort,
        };

        match unsafe { libc::openpty(&mut m, &mut s, ptr::null_mut(), ptr::null_mut(), &mut ws) } {
            -1 => Err(Error::last_os_error()),
            _  => Ok((Pty{fd: m}, Pty{fd: s}))
        }
    }

    /// Updates the window-size of the pseudoterminal (width X height) using `ioctl`.
    pub fn set_window_size(&mut self, term: (u32, u32), pixels: (u32, u32)) -> Result<()> {
        let ws = libc::winsize {
            ws_row:    term.1 as libc::c_ushort,
            ws_col:    term.0 as libc::c_ushort,
            ws_xpixel: pixels.0 as libc::c_ushort,
            ws_ypixel: pixels.1 as libc::c_ushort,
        };

        match unsafe { libc::ioctl(self.fd, TIOCSWINSZ, &ws) } {
            -1 => Err(Error::last_os_error()),
            _  => Ok(()),
        }
    }

    /// Overrides the specified file-descriptor given with the
    /// internal file-descriptor.
    pub fn override_fd(&self, fd: RawFd) -> Result<()> {
        match unsafe { libc::dup2(self.fd, fd) } {
            -1 => Err(Error::last_os_error()),
            _  => Ok(()),
        }
    }

    // TODO: Make it possible to turn blocking back on
    pub fn set_noblock(&mut self) {
        match unsafe { libc::fcntl(self.fd, libc::F_SETFL, libc::fcntl(self.fd, libc::F_GETFL) | libc::O_NONBLOCK) } {
            -1 => panic!(Error::last_os_error()),
            _  => ()
        }
    }
}

impl FromRawFd for Pty {
    #[inline]
    unsafe fn from_raw_fd(fd: RawFd) -> Pty {
        Pty { fd: fd }
    }
}

impl Read for Pty {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        match unsafe { libc::read(self.fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) } {
            -1 => match errno::errno().0 {
                EAGAIN => Ok(0),
                _      => Err(Error::last_os_error())
            },
            r  => Ok(r as usize),
        }
    }
}

impl Write for Pty {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        match unsafe { libc::write(self.fd, buf.as_ptr() as *const libc::c_void, buf.len()) } {
            -1 => Err(Error::last_os_error()),
            r  => Ok(r as usize),
        }
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.fd);
        }
    }
}

impl AsRawFd for Pty {
    #[inline]
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

mod mio {
    use super::Pty;
    use std::io;
    use mio::{Evented, Selector, Token, EventSet, PollOpt};

    impl Evented for Pty {
        fn register(&self, poll: &mut Selector, token: Token, interest: EventSet, opts: PollOpt) -> io::Result<()> {
            poll.register(self.fd, token, interest, opts)
        }

        fn reregister(&self, poll: &mut Selector, token: Token, interest: EventSet, opts: PollOpt) -> io::Result<()> {
            poll.reregister(self.fd, token, interest, opts)
        }

        fn deregister(&self, poll: &mut Selector) -> io::Result<()> {
            poll.deregister(self.fd)
        }
    }
}
