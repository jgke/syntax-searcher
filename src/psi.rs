//! Peekable String Iterator, with possibility to peek multiple characters at once.

use ouroboros::self_referencing;
use std::collections::BTreeMap;
use std::str::CharIndices;

/// Enable peeking for `CharIndices`.
pub trait PeekableCharIndicesExt {
    /// Peek the next character, returning None in the case of end of string.
    fn peek(&self) -> Option<char>;
}

impl<'a> PeekableCharIndicesExt for CharIndices<'a> {
    fn peek(&self) -> Option<char> {
        self.as_str().chars().next()
    }
}

/// A span in the currently parsed file.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Span {
    /// Starting byte index of the span.
    pub lo: usize,
    /// End byte index of the span.
    pub hi: usize,
}

impl Span {
    /// Merge two spans.
    pub fn merge(&self, other: &Span) -> Span {
        Span {
            lo: self.lo.min(other.lo),
            hi: self.hi.max(other.hi),
        }
    }
}

#[self_referencing]
#[derive(Debug)]
struct OwnedCharIndices {
    /// String being iterated over.
    pub content: String,
    /// Iterator over the String. Points to content.
    #[borrows(content)]
    #[covariant]
    pub char_iter: CharIndices<'this>,
}

impl OwnedCharIndices {
    pub fn next(&mut self) -> Option<(usize, char)> {
        self.with_char_iter_mut(|iter| iter.next())
    }

    pub fn peek(&self) -> Option<char> {
        self.with_char_iter(|iter| iter.peek())
    }

    pub fn content(&self) -> &str {
        self.borrow_content()
    }

    pub fn rest_str<F: FnOnce(&str) -> R, R>(&self, cb: F) -> R {
        self.with_char_iter(|iter| cb(iter.as_str()))
    }
}

impl Clone for OwnedCharIndices {
    fn clone(&self) -> Self {
        OwnedCharIndicesBuilder {
            content: self.borrow_content().clone(),
            char_iter_builder: |content: &String| content.char_indices(),
        }
        .build()
    }
}

/// An iterator over strings, keeping track of origins for each substring.
///
/// # Examples
///
/// Basic usage:
/// ```
/// use syns::psi::PeekableStringIterator;
///
/// let mut iter = PeekableStringIterator::new("foo.h".to_string(), "foo BAR baz".to_string());
/// let (identifier, _) = iter.collect_while(|x| match x {
///     'a'..='z' => true,
///     _ => false
/// });
/// assert_eq!(&identifier, "foo");
/// ```
#[derive(Clone, Debug)]
pub struct PeekableStringIterator {
    /// Current Span.
    /// Can be reset with next_new_span().
    current_span: Span,
    /// Iterator.
    iter: OwnedCharIndices,

    /// Current line starting byte.
    current_line_starting_byte: usize,
    /// Current line number, starting from 1.
    current_line_number: usize,
    /// Map from starting byte to (exclusive ending byte, line number) in the file.
    line_numbers: BTreeMap<usize, (usize, usize)>,
}

impl Iterator for PeekableStringIterator {
    type Item = char;

    /// Get next char in the current file
    fn next(&mut self) -> Option<char> {
        if let Some((s, c)) = self.iter.next() {
            if c == '\n' {
                self.line_numbers.insert(
                    self.current_line_starting_byte,
                    (self.current_span.hi + 1, self.current_line_number),
                );
                self.current_line_number += 1;
                self.current_line_starting_byte = s + 1;
            }
            self.current_span.hi = s;
            Some(c)
        } else {
            if self.current_line_starting_byte != 0 {
                self.line_numbers.insert(
                    self.current_line_starting_byte,
                    (self.current_span.hi + 1, self.current_line_number),
                );
                self.current_line_number += 1;
                self.current_line_starting_byte = 0
            }
            None
        }
    }
}

impl PeekableStringIterator {
    /// Initialize the iterator.
    pub fn new(_filename: String, content: String) -> PeekableStringIterator {
        let mut line_numbers = BTreeMap::new();

        // If we don't do this pre-scan, we'll get an error (for files with zero end-of-lines) or
        // an off-by-one (files not ending with eol) in get_line_information
        if let Some(c) = content.chars().last() {
            if c != '\n' {
                let last_line_start = content.rfind('\n').map(|p| p + 1).unwrap_or(0);
                let line_number = content.chars().filter(|&c| c == '\n').count() + 1;
                line_numbers.insert(last_line_start, (content.bytes().count(), line_number));
            }
        }

        let iter = OwnedCharIndicesBuilder {
            content,
            char_iter_builder: |content| content.char_indices(),
        }
        .build();
        let current_span = Span { lo: 0, hi: 0 };

        PeekableStringIterator {
            iter,
            current_span,

            current_line_starting_byte: 0,
            current_line_number: 1,
            line_numbers,
        }
    }

    /// Get next char, resetting the current span to the char's location.
    pub fn next_new_span(&mut self) -> Option<char> {
        if let Some(c) = self.next() {
            self.current_span.lo = self.current_span.hi;
            Some(c)
        } else {
            None
        }
    }

    /// Collect a string until `f` return false. Returns the string and its span.
    /// See [`collect_while_map`] for semantic details.
    ///
    /// [`collect_while_map`]: #method.collect_while_map
    ///
    /// # Example
    /// ```
    /// # use syns::psi::PeekableStringIterator;
    /// let mut iter = PeekableStringIterator::new("foo.h".to_string(), "foo bar baz".to_string());
    /// let (s1, _) = iter.collect_while(|x| match x {
    ///     'a'..='z' => true,
    ///     _ => false
    /// });
    /// assert_eq!(s1, "foo");
    /// ```
    pub fn collect_while(&mut self, mut f: impl FnMut(char) -> bool) -> (String, Span) {
        self.collect_while_map(|c, _| if f(c) { Some(c) } else { None })
    }

    /// Iterate over self, map the results with f and collect to a string from the iterator. Stops
    /// when `f` returns None or the end-of-file is reached. This will always consume at least one
    /// character from the iterator, which is stored in the string if `f` returns Some. Returns the
    /// resulting string and its span.
    ///
    /// # Example
    /// ```
    /// # use syns::psi::PeekableStringIterator;
    /// let mut iter = PeekableStringIterator::new("foo.h".to_string(), "foo bar baz".to_string());
    /// let (s1, _) = iter.collect_while_map(|x, _| match x {
    ///     'a'..='z' => Some(x.to_ascii_uppercase()),
    ///     _ => None
    /// });
    /// assert_eq!(s1, "FOO");
    /// ```
    pub fn collect_while_map(
        &mut self,
        mut f: impl FnMut(char, &mut Self) -> Option<char>,
    ) -> (String, Span) {
        let mut content = String::new();
        if let Some(c) = self.next_new_span() {
            if let Some(c) = f(c, self) {
                content.push(c);
            }
        }
        while let Some(c) = self.peek() {
            if let Some(c) = f(c, self) {
                content.push(c);
                self.next();
            } else {
                break;
            }
        }

        (content, self.current_span())
    }

    /// Peek the next character in the current file.
    pub fn peek(&self) -> Option<char> {
        self.iter.peek()
    }

    /// Peek the next `n` characters in the current file.
    pub fn peek_n(&self, n: usize) -> String {
        self.iter.rest_str(|s| s.chars().take(n).collect())
    }

    /// Returns whether the current iterator position starts with `s`.
    pub fn starts_with(&self, s: &str) -> bool {
        self.iter.rest_str(|iter_s| iter_s.starts_with(s))
    }

    /// Get the current span.
    pub fn current_span(&self) -> Span {
        self.current_span
    }

    /// Get characters contained in the span.
    pub fn get_content_between(&self, span: Span) -> String {
        String::from_utf8_lossy(
            &self
                .iter
                .content()
                .bytes()
                .skip(span.lo)
                .take(span.hi - span.lo + 1)
                .collect::<Vec<_>>(),
        )
        .to_string()
    }

    fn get_start_index(&self, offset: usize) -> usize {
        self.line_numbers
            .range(0..=offset)
            .map(|(start, _)| *start)
            .last()
            .unwrap_or(0)
    }

    fn get_end_index(&self, offset: usize) -> usize {
        self.line_numbers
            .range(0..=offset)
            .map(|(_, (end, _))| *end)
            .last()
            .unwrap_or(usize::MAX)
    }

    fn get_span_indices(&self, span: Span) -> (usize, usize) {
        let start = self.get_start_index(span.lo);
        let end = self.get_end_index(span.hi);
        (start, end)
    }

    fn get_line_starts(&self, span: Span) -> (usize, usize) {
        let start = self.get_start_index(span.lo);
        let end = self.get_start_index(span.hi);
        (start, end)
    }

    /// Get the line numbers for the match. Returns (first_line, last_line).
    pub fn get_line_information(&self, span: Span) -> (usize, usize) {
        let (start_index, end_index) = self.get_line_starts(span);

        (
            self.line_numbers[&start_index].1,
            self.line_numbers[&end_index].1,
        )
    }

    /// Get line contents for the two matches.
    pub fn get_lines_including(&self, span: Span) -> Vec<String> {
        let (start_index, end_index) = self.get_span_indices(span);

        self.iter
            .content()
            .chars()
            .skip(start_index)
            .take(end_index - start_index)
            .collect::<String>()
            .lines()
            .map(|s| s.to_string())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{PeekableStringIterator, Span};

    #[test]
    fn spans() {
        let a = Span { lo: 10, hi: 20 };
        let b = Span { lo: 5, hi: 15 };
        assert_eq!(a.merge(&b), Span { lo: 5, hi: 20 });
        assert_eq!(b.merge(&a), Span { lo: 5, hi: 20 });
    }

    #[test]
    fn iter_simple() {
        let mut iter = PeekableStringIterator::new("foo.h".to_string(), "foo".to_string());
        assert_eq!(iter.next(), Some('f'));
        assert_eq!(iter.next(), Some('o'));
        assert_eq!(iter.next(), Some('o'));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn collect_while() {
        let mut iter = PeekableStringIterator::new("foo.h".to_string(), "foo bar baz".to_string());
        let (s1, _) = iter.collect_while(|x| match x {
            'a'..='z' => true,
            _ => false,
        });
        assert_eq!(s1, "foo");
        assert_eq!(iter.next(), Some(' '));

        let (s2, _) = iter.collect_while(|x| match x {
            'a'..='z' => true,
            _ => false,
        });
        assert_eq!(s2, "bar");
        assert_eq!(iter.next(), Some(' '));

        let (s3, _) = iter.collect_while(|x| match x {
            'a'..='z' => true,
            _ => false,
        });
        assert_eq!(s3, "baz");
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn collect_while_map() {
        let mut iter = PeekableStringIterator::new("foo.h".to_string(), "foo bar baz".to_string());
        let (s1, span) = iter.collect_while_map(|x, _| match x {
            'a'..='z' => Some(x.to_ascii_uppercase()),
            _ => None,
        });
        assert_eq!(s1, "FOO");
        assert_eq!(span, Span { lo: 0, hi: 2 });

        assert_eq!(iter.next(), Some(' '));
    }

    #[test]
    fn empty_iter() {
        let mut iter = PeekableStringIterator::new("foo.h".to_string(), "foo".to_string());
        assert_eq!(iter.next(), Some('f'));
        assert_eq!(iter.next(), Some('o'));
        assert_eq!(iter.next(), Some('o'));
        assert_eq!(iter.peek(), None);
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next_new_span(), None);
    }

    #[test]
    fn peek_ahead() {
        let mut iter = PeekableStringIterator::new("foo.h".to_string(), "foo bar baz".to_string());
        assert_eq!(iter.starts_with("foo "), true);
        assert_eq!(iter.starts_with("foo "), true);
        assert_eq!(iter.starts_with("bar"), false);
        assert_eq!(iter.next(), Some('f'));
        assert_eq!(iter.next(), Some('o'));
        assert_eq!(iter.next(), Some('o'));
        assert_eq!(iter.next(), Some(' '));
        assert_eq!(iter.next(), Some('b'));
        assert_eq!(iter.next(), Some('a'));
        assert_eq!(iter.next(), Some('r'));
    }

    #[test]
    fn get_content_between() {
        let iter = PeekableStringIterator::new("foo.h".to_string(), "foo bar baz".to_string());
        assert_eq!(iter.get_content_between(Span { lo: 4, hi: 6 }), "bar");
        assert_eq!(iter.get_content_between(Span { lo: 4, hi: 4 }), "b");
    }

    #[test]
    fn get_lines() {
        let mut iter =
            PeekableStringIterator::new("foo.h".to_string(), "foo\nbar\nbaz".to_string());
        let (s1, sp1) = iter.collect_while(|x| match x {
            'a'..='z' => true,
            _ => false,
        });
        assert_eq!(s1, "foo");
        assert_eq!(iter.next(), Some('\n'));
        let (s2, sp2) = iter.collect_while(|x| match x {
            'a'..='z' => true,
            _ => false,
        });
        assert_eq!(s2, "bar");
        assert_eq!(iter.next(), Some('\n'));
        let (s3, sp3) = iter.collect_while(|x| match x {
            'a'..='z' => true,
            _ => false,
        });
        assert_eq!(s3, "baz");
        assert_eq!(iter.next(), None);

        assert_eq!(iter.get_line_information(sp1), (1, 1));
        assert_eq!(iter.get_line_information(sp2), (2, 2));
        assert_eq!(iter.get_line_information(sp3), (3, 3));
        assert_eq!(iter.get_line_information(sp1.merge(&sp2)), (1, 2));
        assert_eq!(iter.get_line_information(sp1.merge(&sp3)), (1, 3));
        assert_eq!(iter.get_line_information(sp2.merge(&sp3)), (2, 3));

        assert_eq!(iter.get_lines_including(sp1), vec!["foo"]);
        assert_eq!(iter.get_lines_including(sp2), vec!["bar"]);
        assert_eq!(iter.get_lines_including(sp3), vec!["baz"]);
        assert_eq!(
            iter.get_lines_including(sp1.merge(&sp2)),
            vec!["foo", "bar"]
        );
        assert_eq!(
            iter.get_lines_including(sp1.merge(&sp3)),
            vec!["foo", "bar", "baz"]
        );
        assert_eq!(
            iter.get_lines_including(sp2.merge(&sp3)),
            vec!["bar", "baz"]
        );
    }
}
