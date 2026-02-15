//! Options parsing and handling.

use crate::argparse::{parse_args, Arg, ArgRef};
use itertools::Itertools;
use lazy_static::lazy_static;
use log::warn;
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::iter::Peekable;
use termcolor::ColorChoice;

/// Parsed options.
#[derive(Clone, Debug)]
pub struct Options {
    /// File search paths.
    pub paths: Vec<OsString>,
    /// Query string.
    pub query: String,
    /// Only use paths matching this regex.
    pub only_files_matching: Option<Regex>,
    /// Ignore paths matching this regex.
    pub ignore_files_matching: Option<Regex>,

    /// Set of strings which start or end a string literal (eg. "'").
    pub string_characters: HashSet<String>,
    /// Set of strings which start or end a single-line comment (eg. "//").
    pub single_line_comments: HashSet<String>,
    /// Set of strings which start and end a multi-line comment (eg. ("/*", "*/")).
    pub multi_line_comments: HashSet<(String, String)>,
    /// List of block openers (eg. "(" or "do")
    pub block_openers: HashSet<String>,
    /// List of block closers (eg. ")" or "end")
    pub block_closers: HashSet<String>,
    /// Regex to match first letter of an identifier
    pub identifier_regex_start: Regex,
    /// Regex to match non-first letters of an identifier
    pub identifier_regex_continue: Regex,
    /// Parse '..' as a range.
    pub ranges: bool,
    /// List of regex literal delimiters (eg. "/")
    pub regex_delimiters: HashSet<String>,
    /// Parse <> as type parameter delimiters when they follow an identifier.
    pub type_parameter_parsing: bool,

    /// Print only matching parts of the source code.
    pub only_matching: bool,
    /// Only print matching files' names rather than actual matches.
    pub only_print_filenames: bool,
    /// Don't print any filenames.
    pub dont_print_filenames: bool,
    /// Follow symlinks.
    pub follow_symlinks: bool,
    /// Search binary files as if they were text.
    pub search_binary: bool,
    /// Use colored output.
    pub color: ColorChoice,

    /// Print the state machine as a dot graph and exit.
    pub dump_machine: bool,
}

#[derive(Clone, Debug)]
enum OptionCommand {
    AddStringCharacter(String),
    RemoveStringCharacter(String),
    AddSingleComment(String),
    RemoveSingleComment(String),
    AddMultiComment(String, String),
    RemoveMultiComment(String, String),
    AddBlockSeparator(String, String),
    RemoveBlockOpener(String),
    RemoveBlockCloser(String),
    Language(String),
    Identifier(Regex, Regex),
    OnlyFilesMatching(Regex),
    IgnoreFilesMatching(Regex),
    OnlyMatching,
    OnlyPrintFilenames,
    DontPrintFilenames,
    FollowSymlinks,
    SearchBinary,
    TypeParameterParsing,
    NoTypeParameterParsing,
    Color(ColorChoice),
    DumpMachine,
}

#[derive(Clone, Debug, Deserialize)]
struct BuiltinLanguageDefaults {
    identifier: Vec<String>,
    extensions: Vec<String>,
    strings: Vec<String>,
    single_comments: Vec<String>,
    multi_comments: Vec<(String, String)>,
    blocks: Option<Vec<(String, String)>>, // default () [] {}
    regex: Vec<String>,
    #[serde(default)]
    type_parameter_parsing: bool,
}

const BUILTIN_DATABASE: &str = include_str!("../config.json");

lazy_static! {
    static ref PARSED_DB: HashMap<String, BuiltinLanguageDefaults> =
        serde_json::from_str(BUILTIN_DATABASE).unwrap_or_else(|e| {
            warn!("Built-in JSON database has a syntax error: {}", e);
            HashMap::new()
        });
    static ref EXTENSION_TO_SETTINGS: HashMap<String, Options> = {
        let mut res = HashMap::new();
        let default_opts = Options::default();

        for ty in PARSED_DB.values() {
            let opts = Options {
                string_characters: ty.strings.iter().cloned().collect(),
                single_line_comments: ty.single_comments.iter().cloned().collect(),
                multi_line_comments: ty.multi_comments.iter().cloned().collect(),
                block_openers: ty
                    .blocks
                    .as_ref()
                    .map(|blocks| blocks.iter().map(|(start, _)| start.clone()).collect())
                    .unwrap_or_else(|| default_opts.block_openers.clone()),
                block_closers: ty
                    .blocks
                    .as_ref()
                    .map(|blocks| blocks.iter().map(|(_, end)| end.clone()).collect())
                    .unwrap_or_else(|| default_opts.block_closers.clone()),
                identifier_regex_start: ty
                    .identifier
                    .first()
                    .map(|r| Regex::new(r).expect("Invalid identifier regex in builtin database"))
                    .unwrap_or_else(|| default_opts.identifier_regex_start.clone()),
                identifier_regex_continue: ty
                    .identifier
                    .get(1)
                    .map(|r| Regex::new(r).expect("Invalid identifier regex in builtin database"))
                    .unwrap_or_else(|| default_opts.identifier_regex_continue.clone()),
                regex_delimiters: ty.regex.iter().cloned().collect(),
                type_parameter_parsing: ty.type_parameter_parsing,
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
            only_files_matching: None,
            ignore_files_matching: None,
            string_characters: ["\"", "'", "`"].iter().map(|s| s.to_string()).collect(),
            single_line_comments: ["//"].iter().map(|s| s.to_string()).collect(),
            multi_line_comments: [("/*", "*/")]
                .iter()
                .map(|(from, to)| (from.to_string(), to.to_string()))
                .collect(),
            block_openers: vec!["(".to_string(), "[".to_string(), "{".to_string()]
                .into_iter()
                .collect(),
            block_closers: vec![")".to_string(), "]".to_string(), "}".to_string()]
                .into_iter()
                .collect(),
            identifier_regex_start: Regex::new("[\\p{ID_Start}_]").expect("internal error"),
            identifier_regex_continue: Regex::new("\\p{ID_Continue}").expect("internal error"),
            regex_delimiters: HashSet::new(),
            type_parameter_parsing: false,
            ranges: true,

            only_matching: false,
            only_print_filenames: false,
            dont_print_filenames: false,
            follow_symlinks: false,
            search_binary: false,
            color: ColorChoice::Auto,
            dump_machine: false,
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
  -h, --help                    Display this message
  --lang LANGUAGE               Force defaults for LANGUAGE. Call 'syns --lang'
                                to display available languages.
  --[no-]color                  Enable or disable color output

  -i, --identifier START END    Match identifiers using START regex for the
                                first character and CONT for the rest
  -s, --[no-]string CHARS       Add or remove CHARS from string delimiters
  -c, --[no-]comment CHARS      Add or remove CHARS from single-line comments
  -m, --[no-]multi BEGIN END    Add or remove (BEGIN, END) from multi-line
                                comments

  --only-files-matching REGEX   Only scan files matching REGEX
  --ignore-files-matching REGEX Don't scan files matching REGEX
  -o, --only-matching           Print only the matched parts
  -l, --only-print-filenames    Only print matching files' names
  -I, --dont-print-filenames    Don't print any filenames
  -L, --follow                  Follow symlinks
  -a, --text                    Search binary files as if they were text
  --[no-]type-parameter-parsing Parse <> as type parameter delimiters
"#,
            filename
        );
    }
    std::process::exit(status)
}

fn print_langs() -> ! {
    println!("Available languages:");
    for (lang, defs) in PARSED_DB.iter().sorted_by(|(k1, _), (k2, _)| k1.cmp(k2)) {
        println!("- {} [{}]", lang, defs.extensions.join(", "));
    }
    std::process::exit(0)
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

            ArgRef::Short('i') | ArgRef::Long("identifier") => {
                if let Some(start) = get_whole_arg(&mut arg_iter) {
                    if let Some(cont) = get_whole_arg(&mut arg_iter) {
                        let start = start.to_string_lossy().to_string();
                        let cont = cont.to_string_lossy().to_string();

                        match (Regex::new(&start), Regex::new(&cont)) {
                            (Ok(r1), Ok(r2)) => OptionCommand::Identifier(r1, r2),
                            (Err(e), _) | (_, Err(e)) => {
                                println!("Invalid regex argument for --identifier: {}", e);
                                print_help(false, 1)
                            }
                        }
                    } else {
                        println!("Missing second argument for --identifier");
                        print_help(false, 1)
                    }
                } else {
                    println!("Missing argument for --identifier");
                    print_help(false, 1)
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
            ArgRef::Short('b') | ArgRef::Long("block") => {
                if let Some(start) = get_whole_arg(&mut arg_iter) {
                    if let Some(end) = get_whole_arg(&mut arg_iter) {
                        OptionCommand::AddBlockSeparator(
                            start.to_string_lossy().to_string(),
                            end.to_string_lossy().to_string(),
                        )
                    } else {
                        println!("Missing second argument for --block");
                        print_help(false, 1)
                    }
                } else {
                    println!("Missing argument for --block");
                    print_help(false, 1)
                }
            }

            ArgRef::Long("no-block-opener") => {
                if let Some(arg) = get_whole_arg(&mut arg_iter) {
                    OptionCommand::RemoveBlockOpener(arg.to_string_lossy().to_string())
                } else {
                    println!("Missing argument for --no-block-opener");
                    print_help(false, 1)
                }
            }

            ArgRef::Long("no-block-closer") => {
                if let Some(arg) = get_whole_arg(&mut arg_iter) {
                    OptionCommand::RemoveBlockCloser(arg.to_string_lossy().to_string())
                } else {
                    println!("Missing argument for --no-block-closer");
                    print_help(false, 1)
                }
            }

            ArgRef::Long("only-files-matching") => {
                if let Some(arg) = get_whole_arg(&mut arg_iter) {
                    let s = arg.to_string_lossy().to_string();
                    match Regex::new(&s) {
                        Ok(r) => OptionCommand::OnlyFilesMatching(r),
                        Err(e) => {
                            println!("Invalid regex argument for --only-files-matching: {}", e);
                            print_help(false, 1)
                        }
                    }
                } else {
                    println!("Missing argument for --only-files-matching");
                    print_help(false, 1)
                }
            }
            ArgRef::Long("ignore-files-matching") => {
                if let Some(arg) = get_whole_arg(&mut arg_iter) {
                    let s = arg.to_string_lossy().to_string();
                    match Regex::new(&s) {
                        Ok(r) => OptionCommand::IgnoreFilesMatching(r),
                        Err(e) => {
                            println!("Invalid regex argument for --ignore-files-matching: {}", e);
                            print_help(false, 1)
                        }
                    }
                } else {
                    println!("Missing argument for --ignore-files-matching");
                    print_help(false, 1)
                }
            }

            ArgRef::Long("color") => OptionCommand::Color(ColorChoice::Always),
            ArgRef::Long("no-color") => OptionCommand::Color(ColorChoice::Never),

            ArgRef::Short('o') | ArgRef::Long("only-matching") => OptionCommand::OnlyMatching,
            ArgRef::Short('l') | ArgRef::Long("only-print-filenames") => {
                OptionCommand::OnlyPrintFilenames
            }
            ArgRef::Short('I') | ArgRef::Long("dont-print-filenames") => {
                OptionCommand::DontPrintFilenames
            }
            ArgRef::Short('L') | ArgRef::Long("follow") => OptionCommand::FollowSymlinks,
            ArgRef::Short('a') | ArgRef::Long("text") => OptionCommand::SearchBinary,
            ArgRef::Long("type-parameter-parsing") => OptionCommand::TypeParameterParsing,
            ArgRef::Long("no-type-parameter-parsing") => OptionCommand::NoTypeParameterParsing,
            ArgRef::Long("dump-machine") => OptionCommand::DumpMachine,

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
        let empty_osstring: OsString = "".to_string().into();

        if positionals.is_empty() {
            println!("Missing required argument: PATTERN\n");
            print_help(false, 1);
        };
        let query = positionals
            .first()
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
            .next_back()
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
                OptionCommand::AddBlockSeparator(start, end) => {
                    opts.block_openers.insert(start);
                    opts.block_closers.insert(end);
                }
                OptionCommand::RemoveBlockOpener(start) => {
                    opts.block_openers.remove(&start);
                }
                OptionCommand::RemoveBlockCloser(end) => {
                    opts.block_closers.remove(&end);
                }
                OptionCommand::OnlyFilesMatching(regex) => {
                    opts.only_files_matching = Some(regex);
                }
                OptionCommand::IgnoreFilesMatching(regex) => {
                    opts.ignore_files_matching = Some(regex);
                }
                OptionCommand::Identifier(start, cont) => {
                    opts.identifier_regex_start = start;
                    opts.identifier_regex_continue = cont;
                }
                OptionCommand::OnlyMatching => opts.only_matching = true,
                OptionCommand::OnlyPrintFilenames => opts.only_print_filenames = true,
                OptionCommand::DontPrintFilenames => opts.dont_print_filenames = true,
                OptionCommand::FollowSymlinks => opts.follow_symlinks = true,
                OptionCommand::SearchBinary => opts.search_binary = true,
                OptionCommand::TypeParameterParsing => opts.type_parameter_parsing = true,
                OptionCommand::NoTypeParameterParsing => opts.type_parameter_parsing = false,
                OptionCommand::Color(choice) => opts.color = choice,
                OptionCommand::DumpMachine => opts.dump_machine = true,
                OptionCommand::Language(_) => {}
            }
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
        self.block_openers.iter().any(|s| c == s)
    }

    /// Is `c` a close paren for the current file type?
    /// ```
    /// use syns::options::Options;
    /// let options = Options::new("js".as_ref(), &vec!["syns", "query", "filename"]);
    /// assert!(!options.is_close_paren("{"));
    /// assert!(options.is_close_paren("}"));
    /// ```
    pub fn is_close_paren(&self, c: &str) -> bool {
        self.block_closers.iter().any(|e| c == e)
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
        serde_json::from_str::<HashMap<String, BuiltinLanguageDefaults>>(BUILTIN_DATABASE)
            .expect("Failed to parse builtin JSON database, check config.json");
    }

    #[test]
    fn multiple_paths() {
        let options = Options::new("js".as_ref(), &["syns", "query", "a", "b", "c"]);
        assert_eq!(options.query, "query");
        assert_eq!(options.paths, vec!["a", "b", "c"]);
    }

    #[test]
    fn lang_flag() {
        let options = Options::new("".as_ref(), &["syns", "--lang", "c", "query", "file"]);
        assert!(options.string_characters.contains("\""));
        assert!(options.string_characters.contains("'"));
        assert!(!options.string_characters.contains("`"));
        assert!(options.single_line_comments.contains("//"));
    }

    #[test]
    fn add_string_character_short() {
        let options = Options::new("".as_ref(), &["syns", "-s", "|", "query", "file"]);
        assert!(options.string_characters.contains("|"));
    }

    #[test]
    fn add_string_character_long() {
        let options = Options::new("".as_ref(), &["syns", "--string", "|", "query", "file"]);
        assert!(options.string_characters.contains("|"));
    }

    #[test]
    fn remove_string_character() {
        let options = Options::new("".as_ref(), &["syns", "--no-string", "'", "query", "file"]);
        assert!(!options.string_characters.contains("'"));
        assert!(options.string_characters.contains("\""));
    }

    #[test]
    fn add_single_comment_short() {
        let options = Options::new("".as_ref(), &["syns", "-c", "#", "query", "file"]);
        assert!(options.single_line_comments.contains("#"));
    }

    #[test]
    fn add_single_comment_long() {
        let options = Options::new("".as_ref(), &["syns", "--comment", "#", "query", "file"]);
        assert!(options.single_line_comments.contains("#"));
    }

    #[test]
    fn remove_single_comment() {
        let options = Options::new(
            "".as_ref(),
            &["syns", "--no-comment", "//", "query", "file"],
        );
        assert!(!options.single_line_comments.contains("//"));
    }

    #[test]
    fn add_multi_comment_short() {
        let options = Options::new("".as_ref(), &["syns", "-m", "<!--", "-->", "query", "file"]);
        assert!(options
            .multi_line_comments
            .contains(&("<!--".to_string(), "-->".to_string())));
    }

    #[test]
    fn add_multi_comment_long() {
        let options = Options::new(
            "".as_ref(),
            &["syns", "--multi", "<!--", "-->", "query", "file"],
        );
        assert!(options
            .multi_line_comments
            .contains(&("<!--".to_string(), "-->".to_string())));
    }

    #[test]
    fn remove_multi_comment() {
        let options = Options::new(
            "".as_ref(),
            &["syns", "--no-multi", "/*", "*/", "query", "file"],
        );
        assert!(!options
            .multi_line_comments
            .contains(&("/*".to_string(), "*/".to_string())));
    }

    #[test]
    fn add_block_separator_short() {
        let options = Options::new("".as_ref(), &["syns", "-b", "do", "end", "query", "file"]);
        assert!(options.block_openers.contains("do"));
        assert!(options.block_closers.contains("end"));
    }

    #[test]
    fn add_block_separator_long() {
        let options = Options::new(
            "".as_ref(),
            &["syns", "--block", "do", "end", "query", "file"],
        );
        assert!(options.block_openers.contains("do"));
        assert!(options.block_closers.contains("end"));
    }

    #[test]
    fn remove_block_opener() {
        let options = Options::new(
            "".as_ref(),
            &["syns", "--no-block-opener", "(", "query", "file"],
        );
        assert!(!options.block_openers.contains("("));
        assert!(options.block_openers.contains("["));
    }

    #[test]
    fn remove_block_closer() {
        let options = Options::new(
            "".as_ref(),
            &["syns", "--no-block-closer", ")", "query", "file"],
        );
        assert!(!options.block_closers.contains(")"));
        assert!(options.block_closers.contains("]"));
    }

    #[test]
    fn identifier_short() {
        let options = Options::new(
            "".as_ref(),
            &["syns", "-i", "[a-z]", "[a-z0-9]", "query", "file"],
        );
        assert!(options.identifier_regex_start.is_match("a"));
        assert!(!options.identifier_regex_start.is_match("A"));
        assert!(options.identifier_regex_continue.is_match("1"));
    }

    #[test]
    fn identifier_long() {
        let options = Options::new(
            "".as_ref(),
            &["syns", "--identifier", "[a-z]", "[a-z0-9]", "query", "file"],
        );
        assert!(options.identifier_regex_start.is_match("a"));
        assert!(!options.identifier_regex_start.is_match("A"));
    }

    #[test]
    fn only_files_matching() {
        let options = Options::new(
            "".as_ref(),
            &["syns", "--only-files-matching", "\\.rs$", "query", "file"],
        );
        let re = options.only_files_matching.unwrap();
        assert!(re.is_match("main.rs"));
        assert!(!re.is_match("main.js"));
    }

    #[test]
    fn ignore_files_matching() {
        let options = Options::new(
            "".as_ref(),
            &["syns", "--ignore-files-matching", "\\.rs$", "query", "file"],
        );
        let re = options.ignore_files_matching.unwrap();
        assert!(re.is_match("main.rs"));
        assert!(!re.is_match("main.js"));
    }

    #[test]
    fn only_matching_short() {
        let options = Options::new("".as_ref(), &["syns", "-o", "query", "file"]);
        assert!(options.only_matching);
    }

    #[test]
    fn only_matching_long() {
        let options = Options::new("".as_ref(), &["syns", "--only-matching", "query", "file"]);
        assert!(options.only_matching);
    }

    #[test]
    fn only_print_filenames_short() {
        let options = Options::new("".as_ref(), &["syns", "-l", "query", "file"]);
        assert!(options.only_print_filenames);
    }

    #[test]
    fn only_print_filenames_long() {
        let options = Options::new(
            "".as_ref(),
            &["syns", "--only-print-filenames", "query", "file"],
        );
        assert!(options.only_print_filenames);
    }

    #[test]
    fn dont_print_filenames_short() {
        let options = Options::new("".as_ref(), &["syns", "-I", "query", "file"]);
        assert!(options.dont_print_filenames);
    }

    #[test]
    fn dont_print_filenames_long() {
        let options = Options::new(
            "".as_ref(),
            &["syns", "--dont-print-filenames", "query", "file"],
        );
        assert!(options.dont_print_filenames);
    }

    #[test]
    fn follow_symlinks_short() {
        let options = Options::new("".as_ref(), &["syns", "-L", "query", "file"]);
        assert!(options.follow_symlinks);
    }

    #[test]
    fn follow_symlinks_long() {
        let options = Options::new("".as_ref(), &["syns", "--follow", "query", "file"]);
        assert!(options.follow_symlinks);
    }

    #[test]
    fn search_binary_short() {
        let options = Options::new("".as_ref(), &["syns", "-a", "query", "file"]);
        assert!(options.search_binary);
    }

    #[test]
    fn search_binary_long() {
        let options = Options::new("".as_ref(), &["syns", "--text", "query", "file"]);
        assert!(options.search_binary);
    }

    #[test]
    fn type_parameter_parsing_flag() {
        let options = Options::new(
            "".as_ref(),
            &["syns", "--type-parameter-parsing", "query", "file"],
        );
        assert!(options.type_parameter_parsing);
    }

    #[test]
    fn no_type_parameter_parsing_flag() {
        let options = Options::new(
            "".as_ref(),
            &[
                "syns",
                "--type-parameter-parsing",
                "--no-type-parameter-parsing",
                "query",
                "file",
            ],
        );
        assert!(!options.type_parameter_parsing);
    }

    #[test]
    fn color_flag() {
        let options = Options::new("".as_ref(), &["syns", "--color", "query", "file"]);
        assert_eq!(options.color, ColorChoice::Always);
    }

    #[test]
    fn no_color_flag() {
        let options = Options::new("".as_ref(), &["syns", "--no-color", "query", "file"]);
        assert_eq!(options.color, ColorChoice::Never);
    }

    #[test]
    fn dump_machine_flag() {
        let options = Options::new("".as_ref(), &["syns", "--dump-machine", "query", "file"]);
        assert!(options.dump_machine);
    }

    #[test]
    fn default_options() {
        let opts = Options::default();
        assert!(opts.string_characters.contains("\""));
        assert!(opts.string_characters.contains("'"));
        assert!(opts.string_characters.contains("`"));
        assert!(opts.single_line_comments.contains("//"));
        assert!(opts
            .multi_line_comments
            .contains(&("/*".to_string(), "*/".to_string())));
        assert!(opts.block_openers.contains("("));
        assert!(opts.block_closers.contains(")"));
        assert!(!opts.only_matching);
        assert!(!opts.only_print_filenames);
        assert!(!opts.dont_print_filenames);
        assert!(!opts.follow_symlinks);
        assert!(!opts.search_binary);
        assert!(!opts.type_parameter_parsing);
        assert!(opts.ranges);
        assert_eq!(opts.color, ColorChoice::Auto);
        assert!(!opts.dump_machine);
    }

    #[test]
    fn extension_based_defaults() {
        let options = Options::new("rs".as_ref(), &["syns", "query", "file"]);
        // Rust uses " for strings but not ' or `
        assert!(options.string_characters.contains("\""));
        assert!(!options.string_characters.contains("`"));
    }

    #[test]
    fn combined_flags() {
        let options = Options::new(
            "".as_ref(),
            &[
                "syns",
                "-o",
                "-l",
                "-L",
                "-a",
                "--no-color",
                "query",
                "file",
            ],
        );
        assert!(options.only_matching);
        assert!(options.only_print_filenames);
        assert!(options.follow_symlinks);
        assert!(options.search_binary);
        assert_eq!(options.color, ColorChoice::Never);
    }
}
