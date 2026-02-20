//! Dot graph rendering for the NFA state machine.

use itertools::Itertools;
use std::collections::{HashMap, HashSet};

use crate::compiler::{Machine, Matcher};

fn to_dot_condition(matcher: &Matcher) -> String {
    (match matcher {
        Matcher::Token(t) => format!("token {:?}", t),
        Matcher::Delimited { op, .. } => format!("delim {:?}", op),
        Matcher::Any => "*".to_string(),
        Matcher::End => "$".to_string(),
        Matcher::Regex(r) => format!("r\"{}\"", r.as_str()),
        Matcher::Epsilon => "e".to_string(),
        Matcher::Accept => "accept".to_string(),
    })
    .replace('"', "\\\"")
}

/// A dot graph representation of a state machine.
pub struct DotGraph<'a> {
    machine: &'a Machine,
    initial: usize,
    accept_id: usize,
    state_ids: Vec<usize>,
    used: HashSet<usize>,
    edges: Vec<(String, String, String)>,
    accept_nodes: Vec<String>,
    clusters: HashMap<String, Vec<String>>,
}

impl<'a> DotGraph<'a> {
    /// Create a new dot graph for `machine`.
    pub fn new(machine: &'a Machine) -> Self {
        let accept_id = machine
            .states
            .iter()
            .find(|(_, s)| {
                s.transitions
                    .iter()
                    .any(|(m, _)| matches!(m, Matcher::Accept))
            })
            .map(|(&id, _)| id)
            .expect("No accept state found");

        let mut dg = DotGraph {
            machine,
            initial: machine.initial,
            accept_id,
            state_ids: machine.states.keys().copied().sorted().collect(),
            used: HashSet::new(),
            edges: vec![],
            accept_nodes: vec![],
            clusters: HashMap::new(),
        };

        dg.list_symbols(dg.initial, "");

        let mut seen_accept_nodes: HashSet<String> = HashSet::new();
        dg.accept_nodes
            .retain(|n| seen_accept_nodes.insert(n.clone()));

        let mut seen_edges: HashSet<(String, String, String)> = HashSet::new();
        dg.edges.retain(|e| seen_edges.insert(e.clone()));

        #[allow(clippy::iter_over_hash_type)]
        for nodes in dg.clusters.values_mut() {
            let mut seen_nodes: HashSet<String> = HashSet::new();
            nodes.retain(|n| seen_nodes.insert(n.clone()));
        }

        dg
    }

    fn list_symbols(&mut self, start_from: usize, prefix: &str) -> Vec<usize> {
        let id = start_from;
        if id == self.accept_id || self.used.contains(&id) {
            return vec![];
        };
        self.used.insert(id);
        if !prefix.is_empty() {
            self.clusters
                .entry(prefix.to_string())
                .or_default()
                .push(format!("{}{}", prefix, id));
        }
        let transitions = self.machine.states[&id].transitions.clone();
        let mut out_ids = vec![];
        for (matcher, target_id) in &transitions {
            match matcher {
                Matcher::Delimited { start, .. } => {
                    let new_prefix = format!("{}{}_", prefix, id);
                    self.edges.push((
                        format!("{}{}", prefix, id),
                        format!("{}{}", new_prefix, start),
                        to_dot_condition(matcher),
                    ));
                    let new_outs = self.list_symbols(*start, &new_prefix);
                    let has_inner_outs = !new_outs.is_empty();
                    for out in new_outs {
                        self.edges.push((
                            format!("{}{}", new_prefix, out),
                            format!("{}{}", prefix, target_id),
                            to_dot_condition(&Matcher::Epsilon),
                        ));
                    }
                    if has_inner_outs && *target_id == self.accept_id && !prefix.is_empty() {
                        let node = format!("{}{}", prefix, self.accept_id);
                        self.accept_nodes.push(node.clone());
                        self.clusters
                            .entry(prefix.to_string())
                            .or_default()
                            .push(node);
                        out_ids.push(self.accept_id);
                    }

                    if id != self.accept_id {
                        let mut new_outs = self.list_symbols(*target_id, prefix);
                        out_ids.append(&mut new_outs);
                    }
                }
                _ => {
                    if *target_id == self.accept_id {
                        if !prefix.is_empty() {
                            let node = format!("{}{}", prefix, self.accept_id);
                            self.accept_nodes.push(node.clone());
                            self.clusters
                                .entry(prefix.to_string())
                                .or_default()
                                .push(node);
                        }
                        self.edges.push((
                            format!("{}{}", prefix, id),
                            format!("{}{}", prefix, target_id),
                            to_dot_condition(matcher),
                        ));
                        out_ids.push(self.accept_id);
                    } else {
                        self.edges.push((
                            format!("{}{}", prefix, id),
                            format!("{}{}", prefix, target_id),
                            to_dot_condition(matcher),
                        ));
                    }

                    if id != self.accept_id {
                        let mut new_outs = self.list_symbols(*target_id, prefix);
                        out_ids.append(&mut new_outs);
                    }
                }
            }
        }
        out_ids
    }

    fn render_clusters(&self, parent_prefix: &str, depth: usize) -> String {
        let indent = "  ".repeat(depth + 1);
        let mut children: Vec<&String> = self
            .clusters
            .keys()
            .filter(|p| {
                p.starts_with(parent_prefix)
                    && p.as_str() != parent_prefix
                    && p[parent_prefix.len()..].matches('_').count() == 1
            })
            .collect();
        children.sort();
        let mut output = String::new();
        for prefix in children {
            output += &format!("{}subgraph cluster_{} {{\n", indent, prefix);
            for node in &self.clusters[prefix] {
                let label = node.rsplit_once('_').map(|(_, id)| id).unwrap_or(node);
                output += &format!("{}  \"{}\" [label = \"{}\"];\n", indent, node, label);
            }
            output += &self.render_clusters(prefix, depth + 1);
            output += &format!("{}}}\n", indent);
        }
        output
    }
}

impl std::fmt::Display for DotGraph<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut output = "digraph finite_state_machine {\n".to_string();
        output += "  rankdir=LR;\n";
        output += &format!("  node [shape = diamond]; {};\n", self.initial);
        output += &format!("  node [shape = doublecircle]; {};\n", self.accept_id);
        for node in &self.accept_nodes {
            output += &format!("  node [shape = doublecircle]; \"{}\";\n", node);
        }
        output += "  node [shape = circle];\n";
        for (from, to, label) in &self.edges {
            output += &format!("  \"{}\" -> \"{}\" [label = \"{}\"];\n", from, to, label);
        }
        output += &self.render_clusters("", 0);
        output += &self
            .state_ids
            .iter()
            .map(|id| {
                if !self.used.contains(id) {
                    format!("  {}\n", id)
                } else {
                    "".to_string()
                }
            })
            .collect::<String>();
        output += "}\n";
        write!(f, "{}", output)
    }
}

/// Convert the state machine into a dot graph.
pub fn to_dot_graph(machine: &Machine) -> String {
    DotGraph::new(machine).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::compile_query;
    use crate::options::Options;
    use crate::parser::parse_query;

    fn compile(query: &str) -> Machine {
        let options = Options::new("js".as_ref(), &["syns", query, "-"]);
        let (parsed, _) = parse_query(query.as_bytes(), &options);
        compile_query(parsed)
    }

    #[test]
    fn compile_nested_parens_dot_graph() {
        let dot = to_dot_graph(&compile(r"((a))"));
        assert_eq!(
            dot,
            r#"digraph finite_state_machine {
  rankdir=LR;
  node [shape = diamond]; 3;
  node [shape = doublecircle]; 0;
  node [shape = doublecircle]; "3_2_0";
  node [shape = doublecircle]; "3_0";
  node [shape = circle];
  "3" -> "3_2" [label = "delim Symbol(\"(\")"];
  "3_2" -> "3_2_1" [label = "delim Symbol(\"(\")"];
  "3_2_1" -> "3_2_0" [label = "token Identifier(\"a\")"];
  "3_2_0" -> "3_0" [label = "e"];
  "3_0" -> "0" [label = "e"];
  subgraph cluster_3_ {
    "3_2" [label = "2"];
    "3_0" [label = "0"];
    subgraph cluster_3_2_ {
      "3_2_1" [label = "1"];
      "3_2_0" [label = "0"];
    }
  }
  0
}
"#
        );
    }

    #[test]
    fn compile_star_any_in_parens_dot_graph() {
        let dot = to_dot_graph(&compile(r"(\.\* a)"));
        assert_eq!(
            dot,
            r#"digraph finite_state_machine {
  rankdir=LR;
  node [shape = diamond]; 2;
  node [shape = doublecircle]; 0;
  node [shape = doublecircle]; "2_0";
  node [shape = circle];
  "2" -> "2_1" [label = "delim Symbol(\"(\")"];
  "2_1" -> "2_1" [label = "*"];
  "2_1" -> "2_0" [label = "token Identifier(\"a\")"];
  "2_0" -> "0" [label = "e"];
  subgraph cluster_2_ {
    "2_1" [label = "1"];
    "2_0" [label = "0"];
  }
  0
}
"#
        );
    }

    #[test]
    fn compile_or_group_dot_graph() {
        let dot = to_dot_graph(&compile(r"a \| (b c)"));
        assert_eq!(
            dot,
            r#"digraph finite_state_machine {
  rankdir=LR;
  node [shape = diamond]; 1;
  node [shape = doublecircle]; 0;
  node [shape = doublecircle]; "1_0";
  node [shape = circle];
  "1" -> "0" [label = "token Identifier(\"a\")"];
  "1" -> "1_2" [label = "delim Symbol(\"(\")"];
  "1_2" -> "1_3" [label = "token Identifier(\"b\")"];
  "1_3" -> "1_0" [label = "token Identifier(\"c\")"];
  "1_0" -> "0" [label = "e"];
  subgraph cluster_1_ {
    "1_2" [label = "2"];
    "1_3" [label = "3"];
    "1_0" [label = "0"];
  }
  0
}
"#
        );
    }
}
