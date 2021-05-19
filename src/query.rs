use crate::options::Options;

use crate::parser::{parse_query, Ast, MatcherAst};
use crate::tokenizer::TokenType;

#[derive(Debug)]
pub struct Query {
    matcher_ast: Vec<MatcherAst>,
}

#[derive(Debug)]
pub struct Match {
    pub t: Vec<Ast>,
}

impl Query {
    pub fn new(mut options: Options) -> Query {
        options.parse_as_query = true;
        let (tree, _) = parse_query(&mut options.query.as_bytes(), &options);
        Query { matcher_ast: tree }
    }

    fn ast_match<'a>(left: &'a [Ast], right: &'_ [MatcherAst]) -> Option<&'a [Ast]> {
        let mut left_pos = 0;
        let mut right_pos = 0;
        while right_pos < right.len() {
            match (left.get(left_pos), &right[right_pos]) {
                (None, MatcherAst::Any)
                | (None, MatcherAst::Token { .. })
                | (None, MatcherAst::Delimited { .. })
                | (None, MatcherAst::Plus { .. }) => return None,
                (Some(_), MatcherAst::Any) => {}
                (Some(Ast::Token { token: t1 }), MatcherAst::Regex(re)) => {
                    if let TokenType::StringLiteral(c) = &t1.ty {
                        if !re.is_match(&c) {
                            return None;
                        }
                    } else {
                        return None;
                    }
                }
                (_, MatcherAst::Regex(_)) => return None,
                (Some(Ast::Token { token: t1 }), MatcherAst::Token { token: t2 }) => {
                    if t1.ty != t2.ty {
                        return None;
                    }
                }
                (
                    Some(Ast::Delimited {
                        content: content1, ..
                    }),
                    MatcherAst::Delimited {
                        content: content2, ..
                    },
                ) => {
                    Query::ast_match(&content1, &content2)?;
                }
                (Some(Ast::Token { .. }), MatcherAst::Delimited { .. }) => return None,
                (Some(Ast::Delimited { .. }), MatcherAst::Token { .. }) => return None,
                (_, MatcherAst::Plus { .. }) => unimplemented!(),
                (None, MatcherAst::Star { .. }) => {}
                (Some(mut t), MatcherAst::Star { matches }) => {
                    while Query::ast_match(&[t.clone()], &[*matches.clone()]).is_some() {
                        left_pos += 1;
                        if let Some(tt) = left.get(left_pos) {
                            t = tt;
                        } else {
                            break;
                        }
                    }
                }
            }
            left_pos += 1;
            right_pos += 1;
        }
        Some(&left[0..right_pos])
    }

    fn potential_matches<'a>(input: &'a [Ast]) -> Box<dyn Iterator<Item = &'a [Ast]> + 'a> {
        Box::new(
            (0..input.len()).map(move |start| &input[start..]).chain(
                input
                    .iter()
                    .filter_map(|ast| {
                        if let Ast::Delimited { content, .. } = ast {
                            Some(content)
                        } else {
                            None
                        }
                    })
                    .flat_map(|c| Query::potential_matches(c)),
            ),
        )
    }

    pub fn matches<'a>(&'a self, input: &'a [Ast]) -> impl Iterator<Item = Match> + 'a {
        Query::potential_matches(input)
            .flat_map(move |tts| Query::ast_match(tts, &self.matcher_ast))
            .map(move |tts| Match { t: tts.to_vec() })
    }
}