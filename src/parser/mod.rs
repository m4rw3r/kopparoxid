mod buffer;

pub use self::buffer::Buffer;
pub use self::buffer::IterResult;

#[derive(Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Debug, Hash)]
/// This is the normal result of a parser.
pub enum Parsed<T, E> {
    /// Produced data from the parser, with number of consumed bytes first.
    Data(usize, T),
    /// Error during parsing, with number of consumed bytes first.
    Error(usize, E),
    /// Parser requires more data before being able to produce a result.
    Incomplete,
}

impl<T, E> Parsed<T, E> {
    #[inline]
    pub fn is_data(&self) -> bool {
        match *self {
            Parsed::Data(_, _) => true,
            _                  => false,
        }
    }

    #[inline]
    pub fn is_error(&self) -> bool {
        match *self {
            Parsed::Error(_, _) => true,
            _                   => false,
        }
    }

    #[inline]
    pub fn is_incomplete(&self) -> bool {
        match *self {
            Parsed::Incomplete => true,
            _                  => false,
        }
    }

    #[inline]
    pub fn used(&self) -> usize {
        match *self {
            Parsed::Data(u, _)  => u,
            Parsed::Error(u, _) => u,
            Parsed::Incomplete  => 0,
        }
    }

    #[inline]
    pub fn data(self) -> Option<T> {
        match self {
            Parsed::Data(_, d) => Some(d),
            _                  => None,
        }
    }

    #[inline]
    pub fn error(self) -> Option<E> {
        match self {
            Parsed::Error(_, err) => Some(err),
            _                     => None,
        }
    }

    /// Increases the used value by the specified amount.
    /// 
    /// ```
    /// let d: Parsed<&str, &str> = Parsed::Data(2, &"foo");
    /// 
    /// assert_eq!(d.inc_used(3), Parsed::Data(5, &"foo"));
    /// ```
    #[inline]
    pub fn inc_used(self, used: usize) -> Self {
        self.map_used(|u| u + used)
    }

    /// Applies a function to the used value of the ``Data`` and ``Error`` variants.
    #[inline]
    pub fn map_used<F>(self, f: F) -> Self
      where F: FnOnce(usize) -> usize {
        match self {
            Parsed::Data(u, d)  => Parsed::Data(f(u), d),
            Parsed::Error(u, e) => Parsed::Error(f(u), e),
            Parsed::Incomplete  => Parsed::Incomplete,
        }
    }

    /// Applies a function to the value of the ``Data`` variant.
    #[inline]
    pub fn map<F, U>(self, f: F) -> Parsed<U, E>
      where F: FnOnce(T) -> U {
        match self {
            Parsed::Data(u, d)  => Parsed::Data(u, f(d)),
            Parsed::Error(u, e) => Parsed::Error(u, e),
            Parsed::Incomplete  => Parsed::Incomplete,
        }
    }

    #[inline]
    pub fn map_err<F, U>(self, f: F) -> Parsed<T, U>
      where F: FnOnce(E) -> U {
        match self {
            Parsed::Data(u, d)  => Parsed::Data(u, d),
            Parsed::Error(u, e) => Parsed::Error(u, f(e)),
            Parsed::Incomplete  => Parsed::Incomplete,
        }
    }
}

/// The type of a parser.
/// 
/// Cannot currently be used it seems, use the generic ``<F, R, E> F: Sized + Fn(&'a [u8]) -> Parsed<R, E>``.
pub type Parser<'a, T: 'a, E: 'a> = Fn(&'a [u8]) -> Parsed<T, E>;
