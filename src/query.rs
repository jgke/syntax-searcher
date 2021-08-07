use std::collections::HashSet;

use log::debug;

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
        debug!("Query string: {}", options.query);
        let (tree, _) = parse_query(&mut options.query.as_bytes(), &options);
        debug!("Query AST: {:#?}", tree);
        Query { matcher_ast: tree }
    }

    fn ast_match<'a>(left: &'a [Ast], right: &'_ [MatcherAst]) -> Option<&'a [Ast]> {
        let mut states = HashSet::new();
        states.insert((0, 0));
        let mut longest_match: Option<&'a [Ast]> = None;
        while !states.is_empty() {
            let mut next_states = HashSet::new();
            for (mut left_pos, state) in states {
                if right.get(state).is_none() {
                    longest_match = if left_pos > 0
                        && (longest_match.is_none()
                            || longest_match.map(|p| p.len()) < Some(left_pos))
                    {
                        Some(&left[0..left_pos.min(left.len())])
                    } else {
                        longest_match
                    };
                    continue;
                }
                match (left.get(left_pos), &right[state]) {
                    (None, MatcherAst::Any)
                    | (None, MatcherAst::Token { .. })
                    | (None, MatcherAst::Delimited { .. })
                    | (None, MatcherAst::Plus { .. }) => {}
                    (Some(_), MatcherAst::Any) => {
                        next_states.insert((left_pos + 1, state + 1));
                    }
                    (Some(Ast::Token { token: t1 }), MatcherAst::Regex(re)) => {
                        if let TokenType::StringLiteral(c) = &t1.ty {
                            if re.is_match(&c) {
                                next_states.insert((left_pos + 1, state + 1));
                            }
                        }
                    }
                    (_, MatcherAst::Regex(_)) => return None,
                    (Some(Ast::Token { token: t1 }), MatcherAst::Token { token: t2 }) => {
                        if t1.ty == t2.ty {
                            next_states.insert((left_pos + 1, state + 1));
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
                        if Query::ast_match(&content1, &content2).is_some() {
                            next_states.insert((left_pos + 1, state + 1));
                        }
                    }
                    (Some(Ast::Token { .. }), MatcherAst::Delimited { .. }) => {}
                    (Some(Ast::Delimited { .. }), MatcherAst::Token { .. }) => {}
                    (_, MatcherAst::Plus { matches }) => {
                        while let Some(t) = left.get(left_pos) {
                            if Query::ast_match(&[t.clone()], &[*matches.clone()]).is_some() {
                                next_states.insert((left_pos + 1, state));
                                next_states.insert((left_pos + 1, state + 1));
                                left_pos += 1;
                            } else {
                                break;
                            }
                        }
                    }
                    (_, MatcherAst::Star { matches }) => {
                        while let Some(t) = left.get(left_pos) {
                            if Query::ast_match(&[t.clone()], &[*matches.clone()]).is_some() {
                                next_states.insert((left_pos + 1, state));
                                next_states.insert((left_pos + 1, state + 1));
                                left_pos += 1;
                            } else {
                                next_states.insert((left_pos, state + 1));
                                break;
                            }
                        }
                        next_states.insert((left_pos, state + 1));
                    }
                }
            }
            states = next_states;
        }
        longest_match
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
            .map(move |tts| Match { t: tts.1.to_vec() })
    }
}
