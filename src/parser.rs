//! Parsers for a stream of tokens.

use log::debug;
use regex::Regex;
use std::convert::TryInto;
use std::io::Read;
use std::iter::Peekable;

use crate::options::Options;
use crate::psi::{PeekableStringIterator, Span};
use crate::tokenizer::{
    tokenize, tokenize_query, QueryToken, QueryTokenType, SpecialTokenType, StandardToken,
    StandardTokenType,
};

/// Abstract syntax tree for source code.
#[derive(Clone, Debug)]
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

fn parse(
    options: &Options,
    iter: &mut Peekable<impl Iterator<Item = StandardToken>>,
    recur: bool,
) -> Vec<Ast> {
    let mut res = Vec::new();
    loop {
        if let Some(StandardTokenType::Symbol(c)) = iter.peek().map(|t| &t.ty) {
            if recur && options.is_close_paren(c) {
                break;
            }
        }
        if let Some(token) = iter.next() {
            match &token.ty {
                StandardTokenType::Symbol(c) if options.is_open_paren(c) => {
                    let content = parse(options, iter, true);
                    let cp = iter.next();
                    res.push(Ast::Delimited {
                        op: token,
                        content,
                        cp,
                    });
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
        parse(options, &mut tokens.into_iter().peekable(), false),
        iter,
    )
}

/// Abstract syntax tree for query strings.
#[derive(Clone, Debug)]
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
    Regex(Regex),
}

fn parse_query_ast(
    options: &Options,
    iter: &mut Peekable<impl Iterator<Item = QueryToken>>,
    recur: bool,
) -> Vec<ParsedAstMatcher> {
    let mut res = Vec::new();
    loop {
        if let Some(QueryTokenType::Standard(StandardTokenType::Symbol(c))) =
            iter.peek().map(|t| &t.ty)
        {
            if recur && options.is_close_paren(c) {
                break;
            }
        }
        if let Some(token) = iter.next() {
            match &token.ty {
                QueryTokenType::Standard(StandardTokenType::Symbol(c))
                    if options.is_open_paren(c) =>
                {
                    let op = StandardToken {
                        ty: StandardTokenType::Symbol(c.clone()),
                        span: token.span,
                    };
                    let content = parse_query_ast(options, iter, true);
                    let cp = iter.next().map(|t| {
                        t.try_into()
                            .expect("Expected closing paren but got special token")
                    });
                    res.push(ParsedAstMatcher::Delimited { op, content, cp });
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
                    let next = parse_query_ast(options, iter, true);
                    res.push(ParsedAstMatcher::Or(
                        prev,
                        Box::new(ParsedAstMatcher::Nested(next)),
                    ));
                }
                QueryTokenType::Special(SpecialTokenType::Nested(list)) => {
                    let list =
                        parse_query_ast(options, &mut list.clone().into_iter().peekable(), false);
                    res.push(ParsedAstMatcher::Nested(list));
                }
                QueryTokenType::Special(SpecialTokenType::Regex(content)) => {
                    match Regex::new(content) {
                        Ok(r) => {
                            let matcher = ParsedAstMatcher::Regex(r);
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
    let parsed = parse_query_ast(options, &mut tokens.into_iter().peekable(), false);
    debug!("Parsed query: {:#?}", parsed);

    (parsed, iter)
}
