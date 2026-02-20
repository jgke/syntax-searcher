//! Non-deterministic finite automaton compiler.

use itertools::Itertools;
use lazy_static::lazy_static;
use log::debug;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::parser::ParsedAstMatcher;
use crate::tokenizer::StandardTokenType;
use crate::wrappers::RegexEq;

/// Token matchers.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Matcher {
    /// Match a simple token.
    Token(StandardTokenType),
    /// Match a paren-delimited block.
    Delimited {
        /// Opening paren of the block.
        op: StandardTokenType,
        /// Closing paren of the block, or None in case of EOF.
        cp: Option<StandardTokenType>,
        /// Starting state index of the nested NFA.
        start: usize,
    },
    /// Match any token.
    Any,
    /// Match end of group
    End,
    /// Match a string literal with a regex.
    Regex(RegexEq),
    /// Match anything without consuming the next token.
    Epsilon,
    /// Accept the input.
    Accept,
}

/// A single state in the state machine.
#[derive(Clone, Debug)]
pub struct State {
    /// ID of this state.
    pub id: usize,
    /// Transitions to next states.
    pub transitions: Vec<(Matcher, usize)>,
}

/// Non-deterministic finite automaton.
#[derive(Debug)]
pub struct Machine {
    /// Initial state of this machine.
    pub initial: usize,
    /// All of the states inside this machine.
    pub states: HashMap<usize, State>,
}

static INDEX: AtomicUsize = AtomicUsize::new(0);

fn index() -> usize {
    INDEX.fetch_add(1, Ordering::Relaxed)
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

lazy_static! {
    static ref ACCEPT: State = {
        let id = index();
        State {
            id,
            transitions: vec![(Matcher::Accept, id)],
        }
    };
}

impl Machine {
    fn new() -> Machine {
        Machine {
            initial: 0,
            states: collection!((ACCEPT.id, ACCEPT.clone())),
        }
    }

    fn add_transition(&mut self, from: usize, to: usize, with: Matcher) {
        self.states
            .get_mut(&from)
            .expect("Internal error when compiling query")
            .add_transition(to, with);
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
            ParsedAstMatcher::QuestionMark(matcher) => {
                let (start, end) = self.compile_state(matcher);
                let new_end = self.state().id;
                self.add_transition(start, new_end, Matcher::Epsilon);
                self.add_transition(end, new_end, Matcher::Epsilon);
                (start, new_end)
            }
            ParsedAstMatcher::Star(matcher) => {
                let (start, end) = self.compile_state(matcher);
                let new_end = self.state().id;
                self.add_transition(start, new_end, Matcher::Epsilon);
                self.add_transition(end, start, Matcher::Epsilon);
                self.add_transition(end, new_end, Matcher::Epsilon);
                (start, new_end)
            }
            ParsedAstMatcher::Or(a, b) => {
                let start = self.state().id;
                let (start_a, end_a) = self.compile_state(a);
                let (start_b, end_b) = self.compile_state(b);
                let new_end = self.state().id;
                self.add_transition(start, start_a, Matcher::Epsilon);
                self.add_transition(start, start_b, Matcher::Epsilon);
                self.add_transition(end_a, new_end, Matcher::Epsilon);
                self.add_transition(end_b, new_end, Matcher::Epsilon);
                (start, new_end)
            }
            ParsedAstMatcher::Any => {
                let end = self.state().id;
                let start = self.state();
                start.add_transition(end, Matcher::Any);
                (start.id, end)
            }
            ParsedAstMatcher::End => {
                let end = self.state().id;
                let start = self.state();
                start.add_transition(end, Matcher::End);
                (start.id, end)
            }
            ParsedAstMatcher::Regex(regex) => {
                let end = self.state().id;
                let start = self.state();
                start.add_transition(end, Matcher::Regex(regex.clone()));
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

/// Optimize the state machine by removing unnecessary states and edges.
pub fn optimize(machine: &mut Machine) {
    let ids = machine
        .states
        .keys()
        .copied()
        .sorted()
        .collect::<Vec<usize>>();

    // convert  a[t] -> b[e] -> c to a[t] -> c
    for id in &ids {
        if *id == ACCEPT.id {
            continue;
        }
        if machine.states[id].transitions.len() != 1 {
            continue;
        }
        if let (Matcher::Epsilon, new_id) = machine.states[id].transitions[0] {
            #[allow(clippy::iter_over_hash_type)]
            for state in &mut machine.states.values_mut() {
                for (_, old_id) in &mut state.transitions {
                    if old_id == id {
                        *old_id = new_id;
                    }
                }
            }
        }
    }

    // convert  a[e] -> b[T] -> c to a[t] -> c
    for id in &ids {
        if *id == ACCEPT.id {
            continue;
        }

        let mut new_transitions = Vec::new();
        let mut remove_ids = Vec::new();

        let a = &machine.states[id];
        for (matcher, new_id) in &a.transitions {
            if let Matcher::Epsilon = matcher {
                let b = &machine.states[new_id];
                remove_ids.push(false);
                new_transitions.extend(b.transitions.iter().cloned());
            } else {
                remove_ids.push(true);
            }
        }
        let mut iter = remove_ids.iter();
        machine
            .states
            .get_mut(id)
            .expect("internal error")
            .transitions
            .retain(|_| *iter.next().expect("internal error"));
        machine
            .states
            .get_mut(id)
            .expect("internal error")
            .transitions
            .append(&mut new_transitions);
    }

    // clean up unused states
    {
        let mut to_remove = ids.iter().copied().collect::<HashSet<_>>();
        let mut queue: Vec<usize> = Vec::new();

        queue.push(machine.initial);

        while let Some(id) = queue.pop() {
            if !to_remove.contains(&id) {
                continue;
            }
            to_remove.remove(&id);

            for (matcher, target_id) in &machine.states[&id].transitions {
                queue.push(*target_id);
                if let Matcher::Delimited { start, .. } = matcher {
                    queue.push(*start);
                }
            }
        }

        #[allow(clippy::iter_over_hash_type)]
        for id in to_remove {
            machine.states.remove(&id);
        }
    }

    // remove duplicate transitions
    #[allow(clippy::iter_over_hash_type)]
    for state in machine.states.values_mut() {
        let mut seen = HashSet::new();
        state.transitions.retain(|t| seen.insert(t.clone()));
    }

    // merge states with identical transition sets
    {
        let ids: Vec<usize> = machine.states.keys().copied().sorted().collect();
        let mut remap: HashMap<usize, usize> = HashMap::new();

        for (idx, &i) in ids.iter().enumerate() {
            if i == ACCEPT.id || remap.contains_key(&i) {
                continue;
            }
            for &j in &ids[idx+1..] {
                if j == ACCEPT.id || remap.contains_key(&j) {
                    continue;
                }
                let set_i = machine.states[&i].transitions.iter().collect::<HashSet<_>>();
                let set_j = machine.states[&j].transitions.iter().collect::<HashSet<_>>();
                if set_i == set_j {
                    remap.insert(j, i);
                }
            }
        }

        #[allow(clippy::iter_over_hash_type)]
        for state in machine.states.values_mut() {
            for (matcher, target) in &mut state.transitions {
                if let Some(&new_target) = remap.get(target) {
                    *target = new_target;
                }
                if let Matcher::Delimited { start, .. } = matcher {
                    if let Some(&new_start) = remap.get(start) {
                        *start = new_start;
                    }
                }
            }
        }
        if let Some(&new_initial) = remap.get(&machine.initial) {
            machine.initial = new_initial;
        }
    }
}

/// Normalize a machine by remapping state IDs to 0-based sequential integers in
/// ascending ID order, returning a new Machine with canonical IDs.
fn normalize(machine: &Machine) -> Machine {
    let mut ids: Vec<usize> = machine.states.keys().copied().collect();
    ids.sort();
    let id_map: HashMap<usize, usize> = ids
        .iter()
        .enumerate()
        .map(|(new, &old)| (old, new))
        .collect();

    let states = ids
        .iter()
        .map(|&old_id| {
            let new_id = id_map[&old_id];
            let transitions = machine.states[&old_id]
                .transitions
                .iter()
                .map(|(matcher, target)| {
                    let new_matcher = if let Matcher::Delimited { op, cp, start } = matcher {
                        Matcher::Delimited {
                            op: op.clone(),
                            cp: cp.clone(),
                            start: id_map[start],
                        }
                    } else {
                        matcher.clone()
                    };
                    (new_matcher, id_map[target])
                })
                .collect();
            (
                new_id,
                State {
                    id: new_id,
                    transitions,
                },
            )
        })
        .collect();

    Machine {
        initial: id_map[&machine.initial],
        states,
    }
}

/// Compile a parsed query into a NFA.
pub fn compile_query(query: Vec<ParsedAstMatcher>) -> Machine {
    debug!("Compiling query");
    let mut machine = Machine::new();
    let (start, end) = machine.parse_query_ast(&query);
    machine.initial = start;
    machine
        .states
        .get_mut(&end)
        .expect("Internal error when compiling query")
        .add_transition(ACCEPT.id, Matcher::Epsilon);

    // TODO: this is a bit dumb
    for _ in 0..5 {
        optimize(&mut machine);
    }

    normalize(&machine)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::Options;
    use crate::parser::parse_query;

    fn compile(query: &str) -> Machine {
        let options = Options::new("js".as_ref(), &["syns", query, "-"]);
        let (parsed, _) = parse_query(query.as_bytes(), &options);
        compile_query(parsed)
    }

    #[test]
    fn compile_star_any() {
        let machine = compile(r"\.\* a b");
        let ident = |s: &str| Matcher::Token(StandardTokenType::Identifier(s.to_string()));
        let mut states: Vec<(usize, Vec<(Matcher, usize)>)> = machine
            .states
            .iter()
            .map(|(&id, s)| (id, s.transitions.clone()))
            .collect();
        states.sort_by_key(|(id, _)| *id);
        assert_eq!(
            states,
            vec![
                (0, vec![(Matcher::Accept, 0)]),
                (1, vec![(Matcher::Any, 1), (ident("a"), 2)]),
                (2, vec![(ident("b"), 0)]),
            ]
        );
    }

    #[test]
    fn compile_or_group() {
        let machine = compile(r"a \| (b c)");
        let ident = |s: &str| Matcher::Token(StandardTokenType::Identifier(s.to_string()));
        let sym = |s: &str| StandardTokenType::Symbol(s.to_string());
        let mut states: Vec<(usize, Vec<(Matcher, usize)>)> = machine
            .states
            .iter()
            .map(|(&id, s)| (id, s.transitions.clone()))
            .collect();
        states.sort_by_key(|(id, _)| *id);
        assert_eq!(
            states,
            vec![
                (0, vec![(Matcher::Accept, 0)]),
                (
                    1,
                    vec![
                        (ident("a"), 0),
                        (
                            Matcher::Delimited {
                                op: sym("("),
                                cp: Some(sym(")")),
                                start: 2,
                            },
                            0,
                        ),
                    ],
                ),
                (2, vec![(ident("b"), 3)]),
                (3, vec![(ident("c"), 0)]),
            ]
        );
    }
}
