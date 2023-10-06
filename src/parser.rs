use log::debug;
use regex::Regex;
use std::io::Read;
use std::iter::Peekable;

use crate::options::Options;
use crate::psi::{PeekableStringIterator, Span};
use crate::tokenizer::{tokenize, Token, TokenType};

#[derive(Clone, Debug)]
pub enum Ast {
    Token {
        token: Token,
    },
    Delimited {
        op: Token,
        cp: Option<Token>,
        content: Vec<Ast>,
    },
}

impl Ast {
    pub fn span(&self) -> Span {
        match self {
            Ast::Token { token } => token.span,
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
    iter: &mut Peekable<impl Iterator<Item = Token>>,
    recur: bool,
) -> Vec<Ast> {
    let mut res = Vec::new();
    loop {
        match iter.peek().map(|t| &t.ty) {
            Some(TokenType::Symbol(c)) if options.is_open_paren(&c) => {
                let op = iter.next().unwrap();
                let content = parse(options, iter, true);
                let cp = iter.next();
                res.push(Ast::Delimited { op, content, cp });
            }
            Some(TokenType::Symbol(c)) if recur && options.is_close_paren(&c) => {
                break;
            }
            Some(_token) => res.push(Ast::Token {
                token: iter.next().unwrap(),
            }),
            None => break,
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
pub enum MatcherAst {
    Token {
        token: Token,
    },
    Delimited {
        op: Token,
        cp: Option<Token>,
        content: Vec<MatcherAst>,
    },
    Any,
    Plus {
        matches: Box<MatcherAst>,
    },
    Star {
        matches: Box<MatcherAst>,
    },
    Regex(Regex),
}

fn parse_query_ast(
    options: &Options,
    iter: &mut Peekable<impl Iterator<Item = Token>>,
    recur: bool,
) -> Vec<MatcherAst> {
    let mut res = Vec::new();
    loop {
        match iter.peek().map(|t| &t.ty) {
            Some(TokenType::Symbol(c)) if options.is_open_paren(&c) => {
                let op = iter.next().unwrap();
                let content = parse_query_ast(options, iter, true);
                let cp = iter.next();
                res.push(MatcherAst::Delimited { op, content, cp });
            }
            Some(TokenType::Symbol(c)) if recur && options.is_close_paren(&c) => {
                break;
            }
            Some(TokenType::Any) => {
                assert_eq!(iter.next().map(|t| t.ty), Some(TokenType::Any));
                res.push(MatcherAst::Any);
            }
            Some(TokenType::Plus) => {
                assert_eq!(iter.next().map(|t| t.ty), Some(TokenType::Plus));
                let prev = res.pop().unwrap();
                res.push(MatcherAst::Plus {
                    matches: Box::new(prev),
                });
            }
            Some(TokenType::Star) => {
                assert_eq!(iter.next().map(|t| t.ty), Some(TokenType::Star));
                let prev = res.pop().unwrap();
                res.push(MatcherAst::Star {
                    matches: Box::new(prev),
                });
            }
            Some(TokenType::Regex(_)) => {
                if let Some(Token {
                    ty: TokenType::Regex(content),
                    ..
                }) = iter.next()
                {
                    res.push(MatcherAst::Regex(Regex::new(&content).unwrap()));
                } else {
                    unreachable!()
                }
            }
            Some(_token) => res.push(MatcherAst::Token {
                token: iter.next().unwrap(),
            }),
            None => break,
        }
    }
    res
}

pub fn parse_query<R: Read>(
    file: R,
    options: &Options,
) -> (Vec<MatcherAst>, PeekableStringIterator) {
    debug!("Tokenizing query");
    let (tokens, iter) = tokenize("query", file, options);
    debug!("Parsing query");
    (
        parse_query_ast(options, &mut tokens.into_iter().peekable(), false),
        iter,
    )
}
