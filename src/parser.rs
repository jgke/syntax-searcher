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
        if let Some(TokenType::Symbol(c)) = iter.peek().map(|t| &t.ty) {
            if recur && options.is_close_paren(&c) {
                break;
            }
        }
        if let Some(token) = iter.next() {
            match &token.ty {
                TokenType::Symbol(c) if options.is_open_paren(&c) => {
                    let content = parse(options, iter, true);
                    let cp = iter.next();
                    res.push(Ast::Delimited { op: token, content, cp });
                }
                _ => res.push(Ast::Token { token }),
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
        if let Some(TokenType::Symbol(c)) = iter.peek().map(|t| &t.ty) {
            if recur && options.is_close_paren(&c) {
                break;
            }
        }
        if let Some(token) = iter.next() {
            match &token.ty {
                TokenType::Symbol(c) if options.is_open_paren(&c) => {
                    let op = token;
                    let content = parse_query_ast(options, iter, true);
                    let cp = iter.next();
                    res.push(MatcherAst::Delimited { op, content, cp });
                }
                TokenType::Any => {
                    res.push(MatcherAst::Any);
                }
                TokenType::Plus => {
                    let prev = res.pop().unwrap_or(MatcherAst::Any);
                    res.push(MatcherAst::Plus {
                        matches: Box::new(prev),
                    });
                }
                TokenType::Star => {
                    let prev = res.pop().unwrap_or(MatcherAst::Any);
                    res.push(MatcherAst::Star {
                        matches: Box::new(prev),
                    });
                }
                TokenType::Regex(content) => {
                    match Regex::new(&content) {
                        Ok(r) => {
                            let matcher = MatcherAst::Regex(r);
                            iter.next();
                            res.push(matcher);
                        }
                        Err(e) => {
                            println!("{}", e);
                            std::process::exit(1);
                        }
                    }
                }
                _ => res.push(MatcherAst::Token {
                    token
                }),
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
) -> (Vec<MatcherAst>, PeekableStringIterator) {
    debug!("Tokenizing query");
    let (tokens, iter) = tokenize("query", file, options);
    debug!("Parsing query");
    (
        parse_query_ast(options, &mut tokens.into_iter().peekable(), false),
        iter,
    )
}
