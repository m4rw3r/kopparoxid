extern crate libc;
extern crate clock_ticks;
extern crate errno;
extern crate glutin;
#[macro_use]
extern crate glium;

/// Module for handling pseudoterminals
mod pty {
    use std::io::{Error, Read, Result, Write};
    use std::ptr;
    use libc;
    use errno::errno;

    #[link(name = "util")]
    extern {
        fn openpty(master: *mut libc::c_int, slave: *mut libc::c_int, name: *const u8, termp: *const u8, winp: *const u8) -> libc::c_int;
    }
    
    #[derive(Debug)]
    pub struct Fd {
       fd: libc::c_int
    }
    
    impl Fd {
        /// Overrides the specified file-descriptor given with the
        /// internal file-descriptor.
        pub fn override_fd(&self, fd: libc::c_int) -> Result<()> {
            unsafe {
                match libc::dup2(self.fd, fd) {
                    -1 => Err(Error::last_os_error()),
                    _  => Ok(()),
                }
            }
        }
        
        pub fn set_noblock(&mut self) {
            unsafe {
                match libc::fcntl(self.fd, libc::F_SETFL, libc::fcntl(self.fd, libc::F_GETFL) | libc::O_NONBLOCK) {
                    -1 => panic!(Error::last_os_error()),
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
    
    impl Read for Fd {
        fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
            unsafe {
                match libc::read(self.fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len() as libc::size_t) {
                    -1 => match errno().0 {
                        EAGAIN => Ok(0),
                        _      => Err(Error::last_os_error())
                    },
                    r  => Ok(r as usize),
                }
            }
        }
    }
    
    impl Write for Fd {
        fn write(&mut self, buf: &[u8]) -> Result<usize> {
            unsafe {
                match libc::write(self.fd, buf.as_ptr() as *const libc::c_void, buf.len() as u64) {
                    -1 => Err(Error::last_os_error()),
                    r  => Ok(r as usize),
                }
            }
        }
        
        fn flush(&mut self) -> Result<()> {
            Ok(())
        }
    }
    
    /// Opens a new pseudoterminal returning the filedescriptors for master
    /// and slave.
    pub fn open() -> Result<(Fd, Fd)> {
        let mut m: libc::c_int = 0;
        let mut s: libc::c_int = 0;
        
        unsafe {
            match openpty(&mut m, &mut s, ptr::null(), ptr::null(), ptr::null()) {
                -1 => Err(Error::last_os_error()),
                _  => Ok((Fd{fd: m}, Fd{fd: s}))
            }
        }
    }
}

mod ctrl {
    use std::io;
    use std::str;
    
    #[derive(Debug)]
    pub enum Seq {
        /* Single character functions */
        Bell,
        Backspace,
        CarriageReturn,
        ReturnTerminalStatus,
        FormFeed,
        LineFeed,
        ShiftIn,
        ShiftOut,
        Tab,
        TabVertical,
        
        Delete,
        
        Unicode(u32),
        
        Index,
        NextLine,
        TabSet,
        ReverseIndex,
        SingleShiftSelectG2CharSet,
        SingleShiftSelectG3CharSet,
        DeviceControlString,
        StartOfGuardedArea,
        EndOfGuardedArea,
        StartOfString,
        ReturnTerminalId,
        ControlSequenceIntroducer,
        StringTerminator,
        OperatingSystemCommand,
        PrivacyMessage,
        ApplicationProgramCommand,
        
        SetKeypadMode(KeypadMode),
        
        /* CSI */
        CharAttr(CharAttr),
        EraseInLine(EraseInLine),
        EraseInDisplay(EraseInDisplay),
        /* OSC */
        SetWindowTitle(String),
        SetIconName(String),
        SetXProps(String),
        SetColorNumber(String),
    }
    
    #[derive(Debug)]
    pub enum KeypadMode {
        Numeric,
        Application,
    }
    
    #[derive(Debug)]
    pub enum EraseInLine {
        Left,
        Right,
        All,
    }
    
    #[derive(Debug)]
    pub enum EraseInDisplay {
        Above,
        Below,
        All,
    }
    
    #[derive(Debug)]
    pub enum CharType {
        Normal,
        Bold,
        Faint,
        
        Italicized,
        Underlined,
        Blink,
        Inverse,
        Invisible,
        CrossedOut,
        DoublyUnderlined,
    }
    
    #[derive(Debug)]
    pub enum Color {
        Black,
        Red,
        Green,
        Yellow,
        Blue,
        Magenta,
        Cyan,
        White,
        Default,
        RGB(u8, u8, u8)
    }

    #[derive(Debug)]
    pub enum CharAttr {
        Reset,
        Set(CharType),
        Unset(CharType),
        FGColor(Color),
        BGColor(Color),
    }
    
    enum ParserState {
        Default,
        ESC,
        CSI,
        OSC,
        CharAttr,
        Unicode(u32, u8),
    }
    
    pub struct Parser<T: io::Read> {
        src:   io::Bytes<T>,
        state: ParserState,
        buf:   Vec<u8>,
    }
    
    macro_rules! parse_osc (
        ( $me:ident ) => ({
            let r = if $me.buf.len() > 2 && $me.buf[1] == b';' {
                match $me.buf[0] {
                    b'0' => Some(Seq::SetWindowTitle(String::from_utf8_lossy(&$me.buf[2..]).into_owned())), // And icon name
                    b'1' => Some(Seq::SetIconName(String::from_utf8_lossy(&$me.buf[2..]).into_owned())),
                    b'2' => Some(Seq::SetWindowTitle(String::from_utf8_lossy(&$me.buf[2..]).into_owned())),
                    b'3' => Some(Seq::SetXProps(String::from_utf8_lossy(&$me.buf[2..]).into_owned())),
                    b'4' => Some(Seq::SetColorNumber(String::from_utf8_lossy(&$me.buf[2..]).into_owned())),
                    _    => None
                }
            }
            else {
                None
            };
            
            match r {
                Some(x) => {
                    $me.state = ParserState::Default;
                    
                    $me.buf.truncate(0);
                    
                    return Some(x);
                },
                _ => {
                    match str::from_utf8(&$me.buf[..]) {
                        Ok(x) => println!("Unknown OSC: {}", x),
                        _     => println!("Unknown OSC bytes: {:?}", $me.buf)
                    }
                    
                    $me.state = ParserState::Default;
                    
                    $me.buf.truncate(0);
                    
                    continue;
                }
            };
        })
    );
    
    /// Retrieves the next character from the internal iterator.
    macro_rules! next_char (
        ( $me:ident ) => ( match $me.src.next() {
            Some(Ok(c))    => c,
            Some(Err(err)) => {
                println!("Error during parsing: {}", err);
                
                return None
            },
            None           => return None
        } )
    );
    
    /// Returns the next int (or None) from the buffer, separated by ';'.
    /// 
    /// Parses up to the next ';' or the end of the buffer (throwing away
    /// the ';', leaving the reset present in the buffer.
    macro_rules! buf_next_int (
        ( $me:ident ) => (
            String::from_utf8(match $me.buf.iter().position(|c| *c == b';') {
                Some(p) => {
                    let v = $me.buf[..p].to_vec();
                    
                    $me.buf = $me.buf[p+1..].to_vec();
                    
                    v
                },
                None => {
                    let v = $me.buf.clone();
                    
                    $me.buf.truncate(0);
                    
                    v
                }
            })
            .ok()
            .map_or(None, |s| s.parse::<u8>().ok())
        )
    );
    
    /// Prints a warning message that a control sequence introducer, attempts
    /// to print the result as a unicode string if possible, otheriwse dumps
    /// byte array.
    /// 
    /// Resets state and buffer.
    macro_rules! unknown_csi (
        ( $me:ident ) => ({
            match str::from_utf8(&$me.buf[..]) {
                Ok(x) => println!("Unknown CSI: {}", x),
                _     => println!("Unknown CSI bytes: {:?}", $me.buf)
            };
            
            $me.buf.truncate(0);
            
            $me.state = ParserState::Default;
            
            continue;
        });
        
        ( $me:ident, $buf:ident ) => ({
            match str::from_utf8(&$buf[..]) {
                Ok(x) => println!("Unknown CSI: {}", x),
                _     => println!("Unknown CSI bytes: {:?}", $buf)
            };
            
            $me.buf.truncate(0);
            
            $me.state = ParserState::Default;
            
            continue;
        });
    );
    
    /// Returns the given sequence entry and resets the parser state.
    /// Does NOT reset the buffer.
    macro_rules! return_reset (
        ( $me:ident, $ret:expr ) => ({
            $me.state = ParserState::Default;
            
            return Some($ret);
        })
    );
    
    static UTF8_TRAILING: [u8; 256] = [
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0, 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0, 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0, 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0, 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0, 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0, 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1, 1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,
    2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2, 3,3,3,3,3,3,3,3,4,4,4,4,5,5,5,5];
    
    impl<T: io::Read> Iterator for Parser<T> {
        type Item = Seq;
        
        fn next(&mut self) -> Option<Seq> {
            loop {
                match self.state {
                    ParserState::Default => match next_char!(self) {
                        0x07 => return Some(Seq::Bell),
                        0x08 => return Some(Seq::Backspace),
                        0x09 => return Some(Seq::Tab),
                        0x0A => return Some(Seq::LineFeed),
                        0x0B => return Some(Seq::TabVertical),
                        0x0C => return Some(Seq::FormFeed),
                        0x0D => return Some(Seq::CarriageReturn),
                        0x0E => return Some(Seq::ShiftOut),
                        0x0F => return Some(Seq::ShiftIn),
                        
                        0x1B => self.state = ParserState::ESC,
                        
                        c if c > 127 => {
                            let tail = UTF8_TRAILING[c as usize];
                            let chr  = (c as u32) & (0xff >> (tail + 2));
                            
                            self.state = ParserState::Unicode(chr, tail - 1)
                        },
                        c => return Some(Seq::Unicode(c as u32)),
                    },
                    ParserState::Unicode(chr, 0) => match next_char!(self) {
                        c if c > 127 => {
                            self.state = ParserState::Default;
                        
                            return Some(Seq::Unicode((chr << 6) + ((c as u32) & 0x3f)));
                        },
                        c => {
                            println!("Invalid UTF-8 sequence: {:x}", c);
                        }
                    },
                    ParserState::Unicode(chr, i) => match next_char!(self) {
                        c if c > 127 => {
                            self.state = ParserState::Unicode((chr << 6) + ((c as u32) & 0x3f), i - 1);
                        },
                        c => {
                            println!("Invalid UTF-8 sequence: {:x}", c);
                        }
                    },
                    ParserState::ESC => match next_char!(self) {
                        b'D' => return_reset!(self, Seq::Index), /* IND */
                        b'E' => return_reset!(self, Seq::NextLine), /* NEL */
                        b'H' => return_reset!(self, Seq::TabSet), /* HTS */
                        b'M' => return_reset!(self, Seq::ReverseIndex), /* RI */
                        b'N' => return_reset!(self, Seq::SingleShiftSelectG2CharSet), /* SS2 */
                        b'O' => return_reset!(self, Seq::SingleShiftSelectG3CharSet), /* SS3 */
                        b'P' => return_reset!(self, Seq::DeviceControlString), /* DCS */
                        b'V' => return_reset!(self, Seq::StartOfGuardedArea), /* SPA */
                        b'W' => return_reset!(self, Seq::EndOfGuardedArea), /* EPA */
                        b'X' => return_reset!(self, Seq::StartOfString), /* SOS */
                        b'Z' => return_reset!(self, Seq::ReturnTerminalId), /* DECID */
                        b'[' => self.state = ParserState::CSI, /* CSI */
                        b'\\' => return_reset!(self, Seq::StringTerminator), /* ST */
                        b']' => self.state = ParserState::OSC, /* OSC */
                        b'^' => return_reset!(self, Seq::PrivacyMessage), /* PM */
                        b'_' => return_reset!(self, Seq::ApplicationProgramCommand), /* APC */
                        
                        b'>' => return_reset!(self, Seq::SetKeypadMode(KeypadMode::Numeric)),
                        b'=' => return_reset!(self, Seq::SetKeypadMode(KeypadMode::Application)),
                        
                        c => {
                            print!("Unknown escape char code: {}\n", c);
                                
                            self.state = ParserState::Default;
                        }
                        // Some(b" ") => match 
                    },
                    ParserState::CSI => match next_char!(self) {
                        b'm' => self.state = ParserState::CharAttr,
                        b'J' => {
                            let r = match buf_next_int!(self) {
                                Some(1) => Some(Seq::EraseInDisplay(EraseInDisplay::Above)),
                                Some(2) => Some(Seq::EraseInDisplay(EraseInDisplay::All)),
                                _       => Some(Seq::EraseInDisplay(EraseInDisplay::Below)),
                            };
                            
                            self.buf.truncate(0);
                            
                            self.state = ParserState::Default;
                            
                            return r;
                        },
                        b'K' => {
                            let r = match buf_next_int!(self) {
                                Some(1) => Some(Seq::EraseInLine(EraseInLine::Left)),
                                Some(2) => Some(Seq::EraseInLine(EraseInLine::All)),
                                _       => Some(Seq::EraseInLine(EraseInLine::Right)),
                            };
                            
                            self.buf.truncate(0);
                            
                            self.state = ParserState::Default;
                            
                            return r;
                        },
                        c if c >= 0x40 && c <= 0x7E => {
                            self.buf.push(c);
                            
                            unknown_csi!(self);
                        },
                        c => self.buf.push(c)
                    },
                    ParserState::CharAttr => {
                        let mut cpy = self.buf.clone();
                        
                        cpy.push(b'm');
                        
                        let r = match buf_next_int!(self) {
                            Some(0)              => Some(Seq::CharAttr(CharAttr::Reset)),
                            Some(1)              => Some(Seq::CharAttr(CharAttr::Set(CharType::Bold))),
                            Some(2)              => Some(Seq::CharAttr(CharAttr::Set(CharType::Faint))),
                            Some(3)              => Some(Seq::CharAttr(CharAttr::Set(CharType::Italicized))),
                            Some(4)              => Some(Seq::CharAttr(CharAttr::Set(CharType::Underlined))),
                            Some(5)              => Some(Seq::CharAttr(CharAttr::Set(CharType::Blink))),
                            Some(7)              => Some(Seq::CharAttr(CharAttr::Set(CharType::Inverse))),
                            Some(8)              => Some(Seq::CharAttr(CharAttr::Set(CharType::Invisible))),
                            Some(9)              => Some(Seq::CharAttr(CharAttr::Set(CharType::CrossedOut))),
                            Some(21)             => Some(Seq::CharAttr(CharAttr::Set(CharType::DoublyUnderlined))),
                            Some(22)             => Some(Seq::CharAttr(CharAttr::Set(CharType::Normal))), /* Not bold, not faint */
                            Some(23)             => Some(Seq::CharAttr(CharAttr::Unset(CharType::Italicized))),
                            Some(24)             => Some(Seq::CharAttr(CharAttr::Unset(CharType::Underlined))),
                            Some(25)             => Some(Seq::CharAttr(CharAttr::Unset(CharType::Blink))),
                            Some(27)             => Some(Seq::CharAttr(CharAttr::Unset(CharType::Inverse))),
                            Some(28)             => Some(Seq::CharAttr(CharAttr::Unset(CharType::Invisible))),
                            Some(29)             => Some(Seq::CharAttr(CharAttr::Unset(CharType::CrossedOut))),
                            Some(30) | Some(90)  => Some(Seq::CharAttr(CharAttr::FGColor(Color::Black))),
                            Some(31) | Some(91)  => Some(Seq::CharAttr(CharAttr::FGColor(Color::Red))),
                            Some(32) | Some(92)  => Some(Seq::CharAttr(CharAttr::FGColor(Color::Green))),
                            Some(33) | Some(93)  => Some(Seq::CharAttr(CharAttr::FGColor(Color::Yellow))),
                            Some(34) | Some(94)  => Some(Seq::CharAttr(CharAttr::FGColor(Color::Blue))),
                            Some(35) | Some(95)  => Some(Seq::CharAttr(CharAttr::FGColor(Color::Magenta))),
                            Some(36) | Some(96)  => Some(Seq::CharAttr(CharAttr::FGColor(Color::Cyan))),
                            Some(37) | Some(97)  => Some(Seq::CharAttr(CharAttr::FGColor(Color::White))),
                            Some(39) | Some(99)  => Some(Seq::CharAttr(CharAttr::FGColor(Color::Default))),
                            Some(40) | Some(100) => Some(Seq::CharAttr(CharAttr::BGColor(Color::Black))),
                            Some(41) | Some(101) => Some(Seq::CharAttr(CharAttr::BGColor(Color::Red))),
                            Some(42) | Some(102) => Some(Seq::CharAttr(CharAttr::BGColor(Color::Green))),
                            Some(43) | Some(103) => Some(Seq::CharAttr(CharAttr::BGColor(Color::Yellow))),
                            Some(44) | Some(104) => Some(Seq::CharAttr(CharAttr::BGColor(Color::Blue))),
                            Some(45) | Some(105) => Some(Seq::CharAttr(CharAttr::BGColor(Color::Magenta))),
                            Some(46) | Some(106) => Some(Seq::CharAttr(CharAttr::BGColor(Color::Cyan))),
                            Some(47) | Some(107) => Some(Seq::CharAttr(CharAttr::BGColor(Color::White))),
                            Some(49)             => Some(Seq::CharAttr(CharAttr::BGColor(Color::Default))),
                            Some(38)             => match (buf_next_int!(self), buf_next_int!(self), buf_next_int!(self), buf_next_int!(self)) {
                                (Some(2), Some(r), Some(g), Some(b)) => Some(Seq::CharAttr(CharAttr::FGColor(Color::RGB(r, g, b)))),
                                _                                    => unknown_csi!(self, cpy)
                            },
                            Some(48)             => match (buf_next_int!(self), buf_next_int!(self), buf_next_int!(self), buf_next_int!(self)) {
                                (Some(2), Some(r), Some(g), Some(b)) => Some(Seq::CharAttr(CharAttr::BGColor(Color::RGB(r, g, b)))),
                                _                                    => unknown_csi!(self, cpy)
                            },
                            _ => unknown_csi!(self, cpy)
                        };
                        
                        if self.buf.len() == 0 {
                            self.state = ParserState::Default;
                        }
                        
                        return r;
                    },
                    ParserState::OSC => match next_char!(self) {
                        0x5C if self.buf.iter().last() == Some(&0x1B) => {
                            /* Remove ESC */
                            self.buf.pop();
                            
                            parse_osc!(self)
                        },
                        0x07 => parse_osc!(self),
                        c    => self.buf.push(c)
                    }
                }
            }
        }
    }
    
    pub fn new_parser<T: io::Read>(r: T) -> Parser<T> {
        Parser{
            src:   r.bytes(),
            state: ParserState::Default,
            buf:   Vec::new(),
        }
    }
}

fn execvp(cmd: &str, params: &[&str]) -> std::io::Error {
    use libc::execvp;
    use std::ffi::CString;
    use std::io::Error;
    use std::ptr;
    
    let cmd      = CString::new(cmd).unwrap();
    let mut args = params.iter().map(|&s| CString::new(s).unwrap().as_ptr()).collect::<Vec<*const i8>>();
    
    args.push(ptr::null());
    
    unsafe {
        execvp(cmd.as_ptr(), args.as_mut_ptr());
    }
    
    Error::last_os_error()
}

extern fn interrupt(_:i32) {
    print!("interrupt\n");
}

fn run_sh(m: pty::Fd, s: pty::Fd) {
    use libc;
    use std::process;
    
    /* Get rid of the master fd before running the shell */
    drop(m);
    
    s.override_fd(libc::STDIN_FILENO).unwrap();
    s.override_fd(libc::STDOUT_FILENO).unwrap();
    s.override_fd(libc::STDERR_FILENO).unwrap();
    
    /* This will never return unless the shell command exits with error */
    print!("{}", execvp("zsh", &["-i"]));
    
    process::exit(-1);
}
#[derive(Copy, Clone)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 3],
}

implement_vertex!(Vertex, position, color);

fn main() {
    use clock_ticks;
    use libc;
    use std::io;
    use std::process;
    use std::thread;
    use glium::DisplayBuild;
use glium::index;
use glium::Surface;
    
    let (mut m, s) = pty::open().unwrap();
    
    match unsafe { libc::fork() } {
        -1   => panic!(io::Error::last_os_error()),
         0   => run_sh(m, s),
         pid => {
            print!("master, child pid: {}\n", pid);
            
            unsafe {
                libc::funcs::posix01::signal::signal(17, interrupt as u64);
            }
            
            m.set_noblock();
            
            
            print!("Starting");
            
            let mut p   = ctrl::new_parser(io::BufReader::with_capacity(100, m));
            let display = glutin::WindowBuilder::new().with_gl(glutin::GlRequest::Specific(glutin::Api::OpenGl, (3, 3))).build_glium().unwrap();

let vertex_buffer = glium::VertexBuffer::new(&display, vec![
    Vertex { position: [-0.5, -0.5], color: [0.0, 1.0, 0.0] },
    Vertex { position: [ 0.0,  0.5], color: [0.0, 0.0, 1.0] },
    Vertex { position: [ 0.5, -0.5], color: [1.0, 0.0, 0.0] },
]);
let indices = index::NoIndices(index::PrimitiveType::TrianglesList);
let program = glium::Program::from_source(&display,
    // vertex shader
    "   #version 410

        uniform mat4 matrix;

        in vec2 position;
        in vec3 color;

        out vec3 v_color;

        void main() {
            gl_Position = vec4(position, 0.0, 1.0) * matrix;
            v_color = color;
        }
    ",

    // fragment shader
    "   #version 410
        in vec3 v_color;
        out vec4 FragColor;

        void main() {
            FragColor = vec4(v_color, 1.0);
        }
    ",

    // optional geometry shader
    None
).unwrap();
let uniforms = uniform! {
    matrix: [
        [ 1.0, 0.0, 0.0, 0.0 ],
        [ 0.0, 1.0, 0.0, 0.0 ],
        [ 0.0, 0.0, 1.0, 0.0 ],
        [ 0.0, 0.0, 0.0, 1.0 ]
    ]
};
            
            display.get_window().map(|w| w.set_title("rust_term"));
            
            unsafe { display.get_window().map(|w| w.make_current()); };
            
            let mut accumulator    = 0;
            let mut previous_clock = clock_ticks::precise_time_ns();
            
            loop {
                let now = clock_ticks::precise_time_ns();
                accumulator += now - previous_clock;
                previous_clock = now;
                const FIXED_TIME_STAMP: u64 = 16666667;
                
                while accumulator >= FIXED_TIME_STAMP {
                    accumulator -= FIXED_TIME_STAMP;
                    
                    loop {
                        match p.next() {
                            Some(ctrl::Seq::SetWindowTitle(ref title)) => {
                                display.get_window().map(|w| w.set_title(title));
                            },
                            Some(c) => println!("> {:?}", c),
                            None    => break
                        }
                    }
                    
let mut target = display.draw();
target.clear_color(0.0, 0.0, 0.0, 0.0);  // filling the output with the black color
target.draw(&vertex_buffer, &indices, &program, &uniforms,
            &std::default::Default::default()).unwrap();
target.finish();
                    // display.get_window().map(|w| w.swap_buffers());
                    
                    for i in display.poll_events() {
                        println!("w {:?}", i)
                    }
                    
                    if display.is_closed() {
                        process::exit(0);
                    }
                }
                
                thread::sleep_ms(((FIXED_TIME_STAMP - accumulator) / 1000000) as u32);
            }
         }
    }
}
