//! Tokenizer for source files.

use crate::options::Options;
use crate::psi::{PeekableStringIterator, Span};
use crate::wrappers::Float;
use std::convert::{TryFrom, TryInto};
use std::io::Read;
use std::str::FromStr;

/// Special tokens for queries.
#[derive(Clone, Debug, PartialEq)]
pub enum SpecialTokenType {
    /// Match any token.
    Any,
    /// Match previous matcher zero or more times.
    Star,
    /// Match previous matcher one or more times.
    Plus,
    /// Match group end
    End,
    /// Match previous or next matcher
    Or,
    /// Match string literals with regex.
    Regex(String),
    /// Grouped matchers.
    Nested(Vec<QueryToken>),
}

/// Stardard token types for source files.
// TODO: merge identifier, integer, float and symbol
#[derive(Clone, Debug, PartialEq, Hash)]
pub enum StandardTokenType {
    /// Identifier, eg. foo
    Identifier(String),
    /// Integer, eg. 123
    Integer(i128),
    /// Floating point number, eg. 123.0
    Float(Float),
    /// String literal, eg. "Hello"
    StringLiteral(String),
    /// Symbol, eg. +
    Symbol(String),
    /// Regex literal (without suffix flags), eg. /[a-z]/
    Regex(String),
}

/// Query token type.
#[derive(Clone, Debug, PartialEq)]
pub enum QueryTokenType {
    /// Non-special token type.
    Standard(StandardTokenType),
    /// Special token type.
    Special(SpecialTokenType),
}

/// Source code token.
#[derive(Clone, Debug, PartialEq)]
pub struct StandardToken {
    /// Type of the token.
    pub ty: StandardTokenType,
    /// Location of the token.
    pub span: Span,
}

/// Query string token.
#[derive(Clone, Debug, PartialEq)]
pub struct QueryToken {
    /// Type of the token.
    pub ty: QueryTokenType,
    /// Location of the token.
    pub span: Span,
}

impl TryFrom<QueryToken> for StandardToken {
    type Error = ();

    fn try_from(f: QueryToken) -> Result<Self, Self::Error> {
        match f {
            QueryToken {
                ty: QueryTokenType::Standard(ty),
                span,
            } => Ok(StandardToken { ty, span }),
            QueryToken {
                ty: QueryTokenType::Special(_),
                ..
            } => Err(()),
        }
    }
}

/// Tokenize a source code file.
pub fn tokenize<R: Read>(
    filename: &str,
    mut content: R,
    options: &Options,
) -> (Vec<StandardToken>, PeekableStringIterator) {
    let mut file_buf = vec![];
    content
        .read_to_end(&mut file_buf)
        .expect("Failed to read file to memory");
    let buf = String::from_utf8_lossy(&file_buf).to_string();
    let mut iter = PeekableStringIterator::new(filename.to_string(), buf);
    let res = tokenize_recur(&mut iter, options, false, false)
        .into_iter()
        .map(|t| t.try_into().expect("Unreachable"))
        .collect();
    (res, iter)
}

/// Tokenize a query string.
pub fn tokenize_query<R: Read>(
    mut content: R,
    options: &Options,
) -> (Vec<QueryToken>, PeekableStringIterator) {
    let mut file_buf = vec![];
    content
        .read_to_end(&mut file_buf)
        .expect("Failed to read file to memory");
    let buf = String::from_utf8_lossy(&file_buf).to_string();
    let mut iter = PeekableStringIterator::new("<query>".to_string(), buf);
    let res = tokenize_recur(&mut iter, options, false, true);
    (res, iter)
}

/// Given the token history, can we parse a regex literal?
///
/// This is a JavaScript-specific hack: Regex literals conflict with the division operator and
/// the closing tag. If the previous token looks like it could be a part of an expression, return
/// false. Almost everything looks like an expression in JavaScript, the regex parsing can only
/// happen either opening parens, closed blocks, or operators.
fn can_parse_regex(history: &[QueryToken]) -> bool {
    let ty = match history.last() {
        None => return true,
        Some(QueryToken {
            ty: QueryTokenType::Special(_),
            span: _,
        }) => return true,
        Some(QueryToken {
            ty: QueryTokenType::Standard(ty),
            span: _,
        }) => ty,
    };

    let sym = match ty {
        StandardTokenType::Symbol(sym) => sym,
        _ => return false,
    };

    // I believe closing paren is the only symbol blocking regex parsing?
    sym != ")"
}

/// Generate tokens from a PeekableStringIterator.
pub fn tokenize_recur(
    iter: &mut PeekableStringIterator,
    options: &Options,
    recur: bool,
    is_query: bool,
) -> Vec<QueryToken> {
    let mut res = Vec::new();
    let mut had_whitespace = false;
    while let Some(c) = iter.peek() {
        if options
            .single_line_comments
            .iter()
            .any(|c| iter.starts_with(c))
        {
            flush_single_line(iter);
            had_whitespace = true;
            continue;
        }
        if let Some((start, end)) = options
            .multi_line_comments
            .iter()
            .find(|(start, _)| iter.starts_with(start))
        {
            flush_multi_line_comment(iter, start, end);
            had_whitespace = true;
            continue;
        }
        let token = match c {
            _ if options
                .string_characters
                .iter()
                .any(|c| iter.starts_with(c)) =>
            {
                read_string(iter)
            }
            '\\' if is_query => {
                assert_eq!(iter.next_new_span(), Some('\\'));
                if recur && iter.peek() == Some(')') {
                    break;
                }
                read_query_command(iter, options)
            }
            _ if can_parse_regex(&res)
                && options.regex_delimiters.iter().any(|c| iter.starts_with(c)) =>
            {
                read_regex(iter)
            }
            ' ' | '\t' | '\n' => {
                iter.next();
                had_whitespace = true;
                continue;
            }
            c if options.identifier_regex_start.is_match(&c.to_string()) => {
                read_identifier(iter, options)
            }
            '0'..='9' => read_number(iter, options),
            c if options.is_open_paren(&c.to_string())
                || options.is_close_paren(&c.to_string()) =>
            {
                res.push(read_paren(iter));
                had_whitespace = true;
                continue;
            }
            _ => read_other(&mut res, had_whitespace, iter),
        };
        had_whitespace = false;
        res.push(token);
    }
    res
}

fn flush_single_line(iter: &mut PeekableStringIterator) {
    iter.collect_while(|x| x != '\n');
}

fn flush_multi_line_comment(iter: &mut PeekableStringIterator, start: &str, end: &str) {
    for c in start.chars() {
        assert_eq!(Some(c), iter.next());
    }
    while !iter.starts_with(end) {
        if iter.next().is_none() {
            break;
        }
    }
    for c in end.chars() {
        if let Some(other_c) = iter.next() {
            assert_eq!(c, other_c);
        }
    }
}

fn read_number(iter: &mut PeekableStringIterator, options: &Options) -> QueryToken {
    let radix_str = iter.peek_n(2);
    let radix = match radix_str.as_ref() {
        "0b" => {
            iter.next();
            iter.next();
            2
        }
        "0x" => {
            iter.next();
            iter.next();
            16
        }
        _ => 10,
    };
    let (content_str, span) = iter.collect_while_map(|c, iter| match c {
        '0'..='9' | '_' => Some(c),
        '.' if options.ranges && !iter.starts_with("..") => Some(c),
        'a'..='f' | 'A'..='F' if radix == 16 => Some(c),
        'e' => Some(c),
        _ => None,
    });
    let content = content_str
        .chars()
        .filter(|c| *c != '_')
        .collect::<String>();
    if !content.contains('.') && !content.contains('e') {
        let num = i128::from_str_radix(&content, radix)
            .ok()
            .or_else(|| {
                i128::from_str_radix(
                    &content
                        .chars()
                        .take_while(|c| c.is_ascii_digit())
                        .collect::<String>(),
                    radix,
                )
                .ok()
            })
            .unwrap_or(0);
        QueryToken {
            ty: QueryTokenType::Standard(StandardTokenType::Integer(num)),
            span,
        }
    } else {
        let num = f64::from_str(&content)
            .ok()
            .or_else(|| {
                f64::from_str(
                    &content
                        .chars()
                        .take_while(|&c| c.is_ascii_digit() || c == '.')
                        .collect::<String>(),
                )
                .ok()
            })
            .or_else(|| {
                f64::from_str(
                    &content
                        .chars()
                        .take_while(|c| c.is_ascii_digit())
                        .collect::<String>(),
                )
                .ok()
            })
            .unwrap_or(0.0);
        QueryToken {
            ty: QueryTokenType::Standard(StandardTokenType::Float(num.into())),
            span,
        }
    }
}

fn read_string_content(iter: &mut PeekableStringIterator) -> String {
    let str_end = iter.next_new_span().expect("unreachable");

    let mut content = String::new();

    loop {
        match iter.next() {
            Some(c) if c == str_end => {
                break;
            }
            Some('\\') => {
                if let Some(c) = iter.next() {
                    content.push('\\');
                    content.push(c);
                }
            }
            Some(c) => content.push(c),
            None => break,
        }
    }
    content
}

fn read_string(iter: &mut PeekableStringIterator) -> QueryToken {
    let content = read_string_content(iter);
    QueryToken {
        ty: QueryTokenType::Standard(StandardTokenType::StringLiteral(content)),
        span: iter.current_span(),
    }
}

fn read_regex(iter: &mut PeekableStringIterator) -> QueryToken {
    let content = read_string_content(iter);
    QueryToken {
        ty: QueryTokenType::Standard(StandardTokenType::Regex(content)),
        span: iter.current_span(),
    }
}

fn read_identifier(iter: &mut PeekableStringIterator, options: &Options) -> QueryToken {
    let mut first = true;
    let (content, span) = iter.collect_while(|c| {
        if first {
            first = false;
            options.identifier_regex_start.is_match(&c.to_string())
        } else {
            options.identifier_regex_continue.is_match(&c.to_string())
        }
    });

    if options.is_open_paren(&content) || options.is_close_paren(&content) {
        /* handle eg. do / end -style "parens" */
        QueryToken {
            ty: QueryTokenType::Standard(StandardTokenType::Symbol(content)),
            span,
        }
    } else {
        QueryToken {
            ty: QueryTokenType::Standard(StandardTokenType::Identifier(content)),
            span,
        }
    }
}

fn read_paren(iter: &mut PeekableStringIterator) -> QueryToken {
    match iter.next_new_span() {
        Some(c) => QueryToken {
            ty: QueryTokenType::Standard(StandardTokenType::Symbol(c.to_string())),
            span: iter.current_span(),
        },
        None => panic!("Unexpected end of file"),
    }
}

fn read_other(
    res: &mut Vec<QueryToken>,
    had_whitespace: bool,
    iter: &mut PeekableStringIterator,
) -> QueryToken {
    match iter.next_new_span() {
        Some(c) => {
            if !had_whitespace {
                if let Some(QueryToken {
                    ty: QueryTokenType::Standard(StandardTokenType::Symbol(old_c)),
                    span,
                }) = res.last()
                {
                    let new_symbol = format!("{}{}", old_c, c);
                    let new_span = span.merge(&iter.current_span());
                    res.pop();
                    QueryToken {
                        ty: QueryTokenType::Standard(StandardTokenType::Symbol(new_symbol)),
                        span: new_span,
                    }
                } else {
                    QueryToken {
                        ty: QueryTokenType::Standard(StandardTokenType::Symbol(c.to_string())),
                        span: iter.current_span(),
                    }
                }
            } else {
                QueryToken {
                    ty: QueryTokenType::Standard(StandardTokenType::Symbol(c.to_string())),
                    span: iter.current_span(),
                }
            }
        }
        None => panic!("Unexpected end of file"),
    }
}

fn read_query_command(iter: &mut PeekableStringIterator, options: &Options) -> QueryToken {
    let t = match iter.peek().expect("Unexpected end of query string") {
        '.' => QueryTokenType::Special(SpecialTokenType::Any),
        '*' => QueryTokenType::Special(SpecialTokenType::Star),
        '+' => QueryTokenType::Special(SpecialTokenType::Plus),
        '|' => QueryTokenType::Special(SpecialTokenType::Or),
        '$' => QueryTokenType::Special(SpecialTokenType::End),
        '"' => {
            let ty = QueryTokenType::Special(SpecialTokenType::Regex(read_string_content(iter)));
            return QueryToken {
                ty,
                span: iter.current_span(),
            };
        }
        '(' => {
            assert_eq!(iter.next(), Some('('));
            let tts = QueryTokenType::Special(SpecialTokenType::Nested(tokenize_recur(
                iter, options, true, true,
            )));
            assert_eq!(iter.next(), Some(')'));
            return QueryToken {
                ty: tts,
                span: iter.current_span(),
            };
        }
        c => panic!("Unimplemented query command: {}", c),
    };
    iter.next();
    QueryToken {
        ty: t,
        span: iter.current_span(),
    }
}

#[cfg(test)]
mod tests {
    use crate::tokenizer::*;

    fn t(ty: StandardTokenType, lo: usize, hi: usize) -> StandardToken {
        StandardToken {
            ty,
            span: Span { lo, hi },
        }
    }
    fn q(ty: QueryTokenType, lo: usize, hi: usize) -> QueryToken {
        QueryToken {
            ty,
            span: Span { lo, hi },
        }
    }

    fn test_file(input: &str, expected: Vec<StandardToken>, options: Options) {
        let (tokens, _) = tokenize("foo", input.as_bytes(), &options);
        assert_eq!(
            tokens.iter().map(|t| &t.ty).collect::<Vec<_>>(),
            expected.iter().map(|t| &t.ty).collect::<Vec<_>>()
        );
        assert_eq!(
            tokens.iter().map(|t| &t.span).collect::<Vec<_>>(),
            expected.iter().map(|t| &t.span).collect::<Vec<_>>()
        );
    }

    fn test_query(input: &str, expected: Vec<QueryToken>, options: Options) {
        let (tokens, _) = tokenize_query(input.as_bytes(), &options);
        assert_eq!(
            tokens.iter().map(|t| &t.ty).collect::<Vec<_>>(),
            expected.iter().map(|t| &t.ty).collect::<Vec<_>>()
        );
        assert_eq!(
            tokens.iter().map(|t| &t.span).collect::<Vec<_>>(),
            expected.iter().map(|t| &t.span).collect::<Vec<_>>()
        );
    }

    fn test(input: &str, expected: Vec<StandardToken>) {
        test_file(
            input,
            expected,
            Options::new("js".as_ref(), &["syns", "foo", "foo"]),
        )
    }

    #[test]
    fn simple_tokens() {
        test(
            "foo 123 \"bar\"",
            vec![
                t(StandardTokenType::Identifier("foo".to_string()), 0, 2),
                t(StandardTokenType::Integer(123), 4, 6),
                t(StandardTokenType::StringLiteral("bar".to_string()), 8, 12),
            ],
        );
    }

    #[test]
    fn comments() {
        test(
            "foo /* bar */ baz\ngux //baz",
            vec![
                t(StandardTokenType::Identifier("foo".to_string()), 0, 2),
                t(StandardTokenType::Identifier("baz".to_string()), 14, 16),
                t(StandardTokenType::Identifier("gux".to_string()), 18, 20),
            ],
        );
    }

    #[test]
    fn numbers() {
        test(
            "123 0b101 0x123FG",
            vec![
                t(StandardTokenType::Integer(123), 0, 2),
                t(StandardTokenType::Integer(0b101), 6, 8),
                t(StandardTokenType::Integer(0x123f), 12, 15),
                t(StandardTokenType::Identifier("G".to_string()), 16, 16),
            ],
        );

        test(
            "12.23 2.3e5",
            vec![
                t(StandardTokenType::Float(12.23.into()), 0, 4),
                t(StandardTokenType::Float(230000.0.into()), 6, 10),
            ],
        );
    }

    #[test]
    fn operators() {
        test(
            "+",
            vec![t(StandardTokenType::Symbol("+".to_string()), 0, 0)],
        );
        test(
            "++",
            vec![t(StandardTokenType::Symbol("++".to_string()), 0, 1)],
        );
        test(
            "+ +",
            vec![
                t(StandardTokenType::Symbol("+".to_string()), 0, 0),
                t(StandardTokenType::Symbol("+".to_string()), 2, 2),
            ],
        );
    }

    #[test]
    fn strings() {
        test(
            r#""foo" "bar\"" 'baz\''"#,
            vec![
                t(StandardTokenType::StringLiteral("foo".to_string()), 0, 4),
                t(
                    StandardTokenType::StringLiteral("bar\\\"".to_string()),
                    6,
                    12,
                ),
                t(
                    StandardTokenType::StringLiteral("baz\\'".to_string()),
                    14,
                    20,
                ),
            ],
        );

        test(
            "'foo'",
            vec![t(StandardTokenType::StringLiteral("foo".to_string()), 0, 4)],
        );

        test(
            "\"bar\"",
            vec![t(StandardTokenType::StringLiteral("bar".to_string()), 0, 4)],
        );

        test(
            "\"baz'nt\"",
            vec![t(
                StandardTokenType::StringLiteral("baz'nt".to_string()),
                0,
                7,
            )],
        );

        test(
            "'qux\"d'",
            vec![t(
                StandardTokenType::StringLiteral("qux\"d".to_string()),
                0,
                6,
            )],
        );
    }

    #[test]
    fn regex_literal() {
        test(
            r#"/foo/"#,
            vec![t(StandardTokenType::Regex("foo".to_string()), 0, 4)],
        );

        test(
            r#"/fo\/o/"#,
            vec![t(StandardTokenType::Regex("fo\\/o".to_string()), 0, 6)],
        );

        test(
            r#"a/b/"#,
            vec![
                t(StandardTokenType::Identifier("a".to_string()), 0, 0),
                t(StandardTokenType::Symbol("/".to_string()), 1, 1),
                t(StandardTokenType::Identifier("b".to_string()), 2, 2),
                t(StandardTokenType::Symbol("/".to_string()), 3, 3),
            ],
        );

        test(
            r#"a+/b/"#,
            vec![
                t(StandardTokenType::Identifier("a".to_string()), 0, 0),
                t(StandardTokenType::Symbol("+".to_string()), 1, 1),
                t(StandardTokenType::Regex("b".to_string()), 2, 4),
            ],
        );
        test(
            r#"(a+b)/c"#,
            vec![
                t(StandardTokenType::Symbol("(".to_string()), 0, 0),
                t(StandardTokenType::Identifier("a".to_string()), 1, 1),
                t(StandardTokenType::Symbol("+".to_string()), 2, 2),
                t(StandardTokenType::Identifier("b".to_string()), 3, 3),
                t(StandardTokenType::Symbol(")".to_string()), 4, 4),
                t(StandardTokenType::Symbol("/".to_string()), 5, 5),
                t(StandardTokenType::Identifier("c".to_string()), 6, 6),
            ],
        );
    }

    #[test]
    fn query_tokens() {
        test(
            r#"\.\+\*\"foo.*bar""#,
            vec![
                t(StandardTokenType::Symbol("\\.\\+\\*\\".to_string()), 0, 6),
                t(
                    StandardTokenType::StringLiteral("foo.*bar".to_string()),
                    7,
                    16,
                ),
            ],
        );

        let opts = Options::new("js".as_ref(), &["syns", "foo", "foo"]);
        test_query(
            r#"\.\+\*\"foo.*bar"\$\|"#,
            vec![
                q(QueryTokenType::Special(SpecialTokenType::Any), 0, 1),
                q(QueryTokenType::Special(SpecialTokenType::Plus), 2, 3),
                q(QueryTokenType::Special(SpecialTokenType::Star), 4, 5),
                q(
                    QueryTokenType::Special(SpecialTokenType::Regex("foo.*bar".to_string())),
                    7,
                    16,
                ),
                q(QueryTokenType::Special(SpecialTokenType::End), 17, 18),
                q(QueryTokenType::Special(SpecialTokenType::Or), 19, 20),
            ],
            opts,
        );
    }

    #[test]
    fn plus_after_regex() {
        let opts = Options::new("js".as_ref(), &["syns", "foo", "foo"]);

        test_query(
            r#"\"INSERT .*" +"#,
            vec![
                q(
                    QueryTokenType::Special(SpecialTokenType::Regex("INSERT .*".to_string())),
                    1,
                    11,
                ),
                q(
                    QueryTokenType::Standard(StandardTokenType::Symbol("+".to_string())),
                    13,
                    13,
                ),
            ],
            opts,
        );
    }
}
