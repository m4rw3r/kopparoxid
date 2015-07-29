mod buffer;

pub use self::buffer::Buffer;
pub use self::buffer::Result;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
/// This is the normal result of a parser.
pub enum Parsed<T, E> {
    /// Produced data from the parser, with number of consumed bytes first.
    Data(usize, T),
    /// Error during parsing, with number of consumed bytes first.
    Error(usize, E),
    /// Parser requires more data before being able to produce a result.
    Incomplete,
}

/// The type of a parser.
/// 
/// Cannot currently be used it seems, use the generic ``<F, R, E> F: Sized + Fn(&'a [u8]) -> Parsed<R, E>``.
pub type Parser<'a, T: 'a, E: 'a> = Fn(&'a [u8]) -> Parsed<T, E>;
