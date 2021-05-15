use crate::options::Options;
use crate::psi::{PeekableStringIterator, Span};
use std::io::Read;
use std::str::FromStr;

#[derive(Clone, Debug, PartialEq)]
pub enum TokenType {
    Identifier(String),
    Integer(i128),
    Float(f64),
    StringLiteral(String),
    Symbol(String),
    Any,
    Star,
    Plus,
    Regex(String),
}

#[derive(Clone, Debug)]
pub struct Token {
    pub ty: TokenType,
    pub span: Span,
}

pub fn tokenize<R: Read>(
    filename: &str,
    mut content: R,
    options: &Options,
) -> (Vec<Token>, PeekableStringIterator) {
    let mut buf = String::new();
    content.read_to_string(&mut buf).unwrap();
    let mut iter = PeekableStringIterator::new(filename.to_string(), buf);
    let mut res = Vec::new();
    while iter.peek().is_some() {
        if options
            .single_line_comments
            .iter()
            .any(|c| iter.starts_with(c))
        {
            flush_single_line(&mut iter);
            continue;
        }
        if let Some((start, end)) = options
            .multi_line_comments
            .iter()
            .find(|(start, _)| iter.starts_with(start))
        {
            flush_multi_line_comment(&mut iter, start, end);
            continue;
        }
        let token = match iter.peek().unwrap() {
            _ if options.string_characters.iter().any(|c| iter.starts_with(c)) => read_string(&mut iter),
            '\\' if options.parse_as_query => read_query_command(&mut iter),
            ' ' | '\t' | '\n' => {
                iter.next();
                continue;
            }
            'a'..='z' | 'A'..='Z' | '_' => read_identifier(&mut iter),
            '0'..='9' => read_number(&mut iter, options),
            _ => read_other(&mut iter),
        };
        res.push(token);
    }
    (res, iter)
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

fn read_number(iter: &mut PeekableStringIterator, options: &Options) -> Token {
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
        Token {
            ty: TokenType::Integer(i128::from_str_radix(&content, radix).unwrap()),
            span,
        }
    } else {
        Token {
            ty: TokenType::Float(f64::from_str(&content).unwrap()),
            span,
        }
    }
}

fn read_string_content(iter: &mut PeekableStringIterator) -> String {
    let str_end = iter.next_new_span().unwrap();

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

fn read_string(iter: &mut PeekableStringIterator) -> Token {
    let content = read_string_content(iter);
    Token {
        ty: TokenType::StringLiteral(content),
        span: iter.current_span(),
    }
}

fn read_identifier(iter: &mut PeekableStringIterator) -> Token {
    let (content, span) = iter.collect_while(|c| {
        matches!(c,
            '0'..='9' | 'a'..='z' | 'A'..='Z' | '_'
        )
    });

    Token {
        ty: TokenType::Identifier(content),
        span,
    }
}

fn read_other(iter: &mut PeekableStringIterator) -> Token {
    match iter.next_new_span() {
        Some(c) => Token {
            ty: TokenType::Symbol(c.to_string()),
            span: iter.current_span(),
        },
        None => panic!("Unexpected end of file"),
    }
}

fn read_query_command(iter: &mut PeekableStringIterator) -> Token {
    assert_eq!(iter.next_new_span(), Some('\\'));
    let t = match iter.peek().unwrap() {
        '.' => TokenType::Any,
        '*' => TokenType::Star,
        '+' => TokenType::Plus,
        '"' => {
            let ty = TokenType::Regex(read_string_content(iter));
            return Token {
                ty,
                span: iter.current_span(),
            };
        }
        _ => unimplemented!(),
    };
    iter.next();
    Token {
        ty: t,
        span: iter.current_span(),
    }
}

#[cfg(test)]
mod tests {
    use crate::tokenizer::*;

    fn t(ty: TokenType, lo: usize, hi: usize) -> Token {
        Token {
            ty,
            span: Span { lo, hi },
        }
    }

    fn test_opts(input: &str, expected: Vec<Token>, options: Options) {
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

    fn test(input: &str, expected: Vec<Token>) {
        test_opts(input, expected, Options::new(&["syns", "foo", "foo"]))
    }

    #[test]
    fn simple_tokens() {
        test(
            "foo 123 \"bar\"",
            vec![
                t(TokenType::Identifier("foo".to_string()), 0, 2),
                t(TokenType::Integer(123), 4, 6),
                t(TokenType::StringLiteral("bar".to_string()), 8, 12),
            ],
        );
    }

    #[test]
    fn comments() {
        test(
            "foo /* bar */ baz\ngux //baz",
            vec![
                t(TokenType::Identifier("foo".to_string()), 0, 2),
                t(TokenType::Identifier("baz".to_string()), 14, 16),
                t(TokenType::Identifier("gux".to_string()), 18, 20),
            ],
        );
    }

    #[test]
    fn numbers() {
        test(
            "123 0b101 0x123FG",
            vec![
                t(TokenType::Integer(123), 0, 2),
                t(TokenType::Integer(0b101), 6, 8),
                t(TokenType::Integer(0x123f), 12, 15),
                t(TokenType::Identifier("G".to_string()), 16, 16),
            ],
        );

        test(
            "12.23 2.3e5",
            vec![
                t(TokenType::Float(12.23), 0, 4),
                t(TokenType::Float(230000.0), 6, 10),
            ],
        );
    }

    #[test]
    fn operators() {
        test("+", vec![t(TokenType::Symbol("+".to_string()), 0, 0)]);
    }

    #[test]
    fn strings() {
        test(
            r#""foo" "bar\"" 'baz\''"#,
            vec![
                t(TokenType::StringLiteral("foo".to_string()), 0, 4),
                t(TokenType::StringLiteral("bar\\\"".to_string()), 6, 12),
                t(TokenType::StringLiteral("baz\\'".to_string()), 14, 20),
            ],
        );
    }

    #[test]
    fn query_tokens() {
        test(
            r#"\.\+\*\"foo.*bar""#,
            vec![
                t(TokenType::Symbol("\\".to_string()), 0, 0),
                t(TokenType::Symbol(".".to_string()), 1, 1),
                t(TokenType::Symbol("\\".to_string()), 2, 2),
                t(TokenType::Symbol("+".to_string()), 3, 3),
                t(TokenType::Symbol("\\".to_string()), 4, 4),
                t(TokenType::Symbol("*".to_string()), 5, 5),
                t(TokenType::Symbol("\\".to_string()), 6, 6),
                t(TokenType::StringLiteral("foo.*bar".to_string()), 7, 16),
            ],
        );

        let mut opts = Options::new(&["syns", "foo", "foo"]);
        opts.parse_as_query = true;
        test_opts(
            r#"\.\+\*\"foo.*bar""#,
            vec![
                t(TokenType::Any, 0, 1),
                t(TokenType::Plus, 2, 3),
                t(TokenType::Star, 4, 5),
                t(TokenType::Regex("foo.*bar".to_string()), 7, 16),
            ],
            opts,
        );
    }
}
