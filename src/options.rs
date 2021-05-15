use crate::argparse::{parse_args, Arg, ArgRef};
use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::iter::Peekable;

#[derive(Clone, Debug)]
pub struct Options {
    pub filename: String,
    pub query: String,
    pub parse_as_query: bool,
    pub string_characters: HashSet<String>,
    pub single_line_comments: HashSet<String>,
    pub multi_line_comments: HashSet<(String, String)>,
    pub ranges: bool,

    pub only_matching: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            filename: "".to_string(),
            query: "".to_string(),
            parse_as_query: false,
            string_characters: ["\"", "'", "`"].iter().map(|s| s.to_string()).collect(),
            single_line_comments: ["//"].iter().map(|s| s.to_string()).collect(),
            multi_line_comments: [("/*", "*/")]
                .iter()
                .map(|(from, to)| (from.to_string(), to.to_string()))
                .collect(),
            ranges: true,

            only_matching: false,
        }
    }
}

fn print_help(long: bool) {
    let filename = std::env::args()
        .next()
        .unwrap_or_else(|| "syns".to_string());
    if !long {
        println!(
            "Usage: {} [OPTION]... PATTERN FILE [FILE]...
Pass --help for more information.",
            filename
        );
        return;
    }

    println!(
        r#"Usage: {} [OPTION]... PATTERN FILE [FILE]...
Search for PATTERN in each FILE.

Options:
  -h, --help                Display this message

  -s, --[no-]string CHAR    Add or remove CHAR from string delimiters
  -c, --[no-]comment CHARS  Add or remove CHARS from single-line comments
  -m, --[no-]multi BEGIN END Add or remove (BEGIN, END) from multi-line comments

  --[no-]single             Enable or disable single quote strings
                            Equivalent to -s "'"
  --[no-]double             Enable or disable double quote strings
                            Equivalent to -s '"'
  --[no-]backtick           Enable or disable backtick strings
                            Equivalent to -s '`'

  -o, --only-matching     Print only the matched parts
"#,
        filename
    )
}

fn get_whole_arg<I: Iterator<Item = Arg>>(iter: &mut Peekable<I>) -> Option<OsString> {
    let arg = iter.next()?;
    let index = arg.index();
    while iter.peek().map(|a| a.index()) == Some(index) {
        iter.next();
    }
    Some(arg.entire_match())
}

impl Options {
    pub fn new<S: AsRef<OsStr>>(args: &[S]) -> Options {
        let mut opts: Options = Default::default();
        let mut query: Option<OsString> = None;
        let mut files: Vec<OsString> = Vec::new();

        let parsed = parse_args(&args[1..]);
        let mut arg_iter = parsed.into_iter().peekable();

        #[allow(clippy::while_let_on_iterator)]
        while let Some(arg) = arg_iter.next() {
            match arg.as_ref() {
                ArgRef::Short('h') => {
                    print_help(false);
                    std::process::exit(0);
                }
                ArgRef::Long("help") => {
                    print_help(true);
                    std::process::exit(0);
                }

                ArgRef::Short('s') | ArgRef::Long("string") => {
                    if let Some(arg) = get_whole_arg(&mut arg_iter) {
                        opts.string_characters.insert(arg.to_string_lossy().to_string());
                    }
                }
                ArgRef::Long("no-string") => {
                    if let Some(arg) = get_whole_arg(&mut arg_iter) {
                        opts.string_characters.remove(&arg.to_string_lossy().to_string());
                    }
                }

                ArgRef::Short('c') | ArgRef::Long("comment") => {
                    if let Some(arg) = get_whole_arg(&mut arg_iter) {
                        opts.single_line_comments.insert(arg.to_string_lossy().to_string());
                    }
                }
                ArgRef::Long("no-comment") => {
                    if let Some(arg) = get_whole_arg(&mut arg_iter) {
                        opts.single_line_comments.remove(&arg.to_string_lossy().to_string());
                    }
                }

                ArgRef::Short('m') | ArgRef::Long("multi") => {
                    if let Some(start) = get_whole_arg(&mut arg_iter) {
                        if let Some(end) = get_whole_arg(&mut arg_iter) {
                            opts.multi_line_comments.insert((start.to_string_lossy().to_string(), end.to_string_lossy().to_string()));
                        }
                    }
                }
                ArgRef::Long("no-multi") => {
                    if let Some(start) = get_whole_arg(&mut arg_iter) {
                        if let Some(end) = get_whole_arg(&mut arg_iter) {
                            opts.multi_line_comments.remove(&(start.to_string_lossy().to_string(), end.to_string_lossy().to_string()));
                        }
                    }
                }


                ArgRef::Long("single") => {
                    opts.string_characters.insert("'".to_string());
                }
                ArgRef::Long("no-single") => {
                    opts.string_characters.remove("'");
                }

                ArgRef::Long("double") => {
                    opts.string_characters.insert("\"".to_string());
                }
                ArgRef::Long("no-double") => {
                    opts.string_characters.remove("\"");
                }

                ArgRef::Long("backtick") => {
                    opts.string_characters.insert("`".to_string());
                }
                ArgRef::Long("no-backtick") => {
                    opts.string_characters.remove("`");
                }

                ArgRef::Short('o') | ArgRef::Long("only-matching") => opts.only_matching = true,

                ArgRef::Positional(_) => {
                    if query.is_none() {
                        query = Some(arg.into());
                    } else {
                        files.push(arg.into());
                    }
                }

                ArgRef::Short(s) => {
                    println!("Unknown flag: -{}", s);
                    print_help(false);
                    std::process::exit(1);
                }
                ArgRef::Long(s) => {
                    println!("Unknown flag: --{}", s);
                    print_help(false);
                    std::process::exit(1);
                }
            }
        }

        if query.is_none() {
            println!("Missing required argument: PATTERN\n");
            print_help(false);
            std::process::exit(1);
        } else if files.is_empty() {
            println!("Missing required argument: FILE\n");
            print_help(false);
            std::process::exit(1);
        }

        opts.query = query.unwrap().to_string_lossy().to_string();
        opts.filename = files[0].to_string_lossy().to_string();

        opts
    }

    pub fn is_open_paren(&self, c: &str) -> bool {
        c == "(" || c == "[" || c == "{"
    }

    pub fn is_close_paren(&self, c: &str) -> bool {
        c == ")" || c == "]" || c == "}"
    }
}

#[cfg(test)]
mod tests {
    use super::Options;

    #[test]
    fn parse_options() {
        let options = Options::new(&vec!["syns", "query", "filename"]);
        assert_eq!(options.query, "query");
        assert_eq!(options.filename, "filename");
        assert_eq!(options.parse_as_query, false);
    }

    #[test]
    fn options_parens() {
        let options = Options::new(&vec!["syns", "query", "filename"]);
        assert!(options.is_open_paren("{"));
        assert!(options.is_open_paren("("));
        assert!(options.is_open_paren("["));
        assert!(!options.is_open_paren("}"));
        assert!(!options.is_open_paren(")"));
        assert!(!options.is_open_paren("]"));

        assert!(!options.is_close_paren("{"));
        assert!(!options.is_close_paren("("));
        assert!(!options.is_close_paren("["));
        assert!(options.is_close_paren("}"));
        assert!(options.is_close_paren(")"));
        assert!(options.is_close_paren("]"));
    }
}
