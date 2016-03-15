#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate glium;
extern crate glutin;
extern crate freetype;
#[macro_use]
extern crate log;

extern crate cu2o_term;

mod term;
pub mod color;
pub mod glyph;

pub use term::{FontStyle, GlTerm};
