use std::path::Path;
use std::process;
use std::sync::{Arc, Mutex};

use event_loop::Message;
use gl::glyph::{self, FreeType, FreeTypeConfig, Map, Renderer};
use gl::glyph::Error as GlyphError;
use gl::term::{GlTerm, FontStyle};
use glium::{Display, DisplayBuild};
use glium::backend::Facade;
use glutin::{Event, GlRequest, WindowBuilder, WindowProxy};
use glutin::Api::OpenGl;
use term::color::Manager;
use time::{Duration, PreciseTime};
use ft::{self, FtResult};
use term::Term;
use mio::Sender;

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

fn load_font<'a, 'b>(f: &'a mut ft::Library, font: Font<'b>, scale: f32) -> FtResult<Box<Renderer<u8> + 'static>> {
    let ft_face = try!(f.new_face(font.path, 0));

    try!(ft_face.set_pixel_sizes(0, (font.size as f32 * scale) as u32));

    // TODO: Antialiasing and hinting settings
    Ok(Box::new(FreeType::new(ft_face, font.config)))
}

impl<'a> FontFaces<'a> {
    pub fn load_fonts<'b, 'c>(self, f: &'c mut ft::Library, m: &'b mut Map<FontStyle>, scale: f32) -> Result<(), GlyphError> {
        use gl::term::FontStyle::*;

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
    /// Terminal renderer
    gl:      GlTerm<C>,
}

impl<C> Window<C>
  where C: Manager {
    // TODO: Result
    pub fn new(faces: FontFaces, colors: C) -> Self {
        info!("creating window");

        let display = WindowBuilder::new()
            .with_gl(GlRequest::Specific(OpenGl, (3, 3)))
            .with_srgb(Some(true))
            .build_glium()
            .unwrap();

        let ctx        = display.get_context().clone();
        let mut ft_lib = ft::Library::init().unwrap();
        let mut f_map  = glyph::Map::new(ctx.clone());
        let scale      = display.get_window().map(|w| w.hidpi_factor()).unwrap_or(1.0);

        faces.load_fonts(&mut ft_lib, &mut f_map, scale).unwrap();

        Window {
            display: display,
            gl:      GlTerm::new(ctx, colors, f_map).unwrap(),
        }
    }

    // TODO: Result
    pub fn create_proxy(&self) -> WindowProxy {
        self.display.get_window().unwrap().create_window_proxy()
    }

    pub fn run(&mut self, terminal: Arc<Mutex<Term>>, msg: Sender<Message>) {
        unsafe { self.display.get_window().map(|w| w.make_current()); };

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
                Event::Closed               => process::exit(0),
                // TODO: Proper keyboard handling
                Event::ReceivedCharacter(c) => msg.send(Message::Character(c)).unwrap(),
                Event::Focused(got_focus)   => msg.send(Message::Focus(got_focus)).unwrap(),
                Event::MouseMoved(_)        => {},
                Event::Awakened             => {
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
                        // TODO: See if it is possible (and might make a difference!) to lock the
                        // terminal data for a smaller piece of code (ie. just make the buffers, not
                        // actually call the draw calls)
                        let t = terminal.lock().unwrap();

                        self.display.get_window().map(|w| w.set_title(t.get_title()));

                        let width_offset  = 1.0 * (bufsize.0 % cell.0) as f32 / bufsize.0 as f32;
                        let height_offset = 1.0 * (bufsize.1 % cell.1) as f32 / bufsize.1 as f32;

                        self.gl.draw(&mut target, &t, bufsize, (width_offset, height_offset));
                    }

                    target.finish().unwrap();

                    counter.increment();
                },
                // TODO: More events
                _ => {}, // println!("w {:?}", i)
            }
        }
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


