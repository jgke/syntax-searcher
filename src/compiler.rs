use lazy_static::lazy_static;
use log::debug;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::parser::ParsedAstMatcher;
use crate::tokenizer::StandardTokenType;
use crate::wrappers::RegexEq;

#[derive(Clone, Debug, Hash, PartialEq)]
pub enum Matcher {
    Token(StandardTokenType),
    Delimited {
        op: StandardTokenType,
        cp: Option<StandardTokenType>,
        start: usize,
    },
    Any,
    Regex(RegexEq),
    Epsilon,
    Accept,
}

#[derive(Clone, Debug)]
pub struct State {
    pub id: usize,
    pub transitions: Vec<(Matcher, usize)>,
}

#[derive(Debug)]
pub struct Machine {
    pub initial: usize,
    pub states: HashMap<usize, State>,
}

impl State {
    fn new() -> State {
        let id = index();
        State {
            id,
            transitions: collection!(),
        }
    }
    fn add_transition(&mut self, to: usize, with: Matcher) {
        self.transitions.push((with, to))
    }
}

static INDEX: AtomicUsize = AtomicUsize::new(0);

lazy_static! {
    static ref ACCEPT: State = {
        let id = index();
        State {
            id,
            transitions: vec![(Matcher::Accept, id)],
        }
    };
}

fn index() -> usize {
    INDEX.fetch_add(1, Ordering::Relaxed)
}

impl Machine {
    fn new() -> Machine {
        Machine {
            initial: 0,
            states: collection!((ACCEPT.id, ACCEPT.clone())),
        }
    }

    fn add_transition(&mut self, from: usize, to: usize, with: Matcher) {
        self.states.get_mut(&from).unwrap().add_transition(to, with);
    }

    fn state(&mut self) -> &mut State {
        let state = State::new();
        let id = state.id;
        self.states.entry(id).or_insert(state)
    }

    fn link_list(&mut self, first: &ParsedAstMatcher, rest: &[ParsedAstMatcher]) -> (usize, usize) {
        let (start, mut end) = self.compile_state(first);
        for f in rest {
            let (new_start, new_end) = self.compile_state(f);
            self.add_transition(end, new_start, Matcher::Epsilon);
            end = new_end;
        }
        (start, end)
    }

    fn compile_state(&mut self, matcher: &ParsedAstMatcher) -> (usize, usize) {
        match matcher {
            ParsedAstMatcher::Token(token) => {
                let end = self.state().id;
                let start = self.state();
                start.add_transition(end, Matcher::Token(token.ty.clone()));
                (start.id, end)
            }
            ParsedAstMatcher::Plus(matcher) => {
                let (start, end) = self.compile_state(matcher);
                self.add_transition(end, start, Matcher::Epsilon);
                (start, end)
            }
            ParsedAstMatcher::Star(matcher) => {
                let (start, end) = self.compile_state(matcher);
                let new_end = self.state().id;
                self.add_transition(start, new_end, Matcher::Epsilon);
                self.add_transition(end, start, Matcher::Epsilon);
                self.add_transition(end, new_end, Matcher::Epsilon);
                (start, new_end)
            }
            ParsedAstMatcher::Any => {
                let end = self.state().id;
                let start = self.state();
                start.add_transition(end, Matcher::Any);
                (start.id, end)
            }
            ParsedAstMatcher::Regex(regex) => {
                let end = self.state().id;
                let start = self.state();
                start.add_transition(end, Matcher::Regex(RegexEq(regex.clone())));
                (start.id, end)
            }
            ParsedAstMatcher::Delimited { op, cp, content } => {
                let inner_start = {
                    if let Some((first, rest)) = content.split_first() {
                        let (start, end) = self.link_list(first, rest);
                        self.add_transition(end, ACCEPT.id, Matcher::Epsilon);
                        start
                    } else {
                        ACCEPT.id
                    }
                };
                let end = self.state().id;
                let delim = self.state();

                delim.add_transition(
                    end,
                    Matcher::Delimited {
                        op: op.ty.clone(),
                        cp: cp.as_ref().map(|t| t.ty.clone()),
                        start: inner_start,
                    },
                );
                (delim.id, end)
            }
            ParsedAstMatcher::Nested(content) => {
                if let Some((first, rest)) = content.split_first() {
                    self.link_list(first, rest)
                } else {
                    let state = self.state().id;
                    (state, state)
                }
            }
        }
    }

    fn parse_query_ast(&mut self, content: &[ParsedAstMatcher]) -> (usize, usize) {
        if let Some((first, rest)) = content.split_first() {
            self.link_list(first, rest)
        } else {
            let state = self.state().id;
            (state, state)
        }
    }
}

pub fn compile_query(query: Vec<ParsedAstMatcher>) -> Machine {
    debug!("Compiling query");
    let mut machine = Machine::new();
    let (start, end) = machine.parse_query_ast(&query);
    machine.initial = start;
    machine
        .states
        .get_mut(&end)
        .unwrap()
        .add_transition(ACCEPT.id, Matcher::Epsilon);
    machine
}
