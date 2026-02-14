//! Parsers for a stream of tokens.

use log::debug;
use regex::Regex;
use std::convert::TryInto;
use std::io::Read;

use crate::multipeek_putbackn::{multipeek_put_back_n, MultiPeekPutBackN};
use crate::options::Options;
use crate::psi::{PeekableStringIterator, Span};
use crate::tokenizer::{
    tokenize, tokenize_query, QueryToken, QueryTokenType, SpecialTokenType, StandardToken,
    StandardTokenType,
};
use crate::wrappers::RegexEq;

/// Abstract syntax tree for source code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ast {
    /// A single token.
    Token(StandardToken),
    /// Paren-delimited block of code.
    Delimited {
        /// Opening paren of the block.
        op: StandardToken,
        /// Closing paren of the block, or None in case of EOF.
        cp: Option<StandardToken>,
        /// Content of the block.
        content: Vec<Ast>,
    },
}

impl Ast {
    /// Get the span of this AST node.
    pub fn span(&self) -> Span {
        match self {
            Ast::Token(token) => token.span,
            Ast::Delimited { op, cp, content } => op.span.merge(
                &cp.as_ref()
                    .map(|t| t.span)
                    .or_else(|| content.last().map(|t| t.span()))
                    .unwrap_or(op.span),
            ),
        }
    }
}

fn is_open_type_param(c: &str, inside_type_param: bool) -> bool {
    c == "<" && inside_type_param
}

fn is_close_type_param(c: &str, inside_type_param: bool) -> bool {
    c == ">" && inside_type_param
}

fn last_is_identifier(res: &[Ast]) -> bool {
    res.is_empty()
        || matches!(
            res.last(),
            Some(Ast::Token(t)) if matches!(&t.ty, StandardTokenType::Identifier(_))
        )
}

fn last_matcher_is_identifier(res: &[ParsedAstMatcher]) -> bool {
    res.is_empty()
        || matches!(
            res.last(),
            Some(ParsedAstMatcher::Token(t)) if matches!(&t.ty, StandardTokenType::Identifier(_))
        )
}

fn peek_is_type_params(
    options: &Options,
    iter: &mut MultiPeekPutBackN<impl Iterator<Item = StandardToken>>,
) -> bool {
    let mut depth = 1usize;
    loop {
        match iter.peek() {
            Some(t) => match &t.ty {
                StandardTokenType::Symbol(s) => {
                    for c in s.chars() {
                        match c {
                            '<' => depth += 1,
                            '>' => {
                                depth -= 1;
                                if depth == 0 {
                                    return true;
                                }
                            }
                            c if options.is_open_paren(&c.to_string()) => depth += 1,
                            c if options.is_close_paren(&c.to_string()) => {
                                depth -= 1;
                                if depth == 0 {
                                    return false;
                                }
                            }
                            ',' | '.' | ':' | ';' | '?' | '&' | '|' => {}
                            _ => return false,
                        }
                    }
                }
                StandardTokenType::Identifier(_) => {}
                _ => return false,
            },
            None => return false,
        }
    }
}

fn peek_is_type_params_query(
    options: &Options,
    iter: &mut MultiPeekPutBackN<impl Iterator<Item = QueryToken>>,
) -> bool {
    let mut depth = 1usize;
    loop {
        match iter.peek() {
            Some(t) => match &t.ty {
                QueryTokenType::Standard(StandardTokenType::Symbol(s)) => {
                    for c in s.chars() {
                        match c {
                            '<' => depth += 1,
                            '>' => {
                                depth -= 1;
                                if depth == 0 {
                                    return true;
                                }
                            }
                            c if options.is_open_paren(&c.to_string()) => depth += 1,
                            c if options.is_close_paren(&c.to_string()) => {
                                depth -= 1;
                                if depth == 0 {
                                    return false;
                                }
                            }
                            ',' | '.' | ':' | ';' | '?' | '&' | '|' => {}
                            _ => return false,
                        }
                    }
                }
                QueryTokenType::Standard(StandardTokenType::Identifier(_)) => {}
                QueryTokenType::Special(_) => {}
                _ => return false,
            },
            None => return false,
        }
    }
}

fn split_to_symbols(options: &Options, s: &str, mut span: Span) -> Vec<StandardToken> {
    let mut res = Vec::new();

    let mut iter = s.chars();
    while let Some(c) = iter.next() {
        if options.is_open_paren(&c.to_string())
            || options.is_close_paren(&c.to_string())
            || c == '<'
            || c == '>'
        {
            res.push(StandardToken {
                ty: StandardTokenType::Symbol(c.to_string()),
                span: Span {
                    lo: span.lo,
                    hi: span.lo,
                },
            });
            span = Span {
                lo: span.lo + 1,
                hi: span.hi,
            };
        } else {
            res.push(StandardToken {
                ty: StandardTokenType::Symbol(format!("{}{}", c, iter.as_str())),
                span,
            });
            break;
        }
    }

    res
}

fn parse(
    options: &Options,
    iter: &mut MultiPeekPutBackN<impl Iterator<Item = StandardToken>>,
    recur: bool,
    inside_type_param: bool,
) -> Vec<Ast> {
    let mut res = Vec::new();
    loop {
        if let Some(StandardToken {
            ty: StandardTokenType::Symbol(s),
            span,
        }) = iter.peek()
        {
            if recur && (options.is_close_paren(s) || is_close_type_param(s, inside_type_param)) {
                break;
            }
            if inside_type_param && s.chars().count() > 1 {
                let syms = split_to_symbols(options, s, *span);
                if syms.len() > 1 {
                    assert!(iter.next().is_some());
                    for sym in syms.into_iter().rev() {
                        iter.put_back(sym);
                    }
                    continue;
                }
            }
        }
        if let Some(token) = iter.next() {
            match &token.ty {
                StandardTokenType::Symbol(c)
                    if options.is_open_paren(c) || is_open_type_param(c, inside_type_param) =>
                {
                    let content = parse(options, iter, true, inside_type_param);
                    let cp = iter.next();
                    res.push(Ast::Delimited {
                        op: token,
                        content,
                        cp,
                    });
                }
                StandardTokenType::Symbol(c)
                    if c == "<>" && options.type_parameter_parsing && last_is_identifier(&res) =>
                {
                    res.push(Ast::Delimited {
                        op: StandardToken {
                            ty: StandardTokenType::Symbol("<".to_string()),
                            span: Span {
                                lo: token.span.lo,
                                hi: token.span.lo,
                            },
                        },
                        content: vec![],
                        cp: Some(StandardToken {
                            ty: StandardTokenType::Symbol(">".to_string()),
                            span: Span {
                                lo: token.span.hi,
                                hi: token.span.hi,
                            },
                        }),
                    });
                }
                StandardTokenType::Symbol(c) if c == "<" && options.type_parameter_parsing => {
                    if last_is_identifier(&res) && peek_is_type_params(options, iter) {
                        iter.reset_peek();
                        let content = parse(options, iter, true, true);
                        let cp = iter.next();
                        res.push(Ast::Delimited {
                            op: token,
                            content,
                            cp,
                        });
                    } else {
                        iter.reset_peek();
                        res.push(Ast::Token(token));
                    }
                }
                _ => res.push(Ast::Token(token)),
            }
        } else {
            break;
        }
    }
    res
}

/// Parse a source file into a list of ASTs.
pub fn parse_file<R: Read>(file: R, options: &Options) -> (Vec<Ast>, PeekableStringIterator) {
    let (tokens, iter) = tokenize("filename", file, options);
    (
        parse(options, &mut multipeek_put_back_n(tokens), false, false),
        iter,
    )
}

/// Abstract syntax tree for query strings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParsedAstMatcher {
    /// Single token.
    Token(StandardToken),
    /// Paren-delimited block.
    Delimited {
        /// Opening paren of the block.
        op: StandardToken,
        /// Closing paren of the block, or None in case of EOF.
        cp: Option<StandardToken>,
        /// Content of the block.
        content: Vec<ParsedAstMatcher>,
    },
    /// Match a single any token
    Any,
    /// Match end of group
    End,
    /// Match `ParsedAstMatcher` one or more times
    Plus(Box<ParsedAstMatcher>),
    /// Match `ParsedAstMatcher` zero or more times
    Star(Box<ParsedAstMatcher>),
    /// Match `ParsedAstMatcher` zero or one times
    QuestionMark(Box<ParsedAstMatcher>),
    /// Match either `ParsedAstMatcher`
    Or(Box<ParsedAstMatcher>, Box<ParsedAstMatcher>),
    /// Grouped `ParsedAstMatcher`s
    Nested(Vec<ParsedAstMatcher>),
    /// Match string literal by regex
    Regex(RegexEq),
}

fn parse_query_ast(
    options: &Options,
    iter: &mut MultiPeekPutBackN<impl Iterator<Item = QueryToken>>,
    recur: bool,
    inside_type_param: bool,
) -> Vec<ParsedAstMatcher> {
    let mut res = Vec::new();
    loop {
        if let Some(QueryToken {
            ty: QueryTokenType::Standard(StandardTokenType::Symbol(s)),
            span,
        }) = iter.peek()
        {
            if recur && (options.is_close_paren(s) || is_close_type_param(s, inside_type_param)) {
                break;
            }
            if inside_type_param && s.chars().count() > 1 {
                let syms = split_to_symbols(options, s, *span);
                if syms.len() > 1 {
                    assert!(iter.next().is_some());
                    for sym in syms.into_iter().rev() {
                        iter.put_back(QueryToken {
                            ty: QueryTokenType::Standard(sym.ty),
                            span: sym.span,
                        });
                    }
                    continue;
                }
            }
        }
        if let Some(token) = iter.next() {
            match &token.ty {
                QueryTokenType::Standard(StandardTokenType::Symbol(c))
                    if options.is_open_paren(c) || is_open_type_param(c, inside_type_param) =>
                {
                    let op = StandardToken {
                        ty: StandardTokenType::Symbol(c.clone()),
                        span: token.span,
                    };
                    let content = parse_query_ast(options, iter, true, inside_type_param);
                    let cp = iter.next().map(|t| {
                        t.try_into()
                            .expect("Expected closing paren but got special token")
                    });
                    res.push(ParsedAstMatcher::Delimited { op, content, cp });
                }
                QueryTokenType::Standard(StandardTokenType::Symbol(c))
                    if c == "<>"
                        && options.type_parameter_parsing
                        && last_matcher_is_identifier(&res) =>
                {
                    res.push(ParsedAstMatcher::Delimited {
                        op: StandardToken {
                            ty: StandardTokenType::Symbol("<".to_string()),
                            span: Span {
                                lo: token.span.lo,
                                hi: token.span.lo,
                            },
                        },
                        content: vec![],
                        cp: Some(StandardToken {
                            ty: StandardTokenType::Symbol(">".to_string()),
                            span: Span {
                                lo: token.span.hi,
                                hi: token.span.hi,
                            },
                        }),
                    });
                }
                QueryTokenType::Standard(StandardTokenType::Symbol(c))
                    if c == "<" && options.type_parameter_parsing =>
                {
                    if last_matcher_is_identifier(&res) && peek_is_type_params_query(options, iter)
                    {
                        iter.reset_peek();
                        let op = StandardToken {
                            ty: StandardTokenType::Symbol(c.clone()),
                            span: token.span,
                        };
                        let content = parse_query_ast(options, iter, true, true);
                        let cp = iter.next().map(|t| {
                            t.try_into()
                                .expect("Expected closing paren but got special token")
                        });
                        res.push(ParsedAstMatcher::Delimited { op, content, cp });
                    } else {
                        iter.reset_peek();
                        res.push(ParsedAstMatcher::Token(StandardToken {
                            span: token.span,
                            ty: StandardTokenType::Symbol(c.clone()),
                        }));
                    }
                }
                QueryTokenType::Standard(ty) => res.push(ParsedAstMatcher::Token(StandardToken {
                    span: token.span,
                    ty: ty.clone(),
                })),
                QueryTokenType::Special(SpecialTokenType::Any) => {
                    res.push(ParsedAstMatcher::Any);
                }
                QueryTokenType::Special(SpecialTokenType::End) => {
                    res.push(ParsedAstMatcher::End);
                }
                QueryTokenType::Special(SpecialTokenType::Plus) => {
                    let prev = res.pop().unwrap_or(ParsedAstMatcher::Any);
                    res.push(ParsedAstMatcher::Plus(Box::new(prev)));
                }
                QueryTokenType::Special(SpecialTokenType::QuestionMark) => {
                    let prev = res.pop().unwrap_or(ParsedAstMatcher::Any);
                    res.push(ParsedAstMatcher::QuestionMark(Box::new(prev)));
                }
                QueryTokenType::Special(SpecialTokenType::Star) => {
                    let prev = res.pop().unwrap_or(ParsedAstMatcher::Any);
                    res.push(ParsedAstMatcher::Star(Box::new(prev)));
                }
                QueryTokenType::Special(SpecialTokenType::Or) => {
                    let prev = if res.len() <= 1 {
                        Box::new(res.pop().unwrap_or(ParsedAstMatcher::Any))
                    } else {
                        let inner = res;
                        res = Vec::new();
                        Box::new(ParsedAstMatcher::Nested(inner))
                    };
                    let next = parse_query_ast(options, iter, true, inside_type_param);
                    res.push(ParsedAstMatcher::Or(
                        prev,
                        Box::new(ParsedAstMatcher::Nested(next)),
                    ));
                }
                QueryTokenType::Special(SpecialTokenType::Nested(list)) => {
                    let list = parse_query_ast(
                        options,
                        &mut multipeek_put_back_n(list.clone()),
                        false,
                        inside_type_param,
                    );
                    res.push(ParsedAstMatcher::Nested(list));
                }
                QueryTokenType::Special(SpecialTokenType::Regex(content)) => {
                    match Regex::new(content) {
                        Ok(r) => {
                            let matcher = ParsedAstMatcher::Regex(RegexEq(r));
                            res.push(matcher);
                        }
                        Err(e) => {
                            println!("{}", e);
                            std::process::exit(1);
                        }
                    }
                }
            }
        } else {
            break;
        }
    }
    res
}

/// Parse a query into a list of query ASTs.
pub fn parse_query<R: Read>(
    file: R,
    options: &Options,
) -> (Vec<ParsedAstMatcher>, PeekableStringIterator) {
    debug!("Tokenizing query");
    let (tokens, iter) = tokenize_query(file, options);
    debug!("Tokenized query: {:#?}", tokens);
    debug!("Parsing query");
    let parsed = parse_query_ast(options, &mut multipeek_put_back_n(tokens), false, false);
    debug!("Parsed query: {:#?}", parsed);

    (parsed, iter)
}

#[cfg(test)]
mod tests_ast {
    use super::*;

    fn parse_str(input: &str, ext: &str) -> Vec<Ast> {
        let options = Options::new(ext.as_ref(), &["syns", "query", "file"]);
        let (tokens, _) = tokenize("test", input.as_bytes(), &options);
        parse(
            &options,
            &mut multipeek_put_back_n(tokens.into_iter()),
            false,
            false,
        )
    }

    /// Strip all spans from an AST tree so we can compare structure only.
    fn strip_spans(ast: &[Ast]) -> Vec<Ast> {
        let blank = Span { lo: 0, hi: 0 };
        ast.iter()
            .map(|node| match node {
                Ast::Token(t) => Ast::Token(StandardToken {
                    ty: t.ty.clone(),
                    span: blank,
                }),
                Ast::Delimited { op, cp, content } => Ast::Delimited {
                    op: StandardToken {
                        ty: op.ty.clone(),
                        span: blank,
                    },
                    cp: cp.as_ref().map(|t| StandardToken {
                        ty: t.ty.clone(),
                        span: blank,
                    }),
                    content: strip_spans(content),
                },
            })
            .collect()
    }

    fn tok(ty: StandardTokenType) -> Ast {
        Ast::Token(StandardToken {
            ty,
            span: Span { lo: 0, hi: 0 },
        })
    }

    fn delim(op: &str, content: Vec<Ast>, cp: &str) -> Ast {
        let blank = Span { lo: 0, hi: 0 };
        Ast::Delimited {
            op: StandardToken {
                ty: StandardTokenType::Symbol(op.to_string()),
                span: blank,
            },
            cp: Some(StandardToken {
                ty: StandardTokenType::Symbol(cp.to_string()),
                span: blank,
            }),
            content,
        }
    }

    fn ident(s: &str) -> Ast {
        tok(StandardTokenType::Identifier(s.to_string()))
    }

    fn sym(s: &str) -> Ast {
        tok(StandardTokenType::Symbol(s.to_string()))
    }

    #[test]
    fn parse_braces() {
        let ast = parse_str("foo { bar(); }", "js");
        assert_eq!(
            strip_spans(&ast),
            vec![
                ident("foo"),
                delim(
                    "{",
                    vec![ident("bar"), delim("(", vec![], ")"), sym(";"),],
                    "}"
                ),
            ]
        );
    }

    #[test]
    fn parse_type_parameters() {
        let ast = parse_str("const a: Foo<T>; if(a < b) { foo<T>(); }", "js");
        assert_eq!(
            strip_spans(&ast),
            vec![
                ident("const"),
                ident("a"),
                sym(":"),
                ident("Foo"),
                delim("<", vec![ident("T")], ">"),
                sym(";"),
                ident("if"),
                delim("(", vec![ident("a"), sym("<"), ident("b"),], ")"),
                delim(
                    "{",
                    vec![
                        ident("foo"),
                        delim("<", vec![ident("T")], ">"),
                        delim("(", vec![], ")"),
                        sym(";"),
                    ],
                    "}"
                ),
            ]
        );
    }

    #[test]
    fn complex_type_params() {
        let ast = parse_str("a: Bar<Foo<T>>; b: Foo<Bar<{foo: string[]}>>", "js");

        assert_eq!(
            strip_spans(&ast),
            vec![
                ident("a"),
                sym(":"),
                ident("Bar"),
                delim(
                    "<",
                    vec![ident("Foo"), delim("<", vec![ident("T")], ">"),],
                    ">"
                ),
                sym(";"),
                ident("b"),
                sym(":"),
                ident("Foo"),
                delim(
                    "<",
                    vec![
                        ident("Bar"),
                        delim(
                            "<",
                            vec![delim(
                                "{",
                                vec![
                                    ident("foo"),
                                    sym(":"),
                                    ident("string"),
                                    delim("[", vec![], "]"),
                                ],
                                "}"
                            ),],
                            ">"
                        ),
                    ],
                    ">"
                ),
            ]
        );
    }
}

#[cfg(test)]
mod tests_query {
    use super::*;

    fn parse_str(input: &str, ext: &str) -> Vec<ParsedAstMatcher> {
        let options = Options::new(ext.as_ref(), &["syns", "query", "file"]);
        let (tokens, _) = tokenize_query(input.as_bytes(), &options);
        parse_query_ast(
            &options,
            &mut multipeek_put_back_n(tokens.into_iter()),
            false,
            false,
        )
    }

    /// Strip all spans from an AST tree so we can compare structure only.
    fn strip_span(node: &ParsedAstMatcher) -> ParsedAstMatcher {
        let blank = Span { lo: 0, hi: 0 };
        match node {
            ParsedAstMatcher::Token(t) => ParsedAstMatcher::Token(StandardToken {
                ty: t.ty.clone(),
                span: blank,
            }),
            ParsedAstMatcher::Delimited { op, cp, content } => ParsedAstMatcher::Delimited {
                op: StandardToken {
                    ty: op.ty.clone(),
                    span: blank,
                },
                cp: cp.as_ref().map(|t| StandardToken {
                    ty: t.ty.clone(),
                    span: blank,
                }),
                content: strip_spans(content),
            },

            ParsedAstMatcher::Any => ParsedAstMatcher::Any,
            ParsedAstMatcher::End => ParsedAstMatcher::End,
            ParsedAstMatcher::Plus(content) => {
                ParsedAstMatcher::Plus(Box::new(strip_span(content)))
            }
            ParsedAstMatcher::Star(content) => {
                ParsedAstMatcher::Star(Box::new(strip_span(content)))
            }
            ParsedAstMatcher::QuestionMark(content) => {
                ParsedAstMatcher::QuestionMark(Box::new(strip_span(content)))
            }
            ParsedAstMatcher::Or(left, right) => {
                ParsedAstMatcher::Or(Box::new(strip_span(left)), Box::new(strip_span(right)))
            }
            ParsedAstMatcher::Nested(content) => ParsedAstMatcher::Nested(strip_spans(content)),
            ParsedAstMatcher::Regex(regex) => ParsedAstMatcher::Regex(regex.clone()),
        }
    }

    /// Strip all spans from an AST tree so we can compare structure only.
    fn strip_spans(ast: &[ParsedAstMatcher]) -> Vec<ParsedAstMatcher> {
        ast.iter().map(strip_span).collect()
    }

    fn tok(ty: StandardTokenType) -> ParsedAstMatcher {
        ParsedAstMatcher::Token(StandardToken {
            ty,
            span: Span { lo: 0, hi: 0 },
        })
    }

    fn delim(op: &str, content: Vec<ParsedAstMatcher>, cp: &str) -> ParsedAstMatcher {
        let blank = Span { lo: 0, hi: 0 };
        ParsedAstMatcher::Delimited {
            op: StandardToken {
                ty: StandardTokenType::Symbol(op.to_string()),
                span: blank,
            },
            cp: Some(StandardToken {
                ty: StandardTokenType::Symbol(cp.to_string()),
                span: blank,
            }),
            content,
        }
    }

    fn ident(s: &str) -> ParsedAstMatcher {
        tok(StandardTokenType::Identifier(s.to_string()))
    }

    fn sym(s: &str) -> ParsedAstMatcher {
        tok(StandardTokenType::Symbol(s.to_string()))
    }

    #[test]
    fn parse_type_parameters() {
        let ast = parse_str("const a: Foo<T>; if(a < b) { foo<T>(); }", "js");
        assert_eq!(
            strip_spans(&ast),
            vec![
                ident("const"),
                ident("a"),
                sym(":"),
                ident("Foo"),
                delim("<", vec![ident("T")], ">"),
                sym(";"),
                ident("if"),
                delim("(", vec![ident("a"), sym("<"), ident("b"),], ")"),
                delim(
                    "{",
                    vec![
                        ident("foo"),
                        delim("<", vec![ident("T")], ">"),
                        delim("(", vec![], ")"),
                        sym(";"),
                    ],
                    "}"
                ),
            ]
        );
    }

    #[test]
    fn type_params_query() {
        let ast = parse_str("Foo<Bar<T>>", "js");

        assert_eq!(
            strip_spans(&ast),
            vec![
                ident("Foo"),
                delim(
                    "<",
                    vec![ident("Bar"), delim("<", vec![ident("T")], ">"),],
                    ">"
                ),
            ]
        );
    }
}
