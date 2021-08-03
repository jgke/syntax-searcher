#![warn(missing_docs)]

use log::debug;
use std::io::Read;
use std::path::Path;

use crate::options::*;
use crate::parser::*;
use crate::query::*;

#[cfg(not(tarpaulin_include))]
/// Parse `file` with `options` and print all matches.
pub fn run<R: Read>(options: &Options, filename: &Path, file: R) {
    debug!("Parsing query");
    let query = Query::new(options.clone());
    debug!("Query: {:#?}", query);
    debug!("Parsing file");
    let (file, iter) = parse_file(file, &options);
    debug!("Enumerating matches");
    for m in query.matches(&file) {
        debug!("Match: {:#?}", &m);
        if m.t.is_empty() {
            continue;
        }
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::psi::Span;
    use crate::tokenizer::*;

    fn run_all<R: Read>(options: Options, file: R) -> Vec<Match> {
        let query = Query::new(options.clone());
        let (file, _iter) = parse_file(file, &options);
        // TODO: error reporting
        query.matches(&file).collect()
    }

    #[test]
    fn test_empty() {
        let res = run_all(Options::new(&["syns", "bar", "-"]), "foo".as_bytes());
        assert_eq!(res.len(), 0);
    }

    #[test]
    fn test_one_match() {
        let res = run_all(Options::new(&["syns", "foo", "-"]), "foo".as_bytes());
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].t.len(), 1);
        assert!(matches!(
            res[0].t[0],
            Ast::Token {
                token: Token {
                    ty: TokenType::Identifier(_),
                    span: Span { lo: 0, hi: 2 }
                }
            }
        ));
    }

    #[test]
    fn test_longest_match() {
        let res = run_all(Options::new(&["syns", "\\.\\*", "-"]), "a a".as_bytes());
        dbg!(&res);
        assert_eq!(res.len(), 2);
        assert_eq!(res[0].t.len(), 2);
        assert_eq!(res[1].t.len(), 1);
        assert!(matches!(
            res[0].t[0],
            Ast::Token {
                token: Token {
                    ty: TokenType::Identifier(_),
                    span: Span { lo: 0, hi: 0 }
                }
            }
        ));
        assert!(matches!(
            res[0].t[1],
            Ast::Token {
                token: Token {
                    ty: TokenType::Identifier(_),
                    span: Span { lo: 2, hi: 2 }
                }
            }
        ));
        assert!(matches!(
            res[1].t[0],
            Ast::Token {
                token: Token {
                    ty: TokenType::Identifier(_),
                    span: Span { lo: 2, hi: 2 }
                }
            }
        ));
    }
}
