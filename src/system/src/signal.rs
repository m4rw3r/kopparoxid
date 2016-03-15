use std::io::{Error, Result};
use std::mem::transmute;

use libc;

use ::{ProcessGroup, ProcessId};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(i32)]
pub enum Signal {
    SigAbrt    = libc::SIGABRT,
    SigAlrm    = libc::SIGALRM,
    SigBus     = libc::SIGBUS,
    SigChld    = libc::SIGCHLD,
    SigCont    = libc::SIGCONT,
    SigFpe     = libc::SIGFPE,
    SigHup     = libc::SIGHUP,
    SigIll     = libc::SIGILL,
    SigInt     = libc::SIGINT,
    SigIo      = libc::SIGIO,
    SigKill    = libc::SIGKILL,
    SigPipe    = libc::SIGPIPE,
    SigProf    = libc::SIGPROF,
    SigQuit    = libc::SIGQUIT,
    SigSegv    = libc::SIGSEGV,
    SigStop    = libc::SIGSTOP,
    SigSys     = libc::SIGSYS,
    SigTerm    = libc::SIGTERM,
    SigTrap    = libc::SIGTRAP,
    SigStp     = libc::SIGTSTP,
    SigTtin    = libc::SIGTTIN,
    SigTtou    = libc::SIGTTOU,
    SigUrg     = libc::SIGURG,
    SigUsr1    = libc::SIGUSR1,
    SigUsr2    = libc::SIGUSR2,
    SigVtalrm  = libc::SIGVTALRM,
    SigWinch   = libc::SIGWINCH,
    SigXcpu    = libc::SIGXCPU,
    SigXfsz    = libc::SIGXFSZ,
}

impl From<Signal> for libc::c_int {
    fn from(s: Signal) -> libc::c_int {
        s as libc::c_int
    }
}

pub trait KillTarget {
    fn pid_t(&self) -> libc::pid_t;
}

impl KillTarget for ProcessId {
    fn pid_t(&self) -> libc::pid_t {
        self.0
    }
}

impl KillTarget for ProcessGroup {
    fn pid_t(&self) -> libc::pid_t {
        -self.0
    }
}

pub fn kill<P: KillTarget>(p: P, s: Signal) -> Result<()> {
    match unsafe { libc::kill(p.pid_t(), s.into()) } {
        -1 => Err(Error::last_os_error()),
        _  => Ok(()),
    }
}

#[repr(C)]
pub struct Info(libc::sigset_t);

#[repr(C)]
pub struct UContext(libc::c_void);

pub enum Handler {
    Default,
    Ignore,
    Handler(extern fn(Signal)),
    Action(extern fn(Signal, &mut Info, &mut UContext)),
}

impl From<Handler> for libc::sighandler_t {
    fn from(h: Handler) -> Self {
        match h {
            Handler::Default    => libc::SIG_DFL,
            Handler::Ignore     => libc::SIG_IGN,
            Handler::Handler(f) => unsafe { transmute(f) },
            Handler::Action(f)  => unsafe { transmute(f) },
        }
    }
}

impl From<libc::sigaction> for Handler {
    fn from(h: libc::sigaction) -> Self {
        match h.sa_sigaction {
            libc::SIG_DFL => Handler::Default,
            libc::SIG_IGN => Handler::Ignore,
            f if h.sa_flags & libc::SA_SIGINFO == libc::SA_SIGINFO => Handler::Action(unsafe { transmute(f) }),
            f => Handler::Handler(unsafe { transmute(f) }),
        }
    }
}

pub fn signal(s: Signal, h: Handler) -> Result<Handler> {
    let flags = if let Handler::Action(_) = h { libc::SA_SIGINFO } else { 0 };

    let action = libc::sigaction {
        sa_sigaction: h.into(), /* extern fn(Signal, siginfo_t *, void *) */
        // TODO: Allow the blocking of signals inside of the handler:
        sa_mask:      0, /* sigset_t */
        sa_flags:     flags, /* int */
    };
    let mut old = libc::sigaction {
        sa_sigaction: 0,
        sa_mask:      0,
        sa_flags:     0,
    };

    match unsafe {
        libc::sigaction(s.into(), &action as *const libc::sigaction, &mut old as *mut libc::sigaction)
    } {
        -1  => Err(Error::last_os_error()),
        _   => Ok(old.into()),
    }
}
