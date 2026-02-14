//! Non-deterministic finite automaton compiler.

use lazy_static::lazy_static;
use log::debug;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::parser::ParsedAstMatcher;
use crate::tokenizer::StandardTokenType;
use crate::wrappers::RegexEq;

/// Token matchers.
#[derive(Clone, Debug, Hash, PartialEq)]
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

impl Matcher {
    /// Format the matcher as a dot edge label
    pub fn to_dot_condition(&self) -> String {
        (match self {
            Matcher::Token(t) => format!("token {:?}", t),
            Matcher::Delimited { op, cp, start } => format!("delim {:?} {:?} {}", op, cp, start),
            Matcher::Any => "*".to_string(),
            Matcher::End => "$".to_string(),
            Matcher::Regex(r) => format!("r\"{}\"", r.as_str()),
            Matcher::Epsilon => "e".to_string(),
            Matcher::Accept => "accept".to_string(),
        })
        .replace('"', "\\\"")
    }
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

    fn list_symbols(
        &self,
        start_from: usize,
        prefix: &str,
        used: &mut HashSet<usize>,
    ) -> (String, Vec<usize>) {
        let id = start_from;
        if id == ACCEPT.id || used.contains(&id) {
            return ("".to_string(), vec![]);
        };
        used.insert(id);
        let mut output = String::new();
        let mut out_ids = vec![];
        for (matcher, target_id) in &self.states[&id].transitions {
            match matcher {
                Matcher::Delimited { start, .. } => {
                    let new_prefix = format!("{}{}_", prefix, id);
                    output += &format!(
                        "  \"{}{}\" -> \"{}{}\" [label = \"{}\"];\n",
                        prefix,
                        id,
                        new_prefix,
                        start,
                        matcher.to_dot_condition()
                    );
                    let (edges, new_outs) = self.list_symbols(*start, &new_prefix, used);
                    output += &edges;
                    for out in new_outs {
                        output += &format!(
                            "  \"{}{}\" -> \"{}{}\" [label = \"{}\"];\n",
                            new_prefix,
                            out,
                            prefix,
                            target_id,
                            Matcher::Epsilon.to_dot_condition()
                        );
                    }

                    if id != ACCEPT.id {
                        let (edges, mut new_outs) = self.list_symbols(*target_id, prefix, used);
                        output += &edges;
                        out_ids.append(&mut new_outs);
                    }
                }
                _ => {
                    if *target_id == ACCEPT.id {
                        output += &format!(
                            "  \"{}{}\" -> \"{}\" [label = \"{}\"];\n",
                            prefix,
                            id,
                            target_id,
                            matcher.to_dot_condition()
                        );
                        out_ids.push(id);
                    } else {
                        output += &format!(
                            "  \"{}{}\" -> \"{}{}\" [label = \"{}\"];\n",
                            prefix,
                            id,
                            prefix,
                            target_id,
                            matcher.to_dot_condition()
                        );
                    }

                    if id != ACCEPT.id {
                        let (edges, mut new_outs) = self.list_symbols(*target_id, prefix, used);
                        output += &edges;
                        out_ids.append(&mut new_outs);
                    }
                }
            }
        }
        (output, out_ids)
    }

    /// Convert the state machine into a dot graph
    pub fn to_dot_graph(&self) -> String {
        let mut used = HashSet::new();
        let mut output = "digraph finite_state_machine {\n".to_string();
        output += "  rankdir=LR;\n";
        output += &format!("  node [shape = diamond]; {};\n", self.initial);
        output += &format!("  node [shape = doublecircle]; {};\n", ACCEPT.id);
        output += "  node [shape = circle];\n";
        output += &self.list_symbols(self.initial, "", &mut used).0;
        output += &self
            .states
            .keys()
            .map(|id| {
                if !used.contains(id) {
                    format!("{} ", id)
                } else {
                    "".to_string()
                }
            })
            .collect::<String>();
        output += "}\n";
        output
    }
}

/// Optimize the state machine by removing unnecessary states and edges.
pub fn optimize(machine: &mut Machine) {
    let ids = machine.states.keys().copied().collect::<Vec<usize>>();

    // convert  a[t] -> b[e] -> c to a[t] -> c
    for id in &ids {
        if *id == ACCEPT.id {
            continue;
        }
        if machine.states[id].transitions.len() != 1 {
            continue;
        }
        if let (Matcher::Epsilon, new_id) = machine.states[id].transitions[0] {
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

        for id in to_remove {
            machine.states.remove(&id);
        }
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
    optimize(&mut machine);
    machine
}
