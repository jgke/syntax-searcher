//! Main entry point for the program.

use log::debug;
use std::io::Read;
use std::path::Path;

use crate::options::*;
use crate::parser::*;
use crate::query::*;

#[cfg(not(tarpaulin_include))]
/// Parse `file` with `options` and print all matches.
pub fn run_cached<R: Read>(query: &Query, options: &Options, filename: &Path, file: R) -> bool {
    debug!("Parsing file");
    let (file, iter) = parse_file(file, options);
    debug!("Enumerating matches");
    let mut found_match = false;
    for m in query.matches(&file) {
        debug!("Match: {:#?}", &m);
        if m.t.is_empty() {
            continue;
        }
        found_match = true;
        let span = m.t[0].span().merge(&m.t.last().unwrap_or(&m.t[0]).span());
        let (start, end) = iter.get_line_information(span);
        let line_number = if start == end {
            format!("[{}:{}]", &filename.to_string_lossy(), start)
        } else {
            format! {"[{}:{}-{}]", &filename.to_string_lossy(), start, end}
        };
        if options.only_matching {
            println!("{}: {}", line_number, iter.get_content_between(span));
        } else {
            let lines = iter.get_lines_including(span);
            if lines.len() == 1 {
                println!("{} {}", line_number, lines[0]);
            } else {
                println!("{}", line_number);
                for line in lines {
                    println!("{}", line);
                }
            }
        }
    }
    debug!("Done");
    found_match
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::psi::Span;
    use crate::tokenizer::*;

    fn run_all<R: Read>(options: Options, file: R) -> Vec<Match> {
        let query = Query::new(&options);
        let (file, _iter) = parse_file(file, &options);
        query.matches(&file).collect()
    }

    fn run_strs(query: &str, file: &str) -> Vec<String> {
        let options = Options::new("js".as_ref(), &["syns", query, "-"]);
        let file = file.as_bytes();
        let query = Query::new(&options);
        let (file, iter) = parse_file(file, &options);
        query
            .matches(&file)
            .map(|m| {
                let span = m.t[0].span().merge(&m.t.last().unwrap_or(&m.t[0]).span());
                iter.get_content_between(span)
            })
            .collect()
    }

    #[test]
    fn test_empty() {
        let res = run_all(
            Options::new("js".as_ref(), &["syns", "bar", "-"]),
            "foo".as_bytes(),
        );
        assert_eq!(res.len(), 0);
    }

    #[test]
    fn test_one_match() {
        let res = run_all(
            Options::new("js".as_ref(), &["syns", "foo", "-"]),
            "foo".as_bytes(),
        );
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].t.len(), 1);
        assert!(matches!(
            res[0].t[0],
            Ast::Token(StandardToken {
                ty: StandardTokenType::Identifier(_),
                span: Span { lo: 0, hi: 2 }
            })
        ));
    }

    #[test]
    fn test_longest_match() {
        let res = run_all(
            Options::new("js".as_ref(), &["syns", "\\.\\*", "-"]),
            "a a".as_bytes(),
        );
        assert_eq!(res.len(), 2);
        assert_eq!(res[0].t.len(), 2);
        assert_eq!(res[1].t.len(), 1);
        assert!(matches!(
            res[0].t[0],
            Ast::Token(StandardToken {
                ty: StandardTokenType::Identifier(_),
                span: Span { lo: 0, hi: 0 }
            })
        ));
        assert!(matches!(
            res[0].t[1],
            Ast::Token(StandardToken {
                ty: StandardTokenType::Identifier(_),
                span: Span { lo: 2, hi: 2 }
            })
        ));
        assert!(matches!(
            res[1].t[0],
            Ast::Token(StandardToken {
                ty: StandardTokenType::Identifier(_),
                span: Span { lo: 2, hi: 2 }
            })
        ));
    }

    #[test]
    fn test_groups() {
        assert_eq!(run_strs("b \\(a a\\) b\\+", "b a a b"), vec!["b a a b"]);
        assert_eq!(
            run_strs("b \\(a a\\) b\\+", "b a a a b"),
            Vec::<String>::new()
        );
        assert_eq!(
            run_strs("b \\(a a\\)\\+ b\\+", "b a a a b"),
            Vec::<String>::new()
        );
        assert_eq!(
            run_strs("b \\(a a\\)\\+ b\\+", "b a a a a b"),
            vec!["b a a a a b"]
        );
    }

    #[test]
    fn test_delimited() {
        assert_eq!(run_strs("a () c", "a (b) c"), vec!["a (b) c"]);
        assert_eq!(run_strs("a (c) c", "a (b) c"), Vec::<String>::new());
        assert_eq!(
            run_strs("a (b (c)) c", "a (b (c d) e) c"),
            vec!["a (b (c d) e) c"]
        );
    }

    #[test]
    fn test_mismatched_parens_in_source() {
        assert_eq!(run_strs("a ()", "a ([b)]"), vec!["a ([b)]"]);
    }

    #[test]
    fn test_strict_paren_matching() {
        assert_eq!(run_strs("([a])", "([a])"), vec!["([a])"]);
        assert_eq!(run_strs("[(a)]", "([a])"), Vec::<String>::new());
    }
}
