use glium;
use std::cmp;
use std::fmt;
use std::collections;
use std::marker::PhantomData;
use ctrl;
use ft;

use glium::Surface;

const GROWTH_FACTOR: u32 = 2;

#[derive(Debug)]
pub struct Atlas<'a, F> where F: 'a + glium::backend::Facade {
    context:    &'a F,
    used:       (u32, u32),
    row_height: u32,
    texture:    glium::texture::Texture2d,
}

impl<'a, F> Atlas<'a, F> where F: 'a + glium::backend::Facade {
    pub fn new(facade: &'a F, width: u32, height: u32) -> Self {
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
        self.add_with_padding(raw, (0, 0, 0, 0))
    }

    pub fn add_with_padding<P: glium::texture::PixelValue + Clone>(&mut self, raw: glium::texture::RawImage2d<P>, padding: (u32, u32, u32, u32)) -> glium::Rect {
        let req_size     = (padding.0 + raw.width + padding.2, padding.1 + raw.height + padding.3);
        let cur_size     = (self.texture.get_width(), self.texture.get_height().unwrap_or(1));
        let mut new_size = cur_size;

        // Extend width if necessary
        while req_size.0 > new_size.0 {
            new_size.0 = new_size.0 * GROWTH_FACTOR;
        }

        // Have we used up this row? If so, end it and create a new one
        if self.used.0 + req_size.0 > new_size.0 {
            self.used.0     = 0;
            self.used.1     = self.used.1 + self.row_height;
            self.row_height = 0;
        }

        // Extend height if necessary
        while self.used.1 + req_size.1 > new_size.1 {
            new_size.1 = new_size.1 * GROWTH_FACTOR;
        }

        if cur_size != new_size {
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
            left:   self.used.0 + padding.0,
            bottom: self.used.1 + padding.1,
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

#[derive(Debug)]
pub struct GlyphMap<'a, F> where F: 'a + glium::backend::Facade {
    ft_face:  ft::Face<'a>,
    glyphs:   collections::BTreeMap<usize, glium::Rect>,
    atlas:    Atlas<'a, F>
}

impl<'a, F> GlyphMap<'a, F> where F: 'a + glium::backend::Facade {
    pub fn new(display: &'a F, ft_face: ft::Face<'a>) -> Self {
        GlyphMap::new_with_size(display, ft_face, 1000)
    }

    pub fn new_with_size(display: &'a F, ft_face: ft::Face<'a>, atlas_size: u32) -> Self {
        GlyphMap {
            ft_face: ft_face,
            glyphs:  collections::BTreeMap::new(),
            atlas:   Atlas::new(display, atlas_size, atlas_size),
        }
    }

    pub fn load(&mut self, glyph: usize) -> Result<(), GlyphError> {
        use std::borrow::Cow;
        use glium::texture;

        if self.glyphs.contains_key(&glyph) {
            return Ok(())
        }

        try!(self.ft_face.load_char(glyph, ft::face::RENDER | ft::face::TARGET_LIGHT));

        let g = self.ft_face.glyph();
        let glyph_bitmap = g.bitmap();

        let height   = try!(self.ft_face.size_metrics().ok_or(GlyphError::MissingGlyphMetrics(glyph))).height;
        let ascender = try!(self.ft_face.size_metrics().ok_or(GlyphError::MissingGlyphMetrics(glyph))).ascender;

        let left   = cmp::max(0, g.bitmap_left());
        let top    = cmp::max(0, (ascender >> 6) as i32 - g.bitmap_top());
        let bottom = cmp::max(0, (height >> 6) as i32 - top - glyph_bitmap.rows());
        let right  = cmp::max(0, (g.advance().x >> 6) as i32 - g.bitmap_left() - glyph_bitmap.width());

        let r = self.atlas.add_with_padding(texture::RawImage2d{
            data:   Cow::Borrowed(glyph_bitmap.buffer()),
            width:  glyph_bitmap.width() as u32,
            height: glyph_bitmap.rows() as u32,
            format: texture::ClientFormat::U8
        }, (left as u32 +1, top as u32 +1, right as u32 +1, bottom as u32 +1));

        self.glyphs.insert(glyph, r);

        Ok(())
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
    left:    f32,
    /// Distance from left side of texture to right side of glyph area. [0-1]
    right:   f32,
    /// Distance from bottom side of texture to bottom side of glyph area. [0-1]
    bottom:  f32,
    /// Distance from bottom side of texture to top side of glyph area. [0-1]
    top:     f32,
    /// Guard to prevent resizing of the texture which these fractional values point at
    phantom: PhantomData<&'a usize>,
}

impl<'a> Glyph<'a> {
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
