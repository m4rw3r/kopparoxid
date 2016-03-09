use std::cmp;
use std::ops;

use num::{Bounded, Num, One};

pub trait N: Num + cmp::Ord {}

impl N for usize {}
impl N for isize {}

#[derive(Debug, Clone, Copy, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Coord<T: N> {
    pub col: T,
    pub row: T,
}

impl<T: N> Coord<T> {
    /// Calculates a new `Coord` which is inside `other`
    pub fn limit_within(self, other: Self) -> Self {
        Coord {
            col: cmp::min(self.col, other.col - One::one()),
            row: cmp::min(self.row, other.row - One::one()),
        }
    }
}

impl<T: N> ops::Add for Coord<T> {
    type Output = Coord<T>;

    fn add(self, rhs: Coord<T>) -> Coord<T> {
        Coord {
            col: self.col + rhs.col,
            row: self.row + rhs.row,
        }
    }
}

impl From<Coord<usize>> for Coord<isize> {
    fn from(other: Coord<usize>) -> Self {
        Coord {
            col: cmp::min(other.col, Bounded::max_value()) as isize,
            row: cmp::min(other.row, Bounded::max_value()) as isize,
        }
    }
}

impl From<Coord<isize>> for Coord<usize> {
    fn from(other: Coord<isize>) -> Self {
        Coord {
            col: cmp::max(0, other.col) as usize,
            row: cmp::max(0, other.row) as usize,
        }
    }
}
