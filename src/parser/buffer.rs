use std::io;
use std::iter;

use std::io::BufRead;

use super::Parsed;

pub enum IterResult<T, E> {
    Data(T),
    Error(E),
    IoError(io::Error),
}

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
    chunk:  usize,
    /// The number of bytes from the start of the buffer which has been consumed
    used:   usize,
    /// The number of bytes from the start of the buffer which have been populated
    size:   usize,
}

impl<T: io::Read> Buffer<T> {
    /// Creates a new buffer with the given ``chunksize`` and ``bufsize``
    pub fn new(source: T, chunksize: usize, bufsize: usize) -> Self {
        // TODO: Error
        assert!(chunksize < bufsize);

        let mut buffer = Vec::with_capacity(bufsize);

        // Fill buffer with zeroes
        buffer.extend(iter::repeat(0).take(bufsize));

        Buffer {
            source: source,
            buffer: buffer,
            chunk:  chunksize,
            used:   0,
            size:   0,
        }
    }

    /// Iterates the parser over the data loaded into the buffer, ending when the buffer
    /// is empty or the parser responds with ``Parsed::Incomplete``.
    pub fn iter_buf<'a, R, E, P>(&'a mut self, parser: P) -> ParserBufIter<'a, T, P>
      where P: Sized + Fn(&'a [u8]) -> Parsed<R, E> {
        ParserBufIter {
            buffer: self,
            parser: parser,
            used:   0,
        }
    }

    /// Iterates the parser over the data loaded into the buffer, filling the buffer when
    /// the parser responds with ``Parsed::Incomplete``.
    ///
    /// Stops when attempts to fill the buffer reads 0 bytes.
    pub fn iter<'a, R, E, P>(&'a mut self, parser: P) -> ParserIter<'a, T, P>
      where P: Sized + Fn(&'a [u8]) -> Parsed<R, E> {
        ParserIter {
            buffer: self,
            parser: parser,
            used:   0,
        }
    }

    fn drop_used(&mut self) {
        use std::ptr;

        assert!(self.size >= self.used);

        unsafe {
            ptr::copy(self.buffer.as_ptr().offset(self.used as isize), self.buffer.as_mut_ptr(), self.size - self.used);
        }

        self.size = self.size - self.used;
        self.used = 0;
    }

    /// Attempts to fill this buffer so it contains at least ``chunksize`` bytes.
    pub fn fill(&mut self) -> io::Result<usize> {
        let mut read = 0;

        if self.size < self.used + self.chunk {
            self.drop_used();
        }

        while self.size + read < self.chunk {
            match try!(self.source.read(&mut self.buffer[self.size + read..])) {
                0 => break,
                n => read = read + n,
            }
        }

        self.size = self.size + read;

        Ok(read)
    }

    /// Returns the number of bytes left in the buffer.
    pub fn len(&self) -> usize {
        self.size - self.used
    }

    /// Borrows the remainder of the buffer.
    pub fn buffer(&self) -> &[u8] {
        &self.buffer[self.used..self.size]
    }
}

impl<T: io::Read> io::Read for Buffer<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        use std::io::Read;

        if buf.len() > self.size - self.used {
            try!(self.fill());
        }

        return (&self.buffer[self.used..self.size]).read(buf).map(|n| {
            self.used = self.used + n;

            n
        });
    }
}

impl<T: io::Read> io::BufRead for Buffer<T> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        try!(self.fill());

        Ok(&self.buffer[self.used..self.size])
    }

    fn consume(&mut self, num: usize) {
        self.used = self.used + num
    }
}

struct ParserIter<'a, T: 'a + io::Read, P> {
    buffer: &'a mut Buffer<T>,
    parser: P,
    used:   usize,
}

impl<'a, T: 'a + io::Read, P, R, E> ParserIter<'a, T, P>
  where P: Sized + FnMut(&[u8]) -> Parsed<R, E> {

    /// Limits the number of bytes read while iterating to prevent potentially endless iteration.
    ///
    /// The iterator will return ``None`` upon reaching this limit.
    ///
    /// Bytes already present in the buffer will not count towards this limit.
    pub fn limit_bytes(self, num: usize) -> LimitedParserIter<'a, T, P> {
        self.buffer.consume(self.used);

        LimitedParserIter {
            buffer:    self.buffer,
            parser:    self.parser,
            used:      self.used,
            remaining: num,
        }
    }
}

impl<'a, T: 'a + io::Read, P, R, E> Iterator for ParserIter<'a, T, P>
  where P: Sized + FnMut(&[u8]) -> Parsed<R, E> {
    type Item = IterResult<R, E>;

    fn next(&mut self) -> Option<IterResult<R, E>> {
        self.buffer.consume(self.used);

        let mut parser = &mut self.parser;

        loop {
            match parser(self.buffer.buffer()) {
                Parsed::Data(consumed, data) => {
                    self.used = consumed;

                    return Some(IterResult::Data(data))
                },
                Parsed::Error(consumed, err) => {
                    self.used = consumed;

                    return Some(IterResult::Error(err))
                },
                Parsed::Incomplete => {}
            }

            match self.buffer.fill() {
                Ok(0)    => return None,
                Ok(_)    => {},
                Err(err) => return Some(IterResult::IoError(err)),
            }
        }
    }
}

struct LimitedParserIter<'a, T: 'a + io::Read, P> {
    buffer:    &'a mut Buffer<T>,
    parser:    P,
    used:      usize,
    remaining: usize,
}

impl<'a, T: 'a + io::Read, P, R, E> Iterator for LimitedParserIter<'a, T, P>
  where P: Sized + FnMut(&[u8]) -> Parsed<R, E> {
    type Item = IterResult<R, E>;

    fn next(&mut self) -> Option<IterResult<R, E>> {
        self.buffer.consume(self.used);

        let mut parser = &mut self.parser;

        loop {
            match parser(self.buffer.buffer()) {
                Parsed::Data(consumed, data) => {
                    self.used = consumed;

                    return Some(IterResult::Data(data))
                },
                Parsed::Error(consumed, err) => {
                    self.used = consumed;

                    return Some(IterResult::Error(err))
                },
                Parsed::Incomplete => {}
            }

            if self.remaining == 0 {
                return None;
            }

            match self.buffer.fill() {
                Ok(0)    => return None,
                Ok(n)    => {
                    self.remaining = if self.remaining > n {
                        self.remaining - n
                    } else {
                        0
                    };
                },
                Err(err) => return Some(IterResult::IoError(err)),
            }
        }
    }
}

struct ParserBufIter<'a, T: 'a + io::Read, P> {
    buffer: &'a mut Buffer<T>,
    parser: P,
    used:   usize,
}

impl<'a, T: 'a + io::Read, P, R, E> Iterator for ParserBufIter<'a, T, P>
  where P: Sized + FnMut(&[u8]) -> Parsed<R, E> {
    type Item = Result<R, E>;

    fn next(&mut self) -> Option<Result<R, E>> {
        self.buffer.consume(self.used);

        let mut parser = &mut self.parser;

        match parser(self.buffer.buffer()) {
            Parsed::Data(consumed, data) => {
                self.used = consumed;

                Some(Ok(data))
            },
            Parsed::Error(consumed, err) => {
                self.used = consumed;

                Some(Err(err))
            },
            Parsed::Incomplete => None
        }
    }
}
