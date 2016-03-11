use errno;
use libc;
use std::ffi;
use std::io;
use std::process;
use std::ptr;
use std::env;

use util::Coord;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Fd {
   fd: libc::c_int
}

// OS X:
// TODO: Add to libc crate
#[cfg(any(target_os = "macos", target_os = "ios"))]
const TIOCSWINSZ: libc::c_ulong = 0x80087467;
#[cfg(not(any(target_os ="macos", target_os = "ios")))]
use libc::TIOCSWINSZ;

impl Fd {
    /// Overrides the specified file-descriptor given with the
    /// internal file-descriptor.
    pub fn override_fd(&self, fd: libc::c_int) -> io::Result<()> {
        unsafe {
            match libc::dup2(self.fd, fd) {
                -1 => Err(io::Error::last_os_error()),
                _  => Ok(()),
            }
        }
    }

    /// Sets the window-size
    pub fn set_window_size(&mut self, term: Coord<usize>, pixels: Coord<usize>) -> io::Result<()> {
        unsafe {
            let ws = libc::winsize {
                ws_row:    term.row as libc::c_ushort,
                ws_col:    term.col as libc::c_ushort,
                ws_xpixel: pixels.row as libc::c_ushort,
                ws_ypixel: pixels.col as libc::c_ushort,
            };

            match libc::ioctl(self.fd, TIOCSWINSZ, &ws) {
                -1 => Err(io::Error::last_os_error()),
                _  => Ok(()),
            }
        }
    }

    pub fn set_noblock(&mut self) {
        unsafe {
            match libc::fcntl(self.fd, libc::F_SETFL, libc::fcntl(self.fd, libc::F_GETFL) | libc::O_NONBLOCK) {
                -1 => panic!(io::Error::last_os_error()),
                _  => ()
            }
        }
    }
}

impl Drop for Fd {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.fd);
        }
    }
}

const EAGAIN: libc::c_int = libc::EAGAIN as libc::c_int;

impl io::Read for Fd {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        unsafe {
            match libc::read(self.fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) {
                -1 => match errno::errno().0 {
                    EAGAIN => Ok(0),
                    _      => Err(io::Error::last_os_error())
                },
                r  => Ok(r as usize),
            }
        }
    }
}

impl io::Write for Fd {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        unsafe {
            match libc::write(self.fd, buf.as_ptr() as *const libc::c_void, buf.len()) {
                -1 => Err(io::Error::last_os_error()),
                r  => Ok(r as usize),
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Opens a new pseudoterminal returning the filedescriptors for master
/// and slave.
pub fn open() -> io::Result<(Fd, Fd)> {
    let mut m: libc::c_int = 0;
    let mut s: libc::c_int = 0;

    let mut ws = libc::winsize {
        ws_row:    0,
        ws_col:    0,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };

    unsafe {
        match libc::openpty(&mut m, &mut s, ptr::null_mut(), ptr::null_mut(), &mut ws) {
            -1 => Err(io::Error::last_os_error()),
            _  => Ok((Fd{fd: m}, Fd{fd: s}))
        }
    }
}

fn execvp(cmd: &str, params: &[&str]) -> io::Error {
    let     cmd          = ffi::CString::new(cmd).unwrap();
    let     args: Vec<_> = params.iter().map(|&s| ffi::CString::new(s).unwrap()).collect();
    let mut ptrs: Vec<_> = args.iter().map(|s| s.as_ptr()).collect();

    ptrs.push(ptr::null());

    unsafe {
        libc::execvp(cmd.as_ptr(), ptrs.as_mut_ptr());
    }

    io::Error::last_os_error()
}

pub fn run_sh(m: Fd, s: Fd) -> ! {
    // Get rid of the master fd before running the shell
    drop(m);

    // Needed to make sure that children receive the SIGWINCH signal
    match unsafe { libc::setsid() } {
        -1 => panic!("setsid() failed: {:?}", io::Error::last_os_error()),
        _  => {}
    }

    s.override_fd(libc::STDIN_FILENO).unwrap();
    s.override_fd(libc::STDOUT_FILENO).unwrap();
    s.override_fd(libc::STDERR_FILENO).unwrap();

    // Cleanup env
    env::remove_var("COLUMNS");
    env::remove_var("LINES");
    env::remove_var("TERMCAP");

    // TODO: Configurable
    env::set_var("TERM", "xterm-256color");

    // This will never return unless the shell command exits with error
    print!("{}", execvp("zsh", &["-i"]));

    process::exit(-1);
}
