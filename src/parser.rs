use log::debug;
use regex::Regex;
use std::convert::TryInto;
use std::io::Read;
use std::iter::Peekable;

use crate::options::Options;
use crate::psi::{PeekableStringIterator, Span};
use crate::tokenizer::{
    tokenize, tokenize_query, QueryToken, SpecialTokenType, StandardToken, StandardTokenType,
    TokenType,
};

#[derive(Clone, Debug)]
pub enum Ast {
    Token(StandardToken),
    Delimited {
        op: StandardToken,
        cp: Option<StandardToken>,
        content: Vec<Ast>,
    },
}

impl Ast {
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

pub fn parse_file<R: Read>(file: R, options: &Options) -> (Vec<Ast>, PeekableStringIterator) {
    let (tokens, iter) = tokenize("filename", file, options);
    (
        parse(options, &mut tokens.into_iter().peekable(), false),
        iter,
    )
}

#[derive(Clone, Debug)]
pub enum ParsedAstMatcher {
    Token(StandardToken),
    Delimited {
        op: StandardToken,
        cp: Option<StandardToken>,
        content: Vec<ParsedAstMatcher>,
    },
    Any,
    Plus(Box<ParsedAstMatcher>),
    Star(Box<ParsedAstMatcher>),
    Nested(Vec<ParsedAstMatcher>),
    Regex(Regex),
}

fn parse_query_ast(
    options: &Options,
    iter: &mut Peekable<impl Iterator<Item = QueryToken>>,
    recur: bool,
) -> Vec<ParsedAstMatcher> {
    let mut res = Vec::new();
    loop {
        if let Some(TokenType::Standard(StandardTokenType::Symbol(c))) = iter.peek().map(|t| &t.ty)
        {
            if recur && options.is_close_paren(c) {
                break;
            }
        }
        if let Some(token) = iter.next() {
            match &token.ty {
                TokenType::Standard(StandardTokenType::Symbol(c)) if options.is_open_paren(c) => {
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
                TokenType::Standard(ty) => res.push(ParsedAstMatcher::Token(StandardToken {
                    span: token.span,
                    ty: ty.clone(),
                })),
                TokenType::Special(SpecialTokenType::Any) => {
                    res.push(ParsedAstMatcher::Any);
                }
                TokenType::Special(SpecialTokenType::Plus) => {
                    let prev = res.pop().unwrap_or(ParsedAstMatcher::Any);
                    res.push(ParsedAstMatcher::Plus(Box::new(prev)));
                }
                TokenType::Special(SpecialTokenType::Star) => {
                    let prev = res.pop().unwrap_or(ParsedAstMatcher::Any);
                    res.push(ParsedAstMatcher::Star(Box::new(prev)));
                }
                TokenType::Special(SpecialTokenType::Nested(list)) => {
                    let list =
                        parse_query_ast(options, &mut list.clone().into_iter().peekable(), false);
                    res.push(ParsedAstMatcher::Nested(list));
                }
                TokenType::Special(SpecialTokenType::Regex(content)) => {
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

pub fn parse_query<R: Read>(
    file: R,
    options: &Options,
) -> (Vec<ParsedAstMatcher>, PeekableStringIterator) {
    debug!("Tokenizing query");
    let (tokens, iter) = tokenize_query(file, options);
    debug!("Tokenized query: {:#?}", tokens);
    debug!("Parsing query");
    (
        parse_query_ast(options, &mut tokens.into_iter().peekable(), false),
        iter,
    )
}
