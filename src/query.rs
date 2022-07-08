//! Query handling and matching.

use std::collections::HashSet;

use log::debug;

use crate::compiler::{compile_query, Machine, Matcher};
use crate::options::Options;
use crate::parser::{parse_query, Ast};
use crate::tokenizer::StandardTokenType;

/// Compiled query.
#[derive(Debug)]
pub struct Query {
    machine: Machine,
}

/// Successful match.
#[derive(Debug)]
pub struct Match {
    /// Matched tokens.
    pub t: Vec<Ast>,
}

impl Query {
    /// Compile a query.
    pub fn new(options: &Options) -> Query {
        debug!("Query string: {}", options.query);
        let (query, _) = parse_query(&mut options.query.as_bytes(), options);
        let machine = compile_query(query);
        debug!("Query AST: {:#?}", machine);
        Query { machine }
    }

    fn ast_match<'a>(&self, left: &'a [Ast], initials: &[usize]) -> Option<&'a [Ast]> {
        let mut current_states = initials
            .iter()
            .map(|state| (0, *state))
            .collect::<HashSet<_>>();
        let mut longest_match: Option<&'a [Ast]> = None;
        while !current_states.is_empty() {
            let mut next_states = HashSet::new();
            for (left_pos, state) in current_states {
                for (matcher, next_state) in &self.machine.states[&state].transitions {
                    match (left.get(left_pos), matcher) {
                        (_, Matcher::Accept) => {
                            longest_match = if longest_match.is_none()
                                || longest_match.map(|p| p.len()) < Some(left_pos)
                            {
                                Some(&left[0..left_pos.min(left.len())])
                            } else {
                                longest_match
                            };
                            continue;
                        }
                        (None, Matcher::Any)
                        | (None, Matcher::Token(..))
                        | (None, Matcher::Delimited { .. }) => {}
                        (Some(_), Matcher::Any) => {
                            next_states.insert((left_pos + 1, *next_state));
                        }
                        (Some(_), Matcher::End) => {}
                        (None, Matcher::End) => {
                            next_states.insert((left_pos + 1, *next_state));
                        }
                        (_, Matcher::Epsilon) => {
                            next_states.insert((left_pos, *next_state));
                        }
                        (Some(Ast::Token(t1)), Matcher::Regex(re)) => {
                            if let StandardTokenType::StringLiteral(c) = &t1.ty {
                                if re.is_match(c) {
                                    next_states.insert((left_pos + 1, *next_state));
                                }
                            }
                        }
                        (_, Matcher::Regex(_)) => {}
                        (Some(Ast::Token(t1)), Matcher::Token(t2)) => {
                            if &t1.ty == t2 {
                                next_states.insert((left_pos + 1, *next_state));
                            }
                        }
                        (
                            Some(Ast::Delimited {
                                content: content1,
                                op,
                                ..
                            }),
                            Matcher::Delimited { start, op: op1, .. },
                        ) => {
                            if &op.ty == op1 && self.ast_match(content1, &[*start]).is_some() {
                                next_states.insert((left_pos + 1, *next_state));
                            }
                        }
                        (Some(Ast::Token { .. }), Matcher::Delimited { .. }) => {}
                        (Some(Ast::Delimited { .. }), Matcher::Token { .. }) => {}
                    }
                }
            }
            current_states = next_states;
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

    /// Get all matches for this query from input.
    pub fn matches<'a>(&'a self, input: &'a [Ast]) -> impl Iterator<Item = Match> + 'a {
        Query::potential_matches(input)
            .flat_map(move |tts| self.ast_match(tts, &[self.machine.initial]))
            .map(move |tts| Match { t: tts.to_vec() })
    }
}
