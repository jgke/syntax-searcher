//! Main entry point for the program.

use log::debug;
use std::io::{Read, Write};
use std::path::Path;
use termcolor::{Color, ColorSpec, StandardStream, WriteColor};

use crate::options::*;
use crate::parser::*;
use crate::query::*;

macro_rules! write_colored {
    ($c:expr, $stdout:expr, $($arg:tt)*) => {let _ = {
        let _ = $stdout.set_color($c);
        write!($stdout, $($arg)*)
    };}
}
macro_rules! writeln_colored {
    ($c:expr, $stdout:expr, $($arg:tt)*) => {let _ = {
        let _ = $stdout.set_color($c);
        writeln!($stdout, $($arg)*)
    };}
}


#[cfg(not(tarpaulin_include))]
/// Parse `file` with `options` and print all matches.
pub fn run_cached<R: Read>(query: &Query, options: &Options, filename: &Path, file: R) -> bool {
    /* Colors from ripgrep's printer crate */
    #[cfg(unix)]
    let path_style: Color = Color::Magenta;
    #[cfg(windows)]
    let path_style: Color = Color::Cyan;
    let line_number_style: Color = Color::Green;
    let match_fg_color: Color = Color::Red;

    let reset_spec = ColorSpec::new();
    let mut path_spec = ColorSpec::new(); path_spec.set_fg(Some(path_style));
    let mut line_number_spec = ColorSpec::new(); line_number_spec.set_fg(Some(line_number_style));
    let mut match_spec = ColorSpec::new(); match_spec.set_fg(Some(match_fg_color)).set_bold(true);

    let mut stdout = StandardStream::stdout(options.color);
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
            write_colored!(&path_spec, stdout, "{}", line_number);
            writeln_colored!(&match_spec, stdout, " {}", iter.get_content_between(span));
        } else {
            let (head, lines, tail) = iter.get_lines_including(span);
            if lines.len() == 1 {
                write_colored!(&path_spec, stdout, "{}", line_number);
                write_colored!(&reset_spec, stdout, " {}", head);
                write_colored!(&match_spec, stdout, "{}", lines[0]);
                writeln_colored!(&reset_spec, stdout, "{}", tail);
            } else {
                writeln_colored!(&path_spec, stdout, "{}", line_number);
                write_colored!(&reset_spec, stdout, "{}", head);
                let mut lines_peekable = lines.into_iter().peekable();
                while let Some(line) = lines_peekable.next() {
                    let _ = stdout.set_color(&match_spec);
                    if lines_peekable.peek().is_some() {
                        writeln_colored!(&match_spec, stdout, "{}", line);
                    } else {
                        write_colored!(&match_spec, stdout, "{}", line);
                    }
                }
                writeln_colored!(&reset_spec, stdout, "{}", tail);
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

    #[test]
    fn test_or() {
        assert_eq!(
            run_strs("a c d \\| c \\(a a\\) b\\+", "a c c b b"),
            Vec::<String>::new()
        );
        assert_eq!(run_strs("a c \\| \\(a a\\) b\\+", "a a b"), vec!["a a b"]);
        assert_eq!(run_strs("a c \\| \\(a a\\) b\\+", "a c b"), vec!["a c"]);
    }
}
