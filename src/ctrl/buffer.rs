use std::io;
use std::iter;
use std::str;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Parsed<T, E> {
    /// Produced data from the parser, with number of consumed bytes first.
    Data(usize, T),
    /// Error during parsing, with number of consumed bytes first.
    Error(usize, E),
    /// Parser requires more data before being able to produce a result.
    Incomplete,
}

pub enum Result<T, E> {
    Data(T),
    Error(E),
}

type Parser<'a, T: 'a, E: 'a> = Fn(&'a [u8]) -> Parsed<T, E>;

/*trait Parser<'a, 'b, T: 'b, E: 'b> {
    fn parse(&'a mut self, input: &'b [u8]) -> Result<T, E>;
}*/

/// A buffer which is always attempting to keep at least a certain
/// amount of bytes in memory if available from the underlying source.
///
/// The standard std::io::BufRead does not do this, instead it only fills
/// its buffer when it is completely consumed which causes matching against
/// the returned buffer to fail in case only a partial match is present.
pub struct Buffer<T: io::Read> {
    /// Source reader
    source: T,
    /// Internal buffer
    buffer: Vec<u8>,
    /// The requested amount of bytes to be available for reading from the buffer
    req:    usize,
    /// The number of bytes from the start of the buffer which has been consumed
    used:   usize,
    /// The number of bytes from the start of the buffer which have been populated
    size:   usize,
}

impl<T: io::Read> Buffer<T> {
    /// Creates a new buffer with the given ``reqsize`` and ``bufsize``
    pub fn new(source: T, reqsize: usize, bufsize: usize) -> Self {
        // TODO: Error
        assert!(reqsize < bufsize);

        let mut buffer = Vec::with_capacity(bufsize);

        // Fill buffer with zeroes
        buffer.extend(iter::repeat(0).take(bufsize));

        Buffer {
            source: source,
            buffer: buffer,
            req:    reqsize,
            used:   0,
            size:   0,
        }
    }

    /// Iterates the parser over the data loaded into the buffer, ending when the buffer
    /// is empty or the parser responds with ``Parsed::Incomplete``.
    pub fn iter_buf<'a, R, E, P>(&'a mut self, parser: P) -> ParserIter<'a, T, P>
      where P: Sized + Fn(&'a [u8]) -> Parsed<R, E> {
        ParserIter {
            buffer: self,
            parser: parser,
            used:   0,
        }
    }

    // TODO: iter: will iterate over the buffer, filling it when necessary, blocking or erroring
    // if it cannot read enough data (configurable)
    // pub fn iter<'a, R, E, P>

    fn drop_used(&mut self) {
        use std::ptr;

        assert!(self.size >= self.used);

        unsafe {
            ptr::copy(self.buffer.as_ptr().offset(self.used as isize), self.buffer.as_mut_ptr(), self.size - self.used);
        }

        self.size = self.size - self.used;
        self.used = 0;
    }

    pub fn fill(&mut self) -> io::Result<usize> {
        let mut read = 0;

        if self.size < self.used + self.req {
            self.drop_used();
        }

        if self.size < self.req {
            read = try!(self.source.read(&mut self.buffer[self.size..]));

            self.size = self.size + read;
        }

        Ok(read)
    }

    pub fn fill_buf(&mut self) -> io::Result<&[u8]> {
        try!(self.fill());

        Ok(&self.buffer[self.used..self.size])
    }

    pub fn buffer(&self) -> &[u8] {
        &self.buffer[self.used..self.size]
    }

    pub fn consume(&mut self, num: usize) {
        self.used = self.used + num
    }
}

struct ParserIter<'a, T: 'a + io::Read, P> {
    buffer: &'a mut Buffer<T>,
    parser: P,
    used:   usize,
}

impl<'a, T: 'a + io::Read, P, R, E> Iterator for ParserIter<'a, T, P>
  where P: Sized + FnMut(&[u8]) -> Parsed<R, E> {
    type Item = Result<R, E>;

    fn next(&mut self) -> Option<Result<R, E>> {
        self.buffer.consume(self.used);

        let mut parser = &mut self.parser;

        match parser(self.buffer.buffer()) {
            Parsed::Data(consumed, data) => {
                self.used = consumed;

                Some(Result::Data(data))
            },
            Parsed::Error(consumed, err) => {
                self.used = consumed;

                Some(Result::Error(err))
            },
            Parsed::Incomplete => None
        }
    }
}

pub struct Window<'a> {
    buffer: &'a [u8],
    cursor: usize,
}

impl<'a> Window<'a> {
    pub fn new(buffer: &'a [u8]) -> Self {
        Window {
            buffer: buffer,
            cursor: 0,
        }
    }

    /// Yields the next integer from the buffer.
    pub fn next<T: str::FromStr>(&mut self) -> Option<T> {
        if self.cursor >= self.buffer.len() {
            return None
        }

        let partial = &self.buffer[self.cursor..];

        parse_int::<T>(match partial.iter().position(|c| *c == b';') {
            Some(p) => {
                self.cursor = self.cursor + p + 1;

                &partial[..p]
            },
            None => {
                self.cursor = self.buffer.len();

                partial
            }
        })
    }

    /// Yields the unparsed portion of the buffer
    pub fn rest(&self) -> &[u8] {
        &self.buffer[self.cursor..]
    }

    /// The number of used bytes of the buffer, includes trailing ';'.
    pub fn used(&self) -> usize {
        self.cursor
    }
}

pub fn parse_int<T: str::FromStr>(buf: &[u8]) -> Option<T> {
    unsafe {
        // Should be ok for numbers
        str::from_utf8_unchecked(buf)
    }.parse::<T>().ok()
}
