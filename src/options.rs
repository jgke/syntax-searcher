//! Options parsing and handling.

use crate::argparse::{parse_args, Arg, ArgRef};
use lazy_static::lazy_static;
use log::warn;
use serde::Deserialize;
use std::collections::HashMap;
use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::iter::Peekable;

/// Parsed options.
#[derive(Clone, Debug)]
pub struct Options {
    /// File search paths.
    pub paths: Vec<OsString>,
    /// Query string.
    pub query: String,

    /// Set of strings which start or end a string literal (eg. "'").
    pub string_characters: HashSet<String>,
    /// Set of strings which start or end a single-line comment (eg. "//").
    pub single_line_comments: HashSet<String>,
    /// Set of strings which start and end a multi-line comment (eg. ("/*", "*/")).
    pub multi_line_comments: HashSet<(String, String)>,
    /// Parse '..' as a range.
    pub ranges: bool,

    /// Print only matching parts of the source code.
    pub only_matching: bool,
}

#[derive(Clone, Debug)]
enum OptionCommand {
    AddStringCharacter(String),
    RemoveStringCharacter(String),
    AddSingleComment(String),
    RemoveSingleComment(String),
    AddMultiComment(String, String),
    RemoveMultiComment(String, String),
    Language(String),
    OnlyMatching,
    PrintOptionsAndQuit,
}

#[derive(Clone, Debug, Deserialize)]
struct Defaults {
    extensions: Vec<String>,
    strings: Vec<String>,
    single_comments: Vec<String>,
    multi_comments: Vec<(String, String)>,
}

const BUILTIN_DATABASE: &str = include_str!("../config.json");

lazy_static! {
    static ref PARSED_DB: HashMap<String, Defaults> = serde_json::from_str(BUILTIN_DATABASE)
        .unwrap_or_else(|e| {
            warn!("Built-in JSON database has a syntax error: {}", e);
            HashMap::new()
        });
    static ref EXTENSION_TO_SETTINGS: HashMap<String, Options> = {
        let mut res = HashMap::new();

        for ty in PARSED_DB.values() {
            let opts = Options {
                string_characters: ty.strings.iter().cloned().collect(),
                single_line_comments: ty.single_comments.iter().cloned().collect(),
                multi_line_comments: ty.multi_comments.iter().cloned().collect(),
                ..Options::default()
            };

            for ext in &ty.extensions {
                res.insert(ext.to_string(), opts.clone());
            }
        }

        res
    };
}

impl Default for Options {
    fn default() -> Options {
        Options {
            paths: Vec::new(),
            query: "".to_string(),
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

fn print_help(long: bool, status: i32) -> ! {
    let filename = std::env::args()
        .next()
        .unwrap_or_else(|| "syns".to_string());
    if !long {
        println!(
            "Usage: {} [OPTION]... PATTERN PATH...
Pass --help for more information.",
            filename
        );
    } else {
        println!(
            r#"Usage: {} [OPTION]... PATTERN PATH...
Search for PATTERN in PATHs.

Options:
  -h, --help                 Display this message
  --lang LANGUAGE            Force defaults for LANGUAGE. Call 'syns --lang'
                             to display available languages.

  -s, --[no-]string CHARS    Add or remove CHARS from string delimiters
  -c, --[no-]comment CHARS   Add or remove CHARS from single-line comments
  -m, --[no-]multi BEGIN END Add or remove (BEGIN, END) from multi-line comments

  -o, --only-matching        Print only the matched parts
  --options                  Print what options would have been used to parse FILE
"#,
            filename
        );
    }
    std::process::exit(status)
}

fn print_langs() -> ! {
    println!("Available languages:");
    for (lang, defs) in PARSED_DB.iter() {
        println!("- {} [{}]", lang, defs.extensions.join(", "));
    }
    std::process::exit(0)
}

fn print_options(options: Options) -> ! {
    println!(
        r#"Using following parsing options:
- String delimiters: {}
- Single line comments: {}
- Multi line comments: {}"#,
        options
            .string_characters
            .into_iter()
            .collect::<Vec<_>>()
            .join(", "),
        options
            .single_line_comments
            .into_iter()
            .collect::<Vec<_>>()
            .join(", "),
        options
            .multi_line_comments
            .iter()
            .map(|(start, end)| format!("{} {}", start, end))
            .collect::<Vec<_>>()
            .join(", ")
    );

    std::process::exit(0);
}

fn get_whole_arg<I: Iterator<Item = Arg>>(iter: &mut Peekable<I>) -> Option<OsString> {
    let arg = iter.next()?;
    let index = arg.index();
    while iter.peek().map(|a| a.index()) == Some(index) {
        iter.next();
    }
    Some(arg.entire_match())
}

fn parse_options<S: AsRef<OsStr>>(args: &[S]) -> (Vec<OptionCommand>, Vec<OsString>) {
    let mut opts = Vec::new();
    let mut positionals = Vec::new();
    let parsed = parse_args(&args[1..]);
    let mut arg_iter = parsed.into_iter().peekable();

    while let Some(arg) = arg_iter.next() {
        let cmd = match arg.as_ref() {
            ArgRef::Short('h') => print_help(false, 0),
            ArgRef::Long("help") => print_help(true, 0),
            ArgRef::Long("lang") => {
                if let Some(arg) = get_whole_arg(&mut arg_iter) {
                    OptionCommand::Language(arg.to_string_lossy().to_string())
                } else {
                    print_langs()
                }
            }

            ArgRef::Short('s') | ArgRef::Long("string") => {
                if let Some(arg) = get_whole_arg(&mut arg_iter) {
                    OptionCommand::AddStringCharacter(arg.to_string_lossy().to_string())
                } else {
                    println!("Missing argument for --string");
                    print_help(false, 1)
                }
            }
            ArgRef::Long("no-string") => {
                if let Some(arg) = get_whole_arg(&mut arg_iter) {
                    OptionCommand::RemoveStringCharacter(arg.to_string_lossy().to_string())
                } else {
                    println!("Missing argument for --no-string");
                    print_help(false, 1)
                }
            }

            ArgRef::Short('c') | ArgRef::Long("comment") => {
                if let Some(arg) = get_whole_arg(&mut arg_iter) {
                    OptionCommand::AddSingleComment(arg.to_string_lossy().to_string())
                } else {
                    println!("Missing argument for --comment");
                    print_help(false, 1)
                }
            }
            ArgRef::Long("no-comment") => {
                if let Some(arg) = get_whole_arg(&mut arg_iter) {
                    OptionCommand::RemoveSingleComment(arg.to_string_lossy().to_string())
                } else {
                    println!("Missing argument for --no-comment");
                    print_help(false, 1)
                }
            }

            ArgRef::Short('m') | ArgRef::Long("multi") => {
                if let Some(start) = get_whole_arg(&mut arg_iter) {
                    if let Some(end) = get_whole_arg(&mut arg_iter) {
                        OptionCommand::AddMultiComment(
                            start.to_string_lossy().to_string(),
                            end.to_string_lossy().to_string(),
                        )
                    } else {
                        println!("Missing second argument for --multi");
                        print_help(false, 1)
                    }
                } else {
                    println!("Missing argument for --multi");
                    print_help(false, 1)
                }
            }
            ArgRef::Long("no-multi") => {
                if let Some(start) = get_whole_arg(&mut arg_iter) {
                    if let Some(end) = get_whole_arg(&mut arg_iter) {
                        OptionCommand::RemoveMultiComment(
                            start.to_string_lossy().to_string(),
                            end.to_string_lossy().to_string(),
                        )
                    } else {
                        println!("Missing second argument for --no-multi");
                        print_help(false, 1)
                    }
                } else {
                    println!("Missing argument for --no-multi");
                    print_help(false, 1)
                }
            }

            ArgRef::Short('o') | ArgRef::Long("only-matching") => OptionCommand::OnlyMatching,

            ArgRef::Long("options") => OptionCommand::PrintOptionsAndQuit,

            ArgRef::Positional => {
                positionals.push(arg.entire_match());
                continue;
            }

            ArgRef::Short(s) => {
                println!("Unknown flag: -{}", s);
                print_help(false, 1)
            }
            ArgRef::Long(s) => {
                println!("Unknown flag: --{}", s);
                print_help(false, 1)
            }
        };
        opts.push(cmd);
    }

    (opts, positionals)
}

impl Options {
    /// Parse options from `args`, using defaults for file type `extension`.
    ///
    /// ```
    /// use syns::options::Options;
    /// let options = Options::new("js".as_ref(), &vec!["syns", "query", "filename"]);
    /// assert_eq!(options.query, "query");
    /// assert_eq!(options.paths, vec!["filename"]);
    /// assert_eq!(options.only_matching, false);
    /// ```
    pub fn new<S: AsRef<OsStr>>(extension: &OsStr, args: &[S]) -> Options {
        let (cmds, positionals) = parse_options(args);
        let print_and_quit = cmds
            .iter()
            .any(|c| matches!(c, OptionCommand::PrintOptionsAndQuit));
        let empty_osstring: OsString = "".to_string().into();

        if positionals.is_empty() && !print_and_quit {
            println!("Missing required argument: PATTERN\n");
            print_help(false, 1);
        };
        let query = positionals
            .get(0)
            .unwrap_or(&empty_osstring)
            .to_string_lossy()
            .to_string();

        let files: Vec<OsString> = positionals.into_iter().skip(1).collect();

        let lang = cmds
            .iter()
            .filter_map(|c| {
                if let OptionCommand::Language(l) = c {
                    Some(PARSED_DB[l].extensions[0].to_string())
                } else {
                    None
                }
            })
            .last()
            .unwrap_or_else(|| extension.to_string_lossy().to_string());

        let mut opts: Options = EXTENSION_TO_SETTINGS
            .get(&lang)
            .cloned()
            .unwrap_or_default();

        for cmd in cmds {
            match cmd {
                OptionCommand::AddStringCharacter(s) => {
                    opts.string_characters.insert(s);
                }
                OptionCommand::RemoveStringCharacter(s) => {
                    opts.string_characters.remove(&s);
                }
                OptionCommand::AddSingleComment(s) => {
                    opts.single_line_comments.insert(s);
                }
                OptionCommand::RemoveSingleComment(s) => {
                    opts.single_line_comments.remove(&s);
                }
                OptionCommand::AddMultiComment(start, end) => {
                    opts.multi_line_comments.insert((start, end));
                }
                OptionCommand::RemoveMultiComment(start, end) => {
                    opts.multi_line_comments.remove(&(start, end));
                }
                OptionCommand::OnlyMatching => opts.only_matching = true,
                OptionCommand::PrintOptionsAndQuit => {}
                OptionCommand::Language(_) => {}
            }
        }

        if print_and_quit {
            print_options(opts);
        }

        opts.query = query;
        opts.paths = files;

        opts
    }

    /// Is `c` an open paren for the current file type?
    /// ```
    /// use syns::options::Options;
    /// let options = Options::new("js".as_ref(), &vec!["syns", "query", "filename"]);
    /// assert!(options.is_open_paren("{"));
    /// assert!(!options.is_open_paren("}"));
    /// ```
    pub fn is_open_paren(&self, c: &str) -> bool {
        c == "(" || c == "[" || c == "{"
    }

    /// Is `c` a close paren for the current file type?
    /// ```
    /// use syns::options::Options;
    /// let options = Options::new("js".as_ref(), &vec!["syns", "query", "filename"]);
    /// assert!(!options.is_close_paren("{"));
    /// assert!(options.is_close_paren("}"));
    /// ```
    pub fn is_close_paren(&self, c: &str) -> bool {
        c == ")" || c == "]" || c == "}"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_options() {
        let options = Options::new("js".as_ref(), &["syns", "query", "filename"]);
        assert_eq!(options.query, "query");
        assert_eq!(options.paths[0], "filename");
    }

    #[test]
    fn options_parens() {
        let options = Options::new("js".as_ref(), &["syns", "query", "filename"]);
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
    fn builtin_json_is_valid() {
        serde_json::from_str::<HashMap<String, Defaults>>(BUILTIN_DATABASE).unwrap();
    }
}
