//! Peekable String Iterator, with possibility to peek multiple characters at once.

use ouroboros::self_referencing;
use std::str::CharIndices;

/// Enable peeking for `CharIndices`.
pub trait PeekableCharIndicesExt {
    /// Peek the next character, returning None in the case of end of string.
    fn peek(&self) -> Option<char>;
}

impl PeekableCharIndicesExt for CharIndices<'_> {
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

    /// Skip `n` bytes from the current position (`n` must be at a char boundary).
    /// Returns the byte offset in the original content of the last consumed character,
    /// or `None` if `n == 0`.
    pub fn skip_bytes(&mut self, n: usize) -> Option<usize> {
        self.with_char_iter_mut(|iter| {
            let char_count = iter.as_str()[..n].chars().count();
            if char_count > 0 {
                iter.nth(char_count - 1)
                    .map(|(offset, c)| offset + c.len_utf8() - 1)
            } else {
                None
            }
        })
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
#[derive(Clone, Debug)]
pub struct PeekableStringIterator {
    /// Current Span.
    /// Can be reset with next_new_span().
    current_span: Span,
    /// Iterator.
    iter: OwnedCharIndices,

    /// Sorted table of (line_start_byte, line_end_byte_exclusive, line_number).
    /// `line_end_byte_exclusive` points to the '\n' for lines that end with one,
    /// or to content.len() for the last line.
    line_numbers: Vec<(usize, usize, usize)>,
}

impl Iterator for PeekableStringIterator {
    type Item = char;

    /// Get next char in the current file
    fn next(&mut self) -> Option<char> {
        if let Some((s, c)) = self.iter.next() {
            self.current_span.hi = s + c.len_utf8() - 1;
            Some(c)
        } else {
            None
        }
    }
}

impl PeekableStringIterator {
    /// Build the complete line table from `content` in a single pass.
    /// Returns a Vec of `(line_start_byte, line_end_byte_exclusive, line_number)`,
    /// sorted by `line_start_byte`. `line_end_byte_exclusive` points to the '\n'
    /// for newline-terminated lines, and to `content.len()` for the last line.
    fn build_line_numbers(content: &str) -> Vec<(usize, usize, usize)> {
        let mut lines = Vec::new();
        let mut line_start = 0usize;
        let mut line_num = 1usize;
        for (i, b) in content.bytes().enumerate() {
            if b == b'\n' {
                lines.push((line_start, i, line_num));
                line_num += 1;
                line_start = i + 1;
            }
        }
        if !content.is_empty() {
            lines.push((line_start, content.len(), line_num));
        }
        lines
    }

    /// Initialize the iterator.
    pub fn new(_filename: String, content: String) -> PeekableStringIterator {
        let line_numbers = Self::build_line_numbers(&content);

        let iter = OwnedCharIndicesBuilder {
            content,
            char_iter_builder: |content| content.char_indices(),
        }
        .build();
        let current_span = Span { lo: 0, hi: 0 };

        PeekableStringIterator {
            iter,
            current_span,
            line_numbers,
        }
    }

    /// Get next char, resetting the current span to the char's location.
    pub fn next_new_span(&mut self) -> Option<char> {
        if let Some((s, c)) = self.iter.next() {
            self.current_span.lo = s;
            self.current_span.hi = s + c.len_utf8() - 1;
            Some(c)
        } else {
            None
        }
    }

    /// Peek the next character in the current file.
    pub fn peek(&self) -> Option<char> {
        self.iter.peek()
    }

    /// Call `cb` with the remaining (not-yet-consumed) content as a `&str`.
    pub fn rest_str<F: FnOnce(&str) -> R, R>(&self, cb: F) -> R {
        self.iter.rest_str(cb)
    }

    /// Returns whether the current iterator position starts with `s`.
    pub fn starts_with(&self, s: &str) -> bool {
        self.iter.rest_str(|iter_s| iter_s.starts_with(s))
    }

    /// Get the current span.
    pub fn current_span(&self) -> Span {
        self.current_span
    }

    /// Skip `n` bytes from the current position (`n` must be at a char boundary).
    pub fn skip_bytes(&mut self, n: usize) {
        if let Some(hi) = self.iter.skip_bytes(n) {
            self.current_span.hi = hi;
        }
    }

    /// Skip all content up to (but not including) the next newline character.
    pub fn skip_to_newline(&mut self) {
        let n = self.iter.rest_str(|s| s.find('\n').unwrap_or(s.len()));
        self.skip_bytes(n);
    }

    /// Skip past the first occurrence of `target`. If not found, skip to end of content.
    pub fn skip_past_str(&mut self, target: &str) {
        let n = self
            .iter
            .rest_str(|s| s.find(target).map(|p| p + target.len()).unwrap_or(s.len()));
        self.skip_bytes(n);
    }

    /// Get characters contained in the span.
    pub fn get_content_between(&self, span: Span) -> String {
        self.iter.content()[span.lo..=span.hi].to_string()
    }

    /// Find the line containing `offset`, returning `(line_start, line_end_exclusive, line_number)`.
    fn find_line(&self, offset: usize) -> (usize, usize, usize) {
        let idx = self
            .line_numbers
            .partition_point(|&(start, _, _)| start <= offset);
        self.line_numbers[idx - 1]
    }

    fn get_span_indices(&self, span: Span) -> (usize, usize) {
        (self.find_line(span.lo).0, self.find_line(span.hi).1)
    }

    /// Get the line numbers for the match. Returns (first_line, last_line).
    pub fn get_line_information(&self, span: Span) -> (usize, usize) {
        (self.find_line(span.lo).2, self.find_line(span.hi).2)
    }

    /// Get line contents for the two matches.
    pub fn get_lines_including(&self, span: Span) -> (String, Vec<String>, String) {
        let (start_index, end_index) = self.get_span_indices(span);
        let content_str = self.iter.content();

        let head = content_str[start_index..span.lo].to_string();
        let tail = content_str[span.hi + 1..end_index].to_string();
        let content = self
            .get_content_between(span)
            .lines()
            .map(|s| s.to_string())
            .collect();

        (head, content, tail)
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
    fn peek_ahead() {
        let mut iter = PeekableStringIterator::new("foo.h".to_string(), "foo bar baz".to_string());
        assert!(iter.starts_with("foo "));
        assert!(iter.starts_with("foo "));
        assert!(!iter.starts_with("bar"));
        assert_eq!(iter.next(), Some('f'));
    }

    #[test]
    fn get_content_between() {
        let iter = PeekableStringIterator::new("foo.h".to_string(), "foo bar baz".to_string());
        assert_eq!(iter.get_content_between(Span { lo: 4, hi: 6 }), "bar");
        assert_eq!(iter.get_content_between(Span { lo: 4, hi: 4 }), "b");
    }

    #[test]
    fn current_span_tracks_next() {
        let mut iter = PeekableStringIterator::new("f".to_string(), "foo".to_string());
        assert_eq!(iter.current_span(), Span { lo: 0, hi: 0 });
        iter.next();
        assert_eq!(iter.current_span(), Span { lo: 0, hi: 0 });
        iter.next();
        assert_eq!(iter.current_span(), Span { lo: 0, hi: 1 });
        iter.next();
        assert_eq!(iter.current_span(), Span { lo: 0, hi: 2 });
    }

    #[test]
    fn next_new_span_resets_lo() {
        let mut iter = PeekableStringIterator::new("f".to_string(), "foo bar".to_string());
        iter.next();
        iter.next();
        iter.next();
        assert_eq!(iter.current_span(), Span { lo: 0, hi: 2 });
        iter.next_new_span();
        assert_eq!(iter.current_span(), Span { lo: 3, hi: 3 });
        iter.next();
        iter.next();
        iter.next();
        assert_eq!(iter.current_span(), Span { lo: 3, hi: 6 });
    }

    #[test]
    fn rest_str_returns_remaining() {
        let mut iter = PeekableStringIterator::new("f".to_string(), "foo bar".to_string());
        iter.rest_str(|s| assert_eq!(s, "foo bar"));
        iter.next();
        iter.rest_str(|s| assert_eq!(s, "oo bar"));
        iter.next();
        iter.next();
        iter.rest_str(|s| assert_eq!(s, " bar"));
    }

    #[test]
    fn skip_bytes() {
        let mut iter = PeekableStringIterator::new("f".to_string(), "foo bar".to_string());
        iter.skip_bytes(0);
        assert_eq!(iter.current_span(), Span { lo: 0, hi: 0 });
        assert_eq!(iter.peek(), Some('f'));
        iter.skip_bytes(3);
        assert_eq!(iter.current_span(), Span { lo: 0, hi: 2 });
        assert_eq!(iter.peek(), Some(' '));
    }

    #[test]
    fn skip_to_newline() {
        let mut iter = PeekableStringIterator::new("f".to_string(), "// comment\ncode".to_string());
        iter.skip_to_newline();
        assert_eq!(iter.peek(), Some('\n'));

        // no newline: skip to end
        let mut iter = PeekableStringIterator::new("f".to_string(), "no newline".to_string());
        iter.skip_to_newline();
        assert_eq!(iter.peek(), None);
    }

    #[test]
    fn skip_past_str() {
        let mut iter =
            PeekableStringIterator::new("f".to_string(), "/* comment */ code".to_string());
        iter.skip_past_str("*/");
        assert_eq!(iter.peek(), Some(' '));

        // not found: skip to end
        let mut iter = PeekableStringIterator::new("f".to_string(), "no match here".to_string());
        iter.skip_past_str("*/");
        assert_eq!(iter.peek(), None);
    }

    #[test]
    fn get_line_information() {
        let iter = PeekableStringIterator::new("f".to_string(), "foo\nbar\nbaz".to_string());
        // single-line spans
        assert_eq!(iter.get_line_information(Span { lo: 0, hi: 2 }), (1, 1));
        assert_eq!(iter.get_line_information(Span { lo: 4, hi: 6 }), (2, 2));
        assert_eq!(iter.get_line_information(Span { lo: 8, hi: 10 }), (3, 3));
        // multi-line spans
        assert_eq!(iter.get_line_information(Span { lo: 0, hi: 6 }), (1, 2));
        assert_eq!(iter.get_line_information(Span { lo: 0, hi: 10 }), (1, 3));
    }

    #[test]
    fn get_lines_including() {
        // middle of line
        let iter = PeekableStringIterator::new("f".to_string(), "foo bar baz".to_string());
        assert_eq!(
            iter.get_lines_including(Span { lo: 4, hi: 6 }),
            (
                "foo ".to_string(),
                vec!["bar".to_string()],
                " baz".to_string()
            )
        );

        // start of line: "foo\nbar baz", "bar" starts at byte 4
        let iter = PeekableStringIterator::new("f".to_string(), "foo\nbar baz".to_string());
        assert_eq!(
            iter.get_lines_including(Span { lo: 4, hi: 6 }),
            ("".to_string(), vec!["bar".to_string()], " baz".to_string())
        );

        // multi-line span covering entire "foo\nbar"
        let iter = PeekableStringIterator::new("f".to_string(), "foo\nbar".to_string());
        assert_eq!(
            iter.get_lines_including(Span { lo: 0, hi: 6 }),
            (
                "".to_string(),
                vec!["foo".to_string(), "bar".to_string()],
                "".to_string()
            )
        );
    }

    #[test]
    fn unicode() {
        let mut iter = PeekableStringIterator::new("f".to_string(), "héllo".to_string());
        assert_eq!(iter.next(), Some('h'));
        assert_eq!(iter.next(), Some('é'));
        assert_eq!(iter.next(), Some('l'));
        assert_eq!(iter.peek(), Some('l'));

        let mut iter = PeekableStringIterator::new("f".to_string(), "héllo".to_string());
        iter.skip_bytes(3);
        assert_eq!(iter.peek(), Some('l'));

        let mut iter = PeekableStringIterator::new("f".to_string(), "héllo".to_string());
        iter.next_new_span(); // 'h'
        iter.next_new_span(); // 'é'
        let span = iter.current_span();
        assert_eq!(iter.get_content_between(span), "é");
    }
}
