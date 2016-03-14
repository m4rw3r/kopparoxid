extern crate libc;
extern crate errno;
extern crate mio;

use std::fmt;
use std::ptr;
use std::ffi::CString;
use std::io::{Error, Result};

#[macro_use]
mod atomic_ptr;
mod signal;
mod pty;
mod selfpipe;

pub use libc::{STDIN_FILENO, STDOUT_FILENO, STDERR_FILENO};

pub use atomic_ptr::AtomicPtr;
pub use signal::{kill, signal, Signal, KillTarget};
pub use signal::Handler as SignalHandler;
pub use pty::Pty;

// TODO: Should probably be unsigned
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProcessId(libc::pid_t);

impl fmt::Display for ProcessId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl ProcessId {
    // TODO: This assumes that the process is its own process group
    pub fn process_group(&self) -> ProcessGroup {
        ProcessGroup(self.0)
    }
}

// TODO: Should probably be unsigned
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProcessGroup(libc::pid_t);

impl fmt::Display for ProcessGroup {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Forks the process, returning Ok(Some(ProcessId)) to the parent and Ok(None) to the child.
pub fn fork() -> Result<Option<ProcessId>> {
    match unsafe { libc::fork() } {
        -1  => Err(Error::last_os_error()),
        0   => Ok(None),
        pid => Ok(Some(ProcessId(pid))),
    }
}

/// Makes the current process the group leader of a new group with the group id set to its own
/// process id (`setpgid(0, 0)`).
pub fn create_process_group() -> Result<()> {
    match unsafe { libc::setpgid(0, 0) } {
        -1 => Err(Error::last_os_error()),
        _  => Ok(()),
    }
}

/// Creates a new session if the current process is not a process group leader. The current process
/// becomes the process group leader of the new session.
pub fn create_session() -> Result<ProcessGroup> {
    match unsafe { libc::setsid() } {
        -1 => Err(Error::last_os_error()),
        n  => Ok(ProcessGroup(n)),
    }
}

pub fn execvp<C: Into<Vec<u8>>, P: Into<Vec<C>>>(cmd: C, params: P) -> Error {
    let     cmd          = CString::new(cmd).unwrap();
    let     args: Vec<_> = params.into().into_iter().map(|s| CString::new(s).unwrap()).collect();
    let mut ptrs: Vec<_> = args.iter().map(|s| s.as_ptr()).collect();

    ptrs.push(ptr::null());

    unsafe {
        libc::execvp(cmd.as_ptr(), ptrs.as_mut_ptr());
    }

    // A return from execvp means an error occurred
    Error::last_os_error()
}
