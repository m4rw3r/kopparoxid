use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::Receiver;

use cu2o_gl::glyph::Error as GlyphError;
use cu2o_gl::glyph::{FreeType, FreeTypeConfig, Map, MapError, Renderer};
use cu2o_gl::{GlTerm, FontStyle};
use cu2o_loop::Message;
use cu2o_term::Term;
use cu2o_term::color::Manager;
use freetype::Error as FtError;
use freetype::Library as FtLibrary;
use glium::backend::Facade;
use glium::{Display, DisplayBuild};
use glutin::Api::OpenGl;
use glutin::{Event, GlRequest, WindowBuilder};
use mio::Sender;
use time::{Duration, PreciseTime};

pub use glutin::WindowProxy;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Quit,
}

#[derive(Clone, Debug)]
pub enum Error {
    FreeTypeError(PathBuf, FtError),
    FontError(PathBuf, GlyphError),
    MapError(MapError<FontStyle>),
}

impl From<MapError<FontStyle>> for Error {
    fn from(e: MapError<FontStyle>) -> Self {
        Error::MapError(e)
    }
}

#[derive(Debug)]
pub struct Font<'a> {
    pub path:   &'a Path,
    pub size:   u32,
    pub config: FreeTypeConfig,
}

impl<'a> Font<'a> {
    pub fn new<P: AsRef<Path> + ?Sized>(p: &'a P, size: u32, config: FreeTypeConfig) -> Font {
        Font {
            path:   p.as_ref(),
            size:   size,
            config: config,
        }
    }
}

#[derive(Debug)]
pub struct FontFaces<'a> {
    pub regular:     Font<'a>,
    pub bold:        Option<Font<'a>>,
    pub italic:      Option<Font<'a>>,
    pub bold_italic: Option<Font<'a>>,
}

fn load_font<'a, 'b>(f: &'a mut FtLibrary, font: Font<'b>, scale: f32) -> Result<Box<Renderer<u8> + 'static>, Error> {
    let ft_face = try!(f.new_face(font.path, 0).map_err(|e| Error::FreeTypeError(font.path.to_owned(), e)));

    try!(ft_face.set_pixel_sizes(0, (font.size as f32 * scale) as u32).map_err(|e| Error::FreeTypeError(font.path.to_owned(), e)));

    Ok(Box::new(try!(FreeType::new(ft_face, font.config)
                     .map_err(|e| Error::FontError(font.path.to_owned(), e)))))
}

impl<'a> FontFaces<'a> {
    pub fn load_fonts<'b, 'c>(self, f: &'c mut FtLibrary, m: &'b mut Map<FontStyle>, scale: f32) -> Result<(), Error> {
        use cu2o_gl::FontStyle::*;

        try!(m.add_renderer(Regular, try!(load_font(f, self.regular, scale))));

        if let Some(bold) = self.bold {
            try!(m.add_renderer(Bold, try!(load_font(f, bold, scale))));
        }

        if let Some(italic) = self.italic {
            try!(m.add_renderer(Italic, try!(load_font(f, italic, scale))));
        }

        if let Some(bold_italic) = self.bold_italic {
            try!(m.add_renderer(BoldItalic, try!(load_font(f, bold_italic, scale))));
        }

        Ok(())
    }
}

pub struct Window<C>
  where C: Manager {
    /// Glium window and OpenGl context
    display: Display,
    msgs:    Receiver<Action>,
    /// Terminal renderer
    gl:      GlTerm<C>,
}

impl<C> Window<C>
  where C: Manager {
    // TODO: Result
    pub fn new(faces: FontFaces, colors: C, msgs: Receiver<Action>) -> Self {
        info!("creating window");

        let display = WindowBuilder::new()
            .with_gl(GlRequest::Specific(OpenGl, (3, 3)))
            .with_srgb(Some(true))
            .build_glium()
            .unwrap();

        let ctx        = display.get_context().clone();
        let mut ft_lib = FtLibrary::init().unwrap();
        let mut f_map  = Map::new(ctx.clone());
        let scale      = display.get_window().map(|w| w.hidpi_factor()).unwrap_or(1.0);

        faces.load_fonts(&mut ft_lib, &mut f_map, scale).unwrap();

        Window {
            display: display,
            msgs:    msgs,
            gl:      GlTerm::new(ctx, colors, f_map).unwrap(),
        }
    }

    // TODO: Result
    pub fn create_proxy(&self) -> WindowProxy {
        self.display.get_window().unwrap().create_window_proxy()
    }

    pub fn run(&mut self, terminal: Arc<Mutex<Term>>, msg: Sender<Message>) {
        unsafe { self.display.get_window().unwrap().make_current().unwrap() };
        self.display.get_window().unwrap().show();

        let cell        = self.gl.cell_size();
        let mut bufsize = self.display.get_framebuffer_dimensions();

        msg.send(Message::Resize{
            width:  bufsize.0 / cell.0,
            height: bufsize.1 / cell.1,
            x:      bufsize.0,
            y:      bufsize.1,
        }).unwrap();

        info!("Window: starting event loop");

        let mut counter = FpsCounter::new();

        for i in self.display.wait_events() {
            match i {
                Event::Closed               => {
                    msg.send(Message::Exit).unwrap();

                    break;
                },
                // TODO: Proper keyboard handling
                Event::ReceivedCharacter(c) => msg.send(Message::Character(c)).unwrap(),
                Event::Focused(got_focus)   => msg.send(Message::Focus(got_focus)).unwrap(),
                Event::MouseMoved(_)        => {},
                Event::Awakened             => {
                    // We ignore errors (senders disconnected, channel empty)
                    match self.msgs.try_recv().ok() {
                        Some(Action::Quit) => {
                            info!("Received Action::Quit, exiting window thread");

                            break;
                        },
                        // No event, continue with render nothing
                        None => {},
                    }

                    info!("Window: rendering");

                    let new_bufsize = self.display.get_framebuffer_dimensions();

                    // OS X does not fire glutin::Event::Resize from poll_events(), need to check manually
                    // TODO: Proper resize handling
                    if new_bufsize != bufsize {
                        bufsize = new_bufsize;

                        msg.send(Message::Resize{
                            width:  bufsize.0 / cell.0,
                            height: bufsize.1 / cell.1,
                            x:      bufsize.0,
                            y:      bufsize.1,
                        }).unwrap();
                    }

                    // TODO: Measure frame time
                    let mut target = self.display.draw();

                    {
                        let t = terminal.lock().expect("term::Term mutex poisoned");

                        self.gl.load_vertices(&t);

                        self.display.get_window().map(|w| w.set_title(t.get_title()));
                    }

                    let width_offset  = 1.0 * (bufsize.0 % cell.0) as f32 / bufsize.0 as f32;
                    let height_offset = 1.0 * (bufsize.1 % cell.1) as f32 / bufsize.1 as f32;

                    self.gl.draw(&mut target, bufsize, (width_offset, height_offset));

                    target.finish().unwrap();

                    counter.increment();
                },
                // TODO: More events
                _ => {}, // println!("w {:?}", i)
            }
        }

        info!("window loop exiting");
    }
}

struct FpsCounter {
    count:      u64,
    last_reset: PreciseTime,
}

impl FpsCounter {
    fn new() -> Self {
        FpsCounter {
            count: 0,
            last_reset: PreciseTime::now(),
        }
    }

    fn increment(&mut self) {
        let t    = PreciseTime::now();
        let diff = self.last_reset.to(t);

        if  diff >= Duration::seconds(1) {
            info!("FPS: {}", self.count as f64 / (diff.num_microseconds().unwrap_or(1_000_000) as f64 / 1_000_000.0));

            self.count      = 0;
            self.last_reset = t;
        }

        self.count += 1;
    }
}


