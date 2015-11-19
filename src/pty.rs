use errno;
use libc;
use std::ffi;
use std::io;
use std::process;
use std::ptr;

#[link(name = "util")]
extern {
    fn openpty(master: *mut libc::c_int, slave: *mut libc::c_int, name: *const u8, termp: *const u8, winp: *const u8) -> libc::c_int;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Fd {
   fd: libc::c_int
}

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

    unsafe {
        match openpty(&mut m, &mut s, ptr::null(), ptr::null(), ptr::null()) {
            -1 => Err(io::Error::last_os_error()),
            _  => Ok((Fd{fd: m}, Fd{fd: s}))
        }
    }
}

fn execvp(cmd: &str, params: &[&str]) -> io::Error {
    let cmd      = ffi::CString::new(cmd).unwrap();
    let mut args = params.iter().map(|&s| ffi::CString::new(s).unwrap().as_ptr()).collect::<Vec<*const i8>>();

    args.push(ptr::null());

    unsafe {
        libc::execvp(cmd.as_ptr(), args.as_mut_ptr());
    }

    io::Error::last_os_error()
}

pub fn run_sh(m: Fd, s: Fd) -> ! {
    /* Get rid of the master fd before running the shell */
    drop(m);

    s.override_fd(libc::STDIN_FILENO).unwrap();
    s.override_fd(libc::STDOUT_FILENO).unwrap();
    s.override_fd(libc::STDERR_FILENO).unwrap();

    /* This will never return unless the shell command exits with error */
    print!("{}", execvp("zsh", &["-i"]));

    process::exit(-1);
}
