use glium;
use std::cmp;
use std::fmt;
use std::collections;
use std::marker::PhantomData;
use ctrl;
use ft;

/// The growth factor for the atlas.
const ATLAS_GROWTH_FACTOR: u32 = 2;

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

pub struct Padding {
    left:   u32,
    top:    u32,
    right:  u32,
    bottom: u32,
}

#[derive(Debug)]
pub struct Atlas<'a, F> where F: 'a + glium::backend::Facade {
    context:    &'a F,
    used:       (u32, u32),
    row_height: u32,
    texture:    glium::texture::Texture2d,
}

/// Somewhat basic automatically resizing texture-atlas.
impl<'a, F> Atlas<'a, F> where F: 'a + glium::backend::Facade {
    pub fn new(facade: &'a F, width: u32, height: u32) -> Self {
        use glium::Surface;

        let tex = glium::Texture2d::empty_with_format(facade, glium::texture::UncompressedFloatFormat::U8, glium::texture::MipmapsOption::NoMipmap, width, height).unwrap();

        tex.as_surface().clear(None, Some((0.0, 0.0, 0.0, 0.0)), None, None);

        Atlas {
            context:    facade,
            used:       (0, 0),
            row_height: 0,
            texture:    tex,
        }
    }

    pub fn add<P: glium::texture::PixelValue + Clone>(&mut self, raw: glium::texture::RawImage2d<P>) -> glium::Rect {
        self.add_with_padding(raw, Padding{left: 0, top: 0, right: 0, bottom: 0})
    }

    pub fn add_with_padding<P: glium::texture::PixelValue + Clone>(&mut self, raw: glium::texture::RawImage2d<P>, padding: Padding) -> glium::Rect {
        let req_size     = (padding.left + raw.width + padding.right, padding.top + raw.height + padding.bottom);
        let cur_size     = (self.texture.get_width(), self.texture.get_height().unwrap_or(1));
        let mut new_size = cur_size;

        // Extend width if necessary
        while req_size.0 > new_size.0 {
            new_size.0 = new_size.0 * ATLAS_GROWTH_FACTOR;
        }

        // Have we used up this row? If so, end it and create a new one
        if self.used.0 + req_size.0 > new_size.0 {
            self.used.0     = 0;
            self.used.1     = self.used.1 + self.row_height;
            self.row_height = 0;
        }

        // Extend height if necessary
        while self.used.1 + req_size.1 > new_size.1 {
            new_size.1 = new_size.1 * ATLAS_GROWTH_FACTOR;
        }

        if cur_size != new_size {
            use glium::Surface;

            let img: Vec<_> = self.texture.read();
            let h           = self.texture.get_height().unwrap_or(1);
            let w           = self.texture.get_width();

            self.texture = glium::Texture2d::empty_with_format(self.context, glium::texture::UncompressedFloatFormat::U8, glium::texture::MipmapsOption::NoMipmap, new_size.0, new_size.1).unwrap();

            self.texture.as_surface().clear(None, Some((0.0, 0.0, 0.0, 0.0)), None, None);

            self.texture.write(glium::Rect{
                left:   0,
                bottom: 0,
                height: h,
                width:  w,
            }, img);
        }

        self.texture.write(glium::Rect{
            left:   self.used.0 + padding.left,
            bottom: self.used.1 + padding.top,
            height: raw.height,
            width:  raw.width,
        }, raw);

        let r = glium::Rect{
            left:   self.used.0,
            bottom: self.used.1,
            height: req_size.1,
            width:  req_size.0,
        };

        self.used.0     = self.used.0 + req_size.0;
        self.row_height = cmp::max(self.row_height, req_size.1);

        r
    }

    pub fn texture(&self) -> &glium::Texture2d {
        &self.texture
    }

    pub fn texture_size(&self) -> (u32, u32) {
        (self.texture.get_width(), self.texture.get_height().unwrap_or(1))
    }
}

#[derive(Debug)]
pub enum GlyphError {
    FtError(ft::Error),
    MissingGlyphMetrics(usize)
}

impl From<ft::Error> for GlyphError {
    fn from(err: ft::Error) -> GlyphError {
        GlyphError::FtError(err)
    }
}

impl fmt::Display for GlyphError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            GlyphError::FtError(err) => err.fmt(f),
            GlyphError::MissingGlyphMetrics(glyph) => write!(f, "glyph {} is missing metrics", glyph)
        }
    }
}

pub trait GlyphRenderer<P: glium::texture::PixelValue + Clone> {
    fn render<F>(&mut self, glyph: usize, f: F) -> Result<(), GlyphError>
      where F: FnOnce(glium::texture::RawImage2d<P>, Padding) -> Result<(), GlyphError>;
}

pub struct FTMonoGlyphRenderer<'a> {
    ft_face: ft::Face<'a>
}

impl<'a> FTMonoGlyphRenderer<'a> {
    pub fn new(ft_face: ft::Face<'a>) -> Self {
        FTMonoGlyphRenderer{
            ft_face: ft_face,
        }
    }
}

impl<'a> GlyphRenderer<u8> for FTMonoGlyphRenderer<'a> {
    fn render<F>(&mut self, glyph: usize, f: F) -> Result<(), GlyphError>
      where F: FnOnce(glium::texture::RawImage2d<u8>, Padding) -> Result<(), GlyphError> {
        use std::borrow::Cow;
        use glium::texture;

        try!(self.ft_face.load_char(glyph, ft::face::RENDER | ft::face::TARGET_MONO));

        let g = self.ft_face.glyph();
        let glyph_bitmap = g.bitmap();

        let metrics  = try!(self.ft_face.size_metrics().ok_or(GlyphError::MissingGlyphMetrics(glyph)));
        let height   = (metrics.height   >> 6) as i32;
        let ascender = (metrics.ascender >> 6) as i32;
        let advance  = (g.advance().x    >> 6) as i32;

        let left   = cmp::max(0, g.bitmap_left() as u32);
        let top    = cmp::max(0, ascender - g.bitmap_top());
        let right  = cmp::max(0, advance - g.bitmap_left() - glyph_bitmap.width());
        let bottom = cmp::max(0, height - top - glyph_bitmap.rows());

        f(texture::RawImage2d{
            data:   Cow::Owned(monochrome_to_grayscale(&glyph_bitmap)),
            width:  glyph_bitmap.width() as u32,
            height: glyph_bitmap.rows()  as u32,
            format: texture::ClientFormat::U8
        },
        Padding{
            left:   left   as u32,
            top:    top    as u32,
            right:  right  as u32,
            bottom: bottom as u32
        })
    }
}

pub struct FTGrayscaleGlyphRenderer<'a> {
    ft_face: ft::Face<'a>
}

impl<'a> FTGrayscaleGlyphRenderer<'a> {
    pub fn new(ft_face: ft::Face<'a>) -> Self {
        FTGrayscaleGlyphRenderer{
            ft_face: ft_face,
        }
    }
}

impl<'a> GlyphRenderer<u8> for FTGrayscaleGlyphRenderer<'a> {
    fn render<F>(&mut self, glyph: usize, f: F) -> Result<(), GlyphError>
      where F: FnOnce(glium::texture::RawImage2d<u8>, Padding) -> Result<(), GlyphError> {
        use std::borrow::Cow;
        use glium::texture;

        try!(self.ft_face.load_char(glyph, ft::face::RENDER));

        let g = self.ft_face.glyph();
        let glyph_bitmap = g.bitmap();

        let metrics  = try!(self.ft_face.size_metrics().ok_or(GlyphError::MissingGlyphMetrics(glyph)));
        let height   = (metrics.height   >> 6) as i32;
        let ascender = (metrics.ascender >> 6) as i32;
        let advance  = (g.advance().x    >> 6) as i32;

        let left   = cmp::max(0, g.bitmap_left() as u32);
        let top    = cmp::max(0, ascender - g.bitmap_top());
        let right  = cmp::max(0, advance - g.bitmap_left() - glyph_bitmap.width());
        let bottom = cmp::max(0, height - top - glyph_bitmap.rows());

        f(texture::RawImage2d{
            data:   Cow::Borrowed(glyph_bitmap.buffer()),
            width:  glyph_bitmap.width() as u32,
            height: glyph_bitmap.rows() as u32,
            format: texture::ClientFormat::U8
        },
        Padding{
            left:   left   as u32,
            top:    top    as u32,
            right:  right  as u32,
            bottom: bottom as u32
        })
    }
}

#[derive(Debug)]
pub struct GlyphMap<'a, F, R>
  where F: 'a + glium::backend::Facade, R: 'a + GlyphRenderer<u8> {
    renderer: R,
    glyphs:   collections::BTreeMap<usize, glium::Rect>,
    atlas:    Atlas<'a, F>
}

impl<'a, F, R> GlyphMap<'a, F, R>
  where F: 'a + glium::backend::Facade, R: 'a + GlyphRenderer<u8> {
    pub fn new(display: &'a F, renderer: R) -> Self {
        GlyphMap::new_with_size(display, renderer, 1000)
    }

    pub fn new_with_size(display: &'a F, renderer: R, atlas_size: u32) -> Self {
        GlyphMap {
            renderer: renderer,
            glyphs:   collections::BTreeMap::new(),
            atlas:    Atlas::new(display, atlas_size, atlas_size),
        }
    }

    pub fn load(&mut self, glyph: usize) -> Result<(), GlyphError> {
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
    xy:     [f32; 2],
    fg_rgb: [f32; 3],
    bg_rgb: [f32; 3],
    /// Texture coordinates [left, bottom]
    st:     [f32; 2],
}

implement_vertex!(TexturedVertex, xy, fg_rgb, bg_rgb, st);

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
    /// height of the quad. ``fg`` is the foreground RGB color and ``bg`` is the background
    /// color.
    pub fn vertices(&self, p: (f32, f32), s: (f32, f32), fg: [f32; 3], bg: [f32; 3]) -> [TexturedVertex; 6] {
        // vertex positions
        let l =  p.0        as f32;
        let r = (p.0 + s.0) as f32;
        let b =  p.1        as f32;
        let t = (p.1 + s.1) as f32;

        [
            TexturedVertex { xy: [l, b], fg_rgb: fg, bg_rgb: bg, st: [self.left,  self.top   ] },
            TexturedVertex { xy: [l, t], fg_rgb: fg, bg_rgb: bg, st: [self.left,  self.bottom] },
            TexturedVertex { xy: [r, t], fg_rgb: fg, bg_rgb: bg, st: [self.right, self.bottom] },

            TexturedVertex { xy: [r, t], fg_rgb: fg, bg_rgb: bg, st: [self.right, self.bottom] },
            TexturedVertex { xy: [r, b], fg_rgb: fg, bg_rgb: bg, st: [self.right, self.top   ] },
            TexturedVertex { xy: [l, b], fg_rgb: fg, bg_rgb: bg, st: [self.left,  self.top   ] },
        ]
    }
}
