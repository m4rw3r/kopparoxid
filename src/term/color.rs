use ctrl;

/// Translates a color code into RGB float values in the range [0, 1] suitable for rendering.
pub trait Manager {
    /// Translates the given color in the context of a foreground color.
    fn fg(&self, color: ctrl::Color) -> [f32; 3];
    /// Translates the given color in the context of a background color.
    fn bg(&self, color: ctrl::Color) -> [f32; 3];
    /// Gives the fill-color for filling the background.
    fn fill(&self) -> [f32; 3];
}

/// Maps the color sequences onto the correct palette indexes. These are passed to ``f`` to
/// be converted into a RGB float-value suitable for rendering.
fn to_color<F>(c: ctrl::Color, default: [f32; 3], f: F) -> [f32; 3]
  where F: Sized + Fn(u8) -> [f32; 3] {
    use ctrl::Color::*;

    match c {
        Black        => f(0),
        Red          => f(1),
        Green        => f(2),
        Yellow       => f(3),
        Blue         => f(4),
        Magenta      => f(5),
        Cyan         => f(6),
        White        => f(7),
        Default      => default,
        Palette(p)   => f(p),
        RGB(r, g, b) => [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0],
    }
}

/// Yields the default xterm palette
fn xterm_palette(c: u8) -> [f32; 3] {
    match c {
        // TODO: Make a configurable version of this
        0  => [0.0, 0.0, 0.0],
        1  => [0.5, 0.0, 0.0],
        2  => [0.0, 0.5, 0.0],
        3  => [0.5, 0.5, 0.0],
        4  => [0.0, 0.0, 0.5],
        5  => [0.5, 0.0, 0.5],
        6  => [0.0, 0.5, 0.5],
        7  => [0.75, 0.75, 0.75],
        8  => [0.5, 0.5, 0.5],
        9  => [1.0, 0.0, 0.0],
        10 => [0.0, 1.0, 0.0],
        11 => [1.0, 1.0, 0.0],
        12 => [0.0, 0.0, 1.0],
        13 => [1.0, 0.0, 1.0],
        14 => [0.0, 1.0, 1.0],
        15 => [1.0, 1.0, 1.0],
        c if c < 232 => {
            // 6x6x6 color cube from 16-231, indices 0-6
            let blue  = (c - 16) % 6;
            let green = (c - 16) % 36 / 6;
            let red   = (c - 16) / 36;

            let r = if red   != 0 { (red   * 40 + 55) as f32 } else { 0.0 };
            let g = if green != 0 { (green * 40 + 55) as f32 } else { 0.0 };
            let b = if blue  != 0 { (blue  * 40 + 55) as f32 } else { 0.0 };

            [r / 255.0, g / 255.0, b / 255.0]
        },
        c => {
            // greyscale 232-255
            let level = (c - 232) * 10 + 8;
            let f     = level as f32 / 255.0;

            [f, f, f]
        }
    }
}

pub struct XtermDefault;

impl Manager for XtermDefault {
    fn fg(&self, color: ctrl::Color) -> [f32; 3] {
        to_color(color, [0.0, 0.0, 0.0], xterm_palette)
    }

    fn bg(&self, color: ctrl::Color) -> [f32; 3] {
        to_color(color, [1.0, 1.0, 1.0], xterm_palette)
    }

    fn fill(&self) -> [f32; 3] {
        [1.0, 1.0, 1.0]
    }
}

