//! This combines the MultiPeek and PutBackN iterators from itertools.
//! Dual-licensed under Apache 2.0 or MIT, see
//! https://github.com/rust-itertools/itertools for more details.

use std::collections::VecDeque;
use std::iter::Fuse;

/// See [`multipeek_putbackn()`] for more information.
#[derive(Clone, Debug)]
#[must_use = "iterator adaptors are lazy and do nothing unless consumed"]
pub struct MultiPeekPutBackN<I>
where
    I: Iterator,
{
    iter: Fuse<I>,
    buf: VecDeque<I::Item>,
    index: usize,
}

/// An iterator adaptor that allows the user to peek at multiple `.next()`
/// values without advancing the base iterator.
pub fn multipeek_put_back_n<I>(iterable: I) -> MultiPeekPutBackN<I::IntoIter>
where
    I: IntoIterator,
{
    MultiPeekPutBackN {
        iter: iterable.into_iter().fuse(),
        buf: VecDeque::new(),
        index: 0,
    }
}

impl<I> MultiPeekPutBackN<I>
where
    I: Iterator,
{
    /// Reset the peeking “cursor”
    pub fn reset_peek(&mut self) {
        self.index = 0;
    }

    /// Puts `x` in front of the iterator, resetting the peek buffer.
    ///
    /// The values are yielded in order of the most recently put back
    /// values first.
    ///
    /// ```rust
    /// use itertools::put_back_n;
    ///
    /// let mut it = put_back_n(1..5);
    /// it.next();
    /// it.put_back(1);
    /// it.put_back(0);
    ///
    /// assert!(itertools::equal(it, 0..5));
    /// ```
    #[inline]
    pub fn put_back(&mut self, x: I::Item) {
        self.index = 0;
        self.buf.push_front(x);
    }
}

impl<I: Iterator> MultiPeekPutBackN<I> {
    /// Works exactly like `.next()` with the only difference that it doesn't
    /// advance itself. `.peek()` can be called multiple times, to peek
    /// further ahead.
    /// When `.next()` is called, reset the peeking “cursor”.
    pub fn peek(&mut self) -> Option<&I::Item> {
        let ret = if self.index < self.buf.len() {
            Some(&self.buf[self.index])
        } else {
            match self.iter.next() {
                Some(x) => {
                    self.buf.push_back(x);
                    Some(&self.buf[self.index])
                }
                None => return None,
            }
        };

        self.index += 1;
        ret
    }
}

impl<I> Iterator for MultiPeekPutBackN<I>
where
    I: Iterator,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.index = 0;
        self.buf.pop_front().or_else(|| self.iter.next())
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let (mut low, mut hi) = self.iter.size_hint();
        low = low.saturating_add(self.buf.len());
        hi = hi.and_then(|elt| elt.checked_add(self.buf.len()));
        (low, hi)
    }

    fn fold<B, F>(self, mut init: B, mut f: F) -> B
    where
        F: FnMut(B, Self::Item) -> B,
    {
        init = self.buf.into_iter().fold(init, &mut f);
        self.iter.fold(init, f)
    }
}

// Same size
impl<I> ExactSizeIterator for MultiPeekPutBackN<I> where I: ExactSizeIterator {}
