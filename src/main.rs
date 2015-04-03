/*#![feature(std_misc)]
#![feature(thread_sleep)]
#![feature(os)]
#![feature(libc)]

extern crate glutin;
extern crate glium;
extern crate clock_ticks;
extern crate libc;

mod pty {
    use libc::{c_ushort, c_int};
    use std::ptr;
    use std::io;
    
    #[derive(Debug)]
    pub struct Pty {
        master_fd: c_int,
        slave_fd:  c_int,
    }
    
    #[derive(Debug)]
    pub struct WindowSize {
        pub rows: c_ushort,
        pub cols: c_ushort,
        pub xpixel: c_ushort,
        pub ypixel: c_ushort,
    }
    
    /* TODO */
    struct TermIOs;
    
    #[link(name = "util")]
    extern {
        fn openpty(master: *mut c_int, slave: *mut c_int, name: *const u8, termp: *const u8, winp: *const WindowSize) -> c_int;
        fn forkpty(master: *mut c_int, name: *const u8, termp: *const u8, winp: *const u8) -> c_int;
    }
    
    pub fn open(size: &mut WindowSize) -> Result<Pty, io::Error> {
        let mut m: c_int = 0;
        let mut s: c_int = 0;
        
        unsafe {
            match openpty(m as *mut c_int, s as *mut c_int, ptr::null(), ptr::null(), ptr::null()) {
                -1 => Err(io::Error::last_os_error()),
                _  => Ok(Pty{
                    master_fd: m,
                    slave_fd:  s
                })
            }
            /*match forkpty(m as *mut c_int, ptr::null(), ptr::null(), ptr::null()) {
                -1 => Err(io::Error::last_os_error()),
                _  => Ok(Pty{
                    master_fd: m,
                    slave_fd:  s
                })
            }*/
        }
    }
}


use std::thread;
use std::time::duration::Duration;
pub enum Action {
    Stop,
    Continue,
}

pub fn start_loop<F>(mut callback: F) where F: FnMut() -> Action {
    let mut accumulator    = 0;
    let mut previous_clock = clock_ticks::precise_time_ns();
    
    loop {
        match callback() {
            Action::Stop     => break,
            Action::Continue => ()
        };
        
        let now        = clock_ticks::precise_time_ns();
        accumulator    += now - previous_clock;
        previous_clock = now;
        
        const FIXED_TIME_STAMP: u64 = 16666667;
        
        while accumulator >= FIXED_TIME_STAMP {
            accumulator -= FIXED_TIME_STAMP;
            // if you have a game, update the state here
        }
        
        thread::sleep(Duration::nanoseconds((FIXED_TIME_STAMP - accumulator) as i64));
    }
}

fn main() {
    use glium::DisplayBuild;
    
    let pty = pty::open(&mut pty::WindowSize{
        rows: 10,
        cols: 60,
        xpixel: 0,
        ypixel: 0,
    });
    
    print!("{:?}", pty);
     
    let display = glutin::WindowBuilder::new()
        .with_dimensions(1024, 768)
        .with_title(format!("Hello world"))
        .build_glium().unwrap();
    
    start_loop(|| {
        for event in display.poll_events() {
            match event {
                glutin::Event::Closed => return Action::Stop,
                _ =>  ()
            }
        }
        Action::Continue
    });
}*/

#![feature(libc)]
extern crate libc;

mod pty {
    use std::io::{Error, Read, Result};
    use std::ptr;
    use libc;

    #[link(name = "util")]
    extern {
        fn openpty(master: *mut libc::c_int, slave: *mut libc::c_int, name: *const u8, termp: *const u8, winp: *const u8) -> libc::c_int;
    }
    
    #[derive(Debug)]
    pub struct PtyFd {
       fd: libc::c_int
    }
    
    impl PtyFd {
        pub fn override_fd(&self, fd: libc::c_int) -> Result<()> {
            unsafe {
                match libc::dup2(self.fd, fd) {
                    -1 => Err(Error::last_os_error()),
                    _  => Ok(())
                }
            }
        }
    }
    
    impl Drop for PtyFd {
        fn drop(&mut self) {
            unsafe {
                libc::close(self.fd);
            }
        }
    }
    
    impl Read for PtyFd {
        fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
            unsafe {
                match libc::read(self.fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len() as libc::size_t) {
                    r if r < 0 => Err(Error::last_os_error()),
                    r          => Ok(r as usize),
                }
            }
        }
    }
    
    pub fn open() -> Result<(PtyFd, PtyFd)> {
        let mut m: libc::c_int = 0;
        let mut s: libc::c_int = 0;
        
        unsafe {
            match openpty(&mut m, &mut s, ptr::null(), ptr::null(), ptr::null()) {
                -1 => Err(Error::last_os_error()),
                _  => Ok((PtyFd{fd: m}, PtyFd{fd: s}))
            }
        }
    }
}

fn setup_child(p: pty::PtyFd) {
    use libc::STDIN_FILENO;
    use libc::STDOUT_FILENO;
    use libc::STDERR_FILENO;
     
    p.override_fd(STDIN_FILENO).unwrap();
    p.override_fd(STDOUT_FILENO).unwrap();
    p.override_fd(STDERR_FILENO).unwrap();
    
    print!("child\n");
}

fn main() {
    use std::io;
    use libc::fork;
    use std::io::Read;
    
    let (mut m, s) = pty::open().unwrap();
    
    match unsafe { fork() } {
        -1   => panic!(io::Error::last_os_error()),
         0   => {
             drop(m);
             setup_child(s)
         },
         pid => {
            drop(s);
            print!("master, child pid: {}\n", pid);
            
            let mut v = String::new();
            let r = m.read_to_string(&mut v);
            
            print!("{:?}", r);
            print!("{}", v);
         }
    }
}
