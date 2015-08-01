use ft;
use glium;
use gl;
use std::cmp;
use std::fmt;
use std::collections;
use std::marker::PhantomData;

/// Converts a monochrome bitmap where every bit represents a filled or empty pixel
/// to a grayscale bitmap where every byte represents a pixel.
fn monochrome_to_grayscale(bitmap: &ft::bitmap::Bitmap) -> Vec<u8> {
    let grayscale_size = (bitmap.rows() * bitmap.width()) as usize;

    assert_eq!(bitmap.buffer().len(), (bitmap.rows() * bitmap.pitch().abs()) as usize);

    let mut bytes = Vec::with_capacity(grayscale_size);

    let bytes_per_row = bitmap.pitch().abs();

    for (i, b) in bitmap.buffer().iter().enumerate() {
        // Make sure we skip the padding at the end of every row in case width != pitch * 8:
        let end = cmp::max(0, cmp::min(8, (bitmap.width() - (i as i32 % bytes_per_row * 8)))) as u8;

        for i in 0..end {
            bytes.push(255 * (b >> (7u8 - i) & 1));
        }
    }

    assert_eq!(bytes.len(), (bitmap.rows() * bitmap.width()) as usize);

    bytes
}

#[derive(Debug)]
pub enum Error {
    FtError(ft::Error),
    MissingMetrics(usize)
}

impl From<ft::Error> for Error {
    fn from(err: ft::Error) -> Error {
        Error::FtError(err)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::FtError(err) => err.fmt(f),
            Error::MissingMetrics(glyph) => write!(f, "glyph {} is missing metrics", glyph)
        }
    }
}

/// Trait for rendering glyphs to 2D-textures.
pub trait Renderer<P: glium::texture::PixelValue + Clone> {
    fn render<F>(&mut self, glyph: usize, f: F) -> Result<(), Error>
      where F: FnOnce(glium::texture::RawImage2d<P>, gl::Padding) -> Result<(), Error>;
    /// Returns the (width, height) of a glyph cell in pixels.
    fn cell_size(&self) -> (u32, u32);
}

pub enum FreeTypeMode{
    Monochrome,
    Greyscale
}

/// A renderer using the FreeType library to render glyphs.
pub struct FreeType<'a> {
    ft_face:     ft::Face<'a>,
    render_mode: FreeTypeMode,
    glyphsize:   (u32, u32)
}

/// Truncates a freetype 16.6 fixed-point values to the integer value.
fn ft_to_pixels(fixed_float: i64) -> i32 {
    (fixed_float >> 6) as i32
}

impl<'a> FreeType<'a> {
    pub fn new(ft_face: ft::Face<'a>, mode: FreeTypeMode) -> Self {
        // FIXME: Use try!
        let ft_metrics = ft_face.size_metrics().expect("Could not load size metrics from font face");
        let width      = (ft_to_pixels(ft_metrics.max_advance) + 1) as u32;
        let height     = (ft_to_pixels(ft_metrics.height) + 1) as u32;

        FreeType{
            ft_face:     ft_face,
            render_mode: mode,
            glyphsize:   (width, height),
        }
    }
}

impl<'a> Renderer<u8> for FreeType<'a> {
    fn render<F>(&mut self, glyph: usize, f: F) -> Result<(), Error>
      where F: FnOnce(glium::texture::RawImage2d<u8>, gl::Padding) -> Result<(), Error> {
        use std::borrow::Cow;
        use glium::texture;

        let target = match self.render_mode {
            FreeTypeMode::Monochrome => ft::face::RENDER | ft::face::TARGET_MONO,
            FreeTypeMode::Greyscale  => ft::face::RENDER,
        };

        try!(self.ft_face.load_char(glyph, target));

        let g = self.ft_face.glyph();
        let glyph_bitmap = g.bitmap();

        let metrics  = try!(self.ft_face.size_metrics().ok_or(Error::MissingMetrics(glyph)));
        let height   = ft_to_pixels(metrics.height);
        let ascender = ft_to_pixels(metrics.ascender);
        let advance  = ft_to_pixels(g.advance().x);

        let left   = cmp::max(0, g.bitmap_left() as u32);
        let top    = cmp::max(0, ascender - g.bitmap_top());
        let right  = cmp::max(0, advance - g.bitmap_left() - glyph_bitmap.width());
        let bottom = cmp::max(0, height - top - glyph_bitmap.rows());

        let texdata = match self.render_mode {
            FreeTypeMode::Monochrome => Cow::Owned(monochrome_to_grayscale(&glyph_bitmap)),
            FreeTypeMode::Greyscale  => Cow::Borrowed(glyph_bitmap.buffer()),
        };

        f(texture::RawImage2d{
            data:   texdata,
            width:  glyph_bitmap.width() as u32,
            height: glyph_bitmap.rows()  as u32,
            format: texture::ClientFormat::U8
        },
        gl::Padding{
            left:   left   as u32,
            top:    top    as u32,
            right:  right  as u32,
            bottom: bottom as u32
        })
    }

    fn cell_size(&self) -> (u32, u32) {
        self.glyphsize
    }
}

#[derive(Debug)]
pub struct Map<'a, F, R>
  where F: 'a + glium::backend::Facade, R: 'a + Renderer<u8> {
    renderer: R,
    glyphs:   collections::BTreeMap<usize, glium::Rect>,
    atlas:    gl::Atlas<'a, F>
}

impl<'a, F, R> Map<'a, F, R>
  where F: 'a + glium::backend::Facade, R: 'a + Renderer<u8> {
    pub fn new(display: &'a F, renderer: R) -> Self {
        Map::new_with_size(display, renderer, 1000)
    }

    pub fn new_with_size(display: &'a F, renderer: R, atlas_size: u32) -> Self {
        Map {
            renderer: renderer,
            glyphs:   collections::BTreeMap::new(),
            atlas:    gl::Atlas::new(display, atlas_size, atlas_size),
        }
    }

    pub fn load(&mut self, glyph: usize) -> Result<(), Error> {
        let mut renderer = &mut self.renderer;
        let glyphs   = &mut self.glyphs;
        let atlas    = &mut self.atlas;

        if glyphs.contains_key(&glyph) {
            return Ok(())
        }

        renderer.render(glyph, |texture, padding| {
            let r = atlas.add_with_padding(texture, padding);

            glyphs.insert(glyph, r);

            Ok(())
        })
    }

    /// Retrieves a specific glyph if it exists.
    pub fn get<'b>(&'b self, glyph: usize) -> Option<Glyph<'b>> {
        self.glyphs.get(&glyph).map(|g| {
            let (width, height) = self.atlas.texture_size();

            Glyph {
                left:    (g.left)              as f32 / width  as f32,
                right:   (g.left + g.width)    as f32 / width  as f32,
                bottom:  (g.bottom)            as f32 / height as f32,
                top:     (g.bottom + g.height) as f32 / height as f32,
                width:   g.width,
                height:  g.height,
                phantom: PhantomData,
            }
        })
    }

    pub fn texture(&self) -> &glium::Texture2d {
        self.atlas.texture()
    }
}

#[derive(Copy, Clone, Debug)]
pub struct TexturedVertex {
    /// Vertex coordinates [left, bottom]
    xy:  [f32; 2],
    rgb: [f32; 3],
    /// Texture coordinates [left, bottom]
    st:  [f32; 2],
}

implement_vertex!(TexturedVertex, xy, rgb, st);

#[derive(Copy, Clone, Debug)]
pub struct Glyph<'a> {
    /// Distance from left side of texture to left side of glyph area. [0-1]
    left:       f32,
    /// Distance from left side of texture to right side of glyph area. [0-1]
    right:      f32,
    /// Distance from bottom side of texture to bottom side of glyph area. [0-1]
    bottom:     f32,
    /// Distance from bottom side of texture to top side of glyph area. [0-1]
    top:        f32,
    /// Glyph texture width in pixels.
    pub width:  u32,
    /// Glyph texture height in pixels.
    pub height: u32,
    /// Guard to prevent resizing of the texture which these fractional values point at.
    phantom:    PhantomData<&'a usize>,
}

impl<'a> Glyph<'a> {
    /// Returns a list of vertices for two triangles making up the quad for this texture.
    /// 
    /// ``p`` is the position of the lower-left corner of the quad, ``s`` is the width and
    /// height of the quad. ``rgb`` is the foreground RGB color.
    pub fn vertices(&self, p: (f32, f32), s: (f32, f32), rgb: [f32; 3]) -> [TexturedVertex; 6] {
        // vertex positions
        let l =  p.0        as f32;
        let r = (p.0 + s.0) as f32;
        let b =  p.1        as f32;
        let t = (p.1 + s.1) as f32;

        [
            TexturedVertex { xy: [l, b], rgb: rgb, st: [self.left,  self.top   ] },
            TexturedVertex { xy: [l, t], rgb: rgb, st: [self.left,  self.bottom] },
            TexturedVertex { xy: [r, t], rgb: rgb, st: [self.right, self.bottom] },

            TexturedVertex { xy: [r, t], rgb: rgb, st: [self.right, self.bottom] },
            TexturedVertex { xy: [r, b], rgb: rgb, st: [self.right, self.top   ] },
            TexturedVertex { xy: [l, b], rgb: rgb, st: [self.left,  self.top   ] },
        ]
    }
}
