use std::cmp;
use std::rc::Rc;
use std::collections;
use std::fmt;
use std::marker::PhantomData;

use ft;
use glium::{Texture2d, Rect};
use glium::backend::Context;
use glium::texture::{ClientFormat, MipmapsOption, PixelValue, RawImage2d, UncompressedFloatFormat};

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
    MissingMetrics(usize),
    UnknownRenderer,
    DuplicateRendererKey,
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
            Error::MissingMetrics(glyph) => write!(f, "glyph {} is missing metrics", glyph),
            Error::UnknownRenderer => write!(f, "unkown render key"),
            Error::DuplicateRendererKey => write!(f, "key already exists"),
        }
    }
}

/// Glyph padding in pixels, distance outisde the draw area which is supposed to be empty.
#[derive(Debug, Copy, Clone)]
pub struct Padding {
    pub left:   u32,
    pub top:    u32,
    pub right:  u32,
    pub bottom: u32,
}

/// Glyph metrics in pixels
#[derive(Debug, Copy, Clone)]
pub struct Metrics {
    pub padding: Padding,
    pub height:  u32,
    pub width:   u32,
}

#[derive(Debug, Copy, Clone)]
struct GlyphData {
    padding:  Padding,
    tex_rect: Rect,
}

/// Trait for rendering glyphs to 2D-textures.
pub trait Renderer<P: PixelValue + Clone>: fmt::Debug {
    fn render(&mut self, glyph: usize, f: &mut FnMut(RawImage2d<P>, Padding) -> Result<(), Error>) -> Result<(), Error>;
    /// Returns the (width, height) of a glyph cell in pixels.
    fn cell_size(&self) -> (u32, u32);
}

/// The type of hinting used for FreeType font rendering.
///
/// Default is `autohint=false, light=false`, ie. prefer the font's own hinting.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct HintMode {
    /// If to force automatic hinting (`FT_LOAD_FORCE_AUTOHINT`).
    pub autohint: bool,
    /// If to use a lighter hinting algorithm (`FT_LOAD_TARGET_LIGHT`).
    pub light:    bool,
}

impl Default for HintMode {
    fn default() -> Self {
        HintMode {
            autohint: false,
            light:    false,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct FreeTypeConfig {
    /// Render mode, true=grayscale and false=monochrome rendering. Corresponds to antialiasing.
    pub antialias: bool,
    /// If to use hinting
    pub hinting:   Option<HintMode>,

}

/// A renderer using the FreeType library to render glyphs.
pub struct FreeType<'a> {
    ft_face:     ft::Face<'a>,
    render_mode: FreeTypeConfig,
    glyphsize:   (u32, u32)
}

impl<'a> fmt::Debug for FreeType<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "FreeType({:?}, {:?})", self.render_mode, self.glyphsize)
    }
}

/// Truncates a freetype 16.6 fixed-point values to the integer value.
fn ft_to_pixels(fixed_float: i64) -> i32 {
    (fixed_float >> 6) as i32
}

impl<'a> FreeType<'a> {
    pub fn new(ft_face: ft::Face<'a>, mode: FreeTypeConfig) -> Self {
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
    fn render(&mut self, glyph: usize, f: &mut FnMut(RawImage2d<u8>, Padding) -> Result<(), Error>) -> Result<(), Error> {
        use std::borrow::Cow;

        // Build target bitmask
        let target = ft::face::RENDER |
            if self.render_mode.antialias { ft::face::LoadFlag::empty() } else { ft::face::TARGET_MONO } |
            if let Some(hint) = self.render_mode.hinting {
                (if hint.autohint { ft::face::FORCE_AUTOHINT } else { ft::face::LoadFlag::empty() }) |
                (if hint.light    { ft::face::TARGET_LIGHT   } else { ft::face::LoadFlag::empty() })
            } else {
                ft::face::NO_HINTING
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

        let texdata = if self.render_mode.antialias {
            Cow::Borrowed(glyph_bitmap.buffer())
        } else {
            Cow::Owned(monochrome_to_grayscale(&glyph_bitmap))
        };

        f(RawImage2d{
            data:   texdata,
            width:  glyph_bitmap.width() as u32,
            height: glyph_bitmap.rows()  as u32,
            format: ClientFormat::U8
        },
        Padding{
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

pub struct Map<K>
  where K: Clone + Ord {
    renderers: collections::BTreeMap<K, Box<Renderer<u8> + 'static>>,
    glyphs:    collections::BTreeMap<(K, usize), GlyphData>,
    atlas:     Atlas
}

impl<K> Map<K>
  where K: Clone + Ord {
    #[inline]
    pub fn new(context: Rc<Context>) -> Self {
        Map::new_with_size(context, 1000)
    }

    #[inline]
    pub fn new_with_size(context: Rc<Context>, atlas_size: u32) -> Self {
        Map {
            renderers: collections::BTreeMap::new(),
            glyphs:    collections::BTreeMap::new(),
            atlas:     Atlas::new(context, atlas_size, atlas_size),
        }
    }

    pub fn add_renderer(&mut self, render_key: K, renderer: Box<Renderer<u8> + 'static>) -> Result<(), Error> {
        if self.renderers.contains_key(&render_key) {
            return Err(Error::DuplicateRendererKey);
        }

        self.renderers.insert(render_key, renderer);

        Ok(())
    }

    pub fn load(&mut self, render_key: K, glyph: usize) -> Result<(), Error> {
        let glyphs = &mut self.glyphs;
        let atlas  = &mut self.atlas;

        if glyphs.contains_key(&(render_key.clone(), glyph)) {
            return Ok(())
        }

        self.renderers.get_mut(&render_key)
            .ok_or(Error::UnknownRenderer)
            .and_then(|mut r|
            r.render(glyph, &mut move |texture, padding| {
                let r = atlas.add(texture);

                glyphs.insert((render_key.clone(), glyph), GlyphData { padding: padding, tex_rect: r });

                Ok(())
            })
        )
    }

    /// Retrieves a specific glyph if it exists.
    #[inline]
    pub fn get(&self, render_key: K, glyph: usize) -> Option<Glyph> {
        self.glyphs.get(&(render_key, glyph)).map(|d| {
            let g               = d.tex_rect;
            let (width, height) = self.atlas.texture_size();

            Glyph {
                left:    (g.left)              as f32 / width  as f32,
                right:   (g.left + g.width)    as f32 / width  as f32,
                bottom:  (g.bottom)            as f32 / height as f32,
                top:     (g.bottom + g.height) as f32 / height as f32,
                metrics: Metrics {
                    padding: d.padding,
                    width:   g.width,
                    height:  g.height,
                },
                phantom: PhantomData,
            }
        })
    }

    #[inline]
    pub fn texture(&self) -> &Texture2d {
        self.atlas.texture()
    }

    pub fn cell_size(&self) -> (u32, u32) {
        let mut size = (0, 0);

        for v in self.renderers.values() {
            let c = v.cell_size();

            size.0 = cmp::max(size.0, c.0);
            size.1 = cmp::max(size.1, c.1);
        }

        size
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

#[derive(Debug)]
pub struct Glyph<'a> {
    /// Distance from left side of texture to left side of glyph area. [0-1]
    left:       f32,
    /// Distance from left side of texture to right side of glyph area. [0-1]
    right:      f32,
    /// Distance from bottom side of texture to bottom side of glyph area. [0-1]
    bottom:     f32,
    /// Distance from bottom side of texture to top side of glyph area. [0-1]
    top:        f32,
    /// Glyph metrics in pixels.
    pub metrics: Metrics,
    /// Guard to prevent resizing of the texture which these fractional values point at.
    phantom:    PhantomData<&'a usize>,
}

impl<'a> Glyph<'a> {
    /// Returns a list of non-scaled vertices for two triangles making up the quad for this texture.
    ///
    /// `p` is the position of the lower-left corner of the quad in pixels. `rgb` is the foreground
    /// sRGB color.
    #[inline]
    pub fn vertices(&self, p: (f32, f32), rgb: [f32; 3]) -> [TexturedVertex; 6] {
        // vertex positions
        let l = p.0 + self.metrics.padding.left   as f32;
        let r =   l + self.metrics.width          as f32;
        let b = p.1 + self.metrics.padding.bottom as f32;
        let t =   b + self.metrics.height         as f32;

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

/// The growth factor for the atlas.
const ATLAS_GROWTH_FACTOR: u32 = 2;

pub struct Atlas {
    context:    Rc<Context>,
    /// Used pixels of `texture` (left, bottom), free space above and to the right of this point.
    used:       (u32, u32),
    /// Current height of the currently building row.
    row_height: u32,
    texture:    Texture2d,
}

/// Somewhat basic automatically resizing texture-atlas.
impl Atlas {
    pub fn new(context: Rc<Context>, width: u32, height: u32) -> Self {
        use glium::Surface;

        // FIXME: Return Result instead
        let tex = Texture2d::empty_with_format(&context, UncompressedFloatFormat::U8, MipmapsOption::NoMipmap, width, height).unwrap();

        tex.as_surface().clear_color(0.0, 0.0, 0.0, 0.0);

        Atlas {
            context:    context,
            used:       (0, 0),
            row_height: 0,
            texture:    tex,
        }
    }

    pub fn add<P: PixelValue + Clone>(&mut self, raw: RawImage2d<P>) -> Rect {
        // TODO: Add 1 pixel margins to avoid rounding errors in sampling?

        let cur_size     = (self.texture.get_width(), self.texture.get_height().unwrap_or(1));
        let mut new_size = cur_size;

        // Extend width if necessary
        while raw.width > new_size.0 {
            new_size.0 = new_size.0 * ATLAS_GROWTH_FACTOR;
        }

        // Have we used up this row? If so, end it and create a new one
        if self.used.0 + raw.width > new_size.0 {
            self.used.0     = 0;
            self.used.1     = self.used.1 + self.row_height;
            self.row_height = 0;
        }

        // Extend height if necessary
        while self.used.1 + raw.height > new_size.1 {
            new_size.1 = new_size.1 * ATLAS_GROWTH_FACTOR;
        }

        if cur_size != new_size {
            use glium::Surface;

            info!("Resizing texture atlas to ({}, {})", new_size.0, new_size.1);

            let img: Vec<_> = self.texture.read();
            let h           = self.texture.get_height().unwrap_or(1);
            let w           = self.texture.get_width();

            // FIXME: Return Result
            self.texture = Texture2d::empty_with_format(&self.context, UncompressedFloatFormat::U8, MipmapsOption::NoMipmap, new_size.0, new_size.1).unwrap();

            self.texture.as_surface().clear_color(0.0, 0.0, 0.0, 0.0);

            self.texture.write(Rect{
                left:   0,
                bottom: 0,
                height: h,
                width:  w,
            }, img);
        }

        let r = Rect{
            left:   self.used.0,
            bottom: self.used.1,
            height: raw.height,
            width:  raw.width,
        };

        self.texture.write(r, raw);

        self.used.0     = self.used.0 + r.width;
        self.row_height = cmp::max(self.row_height, r.height);

        r
    }

    #[inline]
    pub fn texture(&self) -> &Texture2d {
        &self.texture
    }

    #[inline]
    pub fn texture_size(&self) -> (u32, u32) {
        (self.texture.get_width(), self.texture.get_height().unwrap_or(1))
    }
}
