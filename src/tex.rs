use glium;
use std::cmp;

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
