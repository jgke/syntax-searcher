use crate::argparse::{parse_args, ArgRef};
use std::ffi::{OsStr, OsString};

#[derive(Clone, Debug)]
pub struct Options {
    pub filename: String,
    pub query: String,
    pub parse_as_query: bool,
    pub single_quote_strings: bool,
    pub double_quote_strings: bool,
    pub backtick_strings: bool,
    pub ranges: bool,

    pub only_matching: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            filename: "".to_string(),
            query: "".to_string(),
            parse_as_query: false,
            single_quote_strings: true,
            double_quote_strings: true,
            backtick_strings: true,
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
        "Usage: {} [OPTION]... PATTERN FILE [FILE]...
Search for PATTERN in each FILE.

Options:
    -h, --help              Display this message
    -s, --[no-]single       Enable or disable single quote strings
    -d, --[no-]double       Enable or disable double quote strings
    -b, --[no-]backtick     Enable or disable backtick strings
    -o, --only-matching     Print only the matched parts
",
        filename
    )
}

impl Options {
    pub fn new<S: AsRef<OsStr>>(args: &[S]) -> Options {
        let mut opts: Options = Default::default();
        let mut query: Option<OsString> = None;
        let mut files: Vec<OsString> = Vec::new();

        let parsed = parse_args(&args[1..]);
        let mut arg_iter = parsed.into_iter();

        #[allow(clippy::while_let_on_iterator)]
        while let Some(arg) = arg_iter.next() {
            match arg.as_ref() {
                ArgRef::Short("h") => {
                    print_help(false);
                    std::process::exit(0);
                }
                ArgRef::Long("help") => {
                    print_help(true);
                    std::process::exit(0);
                }

                ArgRef::Short("s") | ArgRef::Long("single") => opts.single_quote_strings = true,
                ArgRef::Long("no-single") => opts.single_quote_strings = false,

                ArgRef::Short("d") | ArgRef::Long("double") => opts.double_quote_strings = true,
                ArgRef::Long("no-double") => opts.double_quote_strings = false,

                ArgRef::Short("b") | ArgRef::Long("backtick") => opts.backtick_strings = true,
                ArgRef::Long("no-backtick") => opts.backtick_strings = false,

                ArgRef::Short("o") | ArgRef::Long("only-matching") => opts.only_matching = true,

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

    pub fn is_start_string(&self, c: char) -> bool {
        (self.double_quote_strings && c == '"')
            || (self.single_quote_strings && c == '\'')
            || (self.backtick_strings && c == '`')
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
        assert_eq!(options.single_quote_strings, true);
        assert_eq!(options.backtick_strings, true);
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

    #[test]
    fn options_quotes() {
        let mut options = Options::new(&vec!["syns", "query", "filename"]);
        assert!(options.is_start_string('\''));
        assert!(options.is_start_string('"'));
        assert!(options.is_start_string('`'));
        assert!(!options.is_start_string('.'));

        options.single_quote_strings = false;
        assert!(!options.is_start_string('\''));
        assert!(options.is_start_string('"'));
        assert!(options.is_start_string('`'));
        assert!(!options.is_start_string('.'));

        options.backtick_strings = false;
        assert!(!options.is_start_string('\''));
        assert!(options.is_start_string('"'));
        assert!(!options.is_start_string('`'));
        assert!(!options.is_start_string('.'));
    }
}
