Syntax-scanner
==============

Generic source code searcher for paren-delimited languages.

Example:
--------

```
# Search for sprintf and its contents
$ syns 'sprintf(\.\*)' test-files/injection.php
[test-files/injection.php:2-4]
$query = sprintf("SELECT * FROM `Users` WHERE UserName='%s' AND Password='%s'",
                  $mysqli->real_escape_string($username),
                  $mysqli->real_escape_string($password));

# search potential SQL injections in Clojure
syns '(\./query \. [(str \.\*)])' test-files/injection.clj
[test-files/injection.clj:2-3]
  (j/query db
           [(str "select * from user where username = '" param "'")]))
```

Query language
==============

The query language is parsed with the same options as the source file. Query patterns
can be matched using backslash. The following commands are available:

| Pattern | Description |
| --- | --- |
| `\.` | Match any token or paren-delimited tree. |
| `\+` | Match the previous pattern one or more times. |
| `\*` | Match the previous pattern zero or more times. |
| `\"[regex]"` | Match any string literal with the regex pattern. |

Examples
--------

| Query | Explanation |
| --- | --- |
| `printf(\"Hello.*" \.\+)` | Find all calls to `printf` with more than one argument, where the format string starts with "Hello". |
| `\"SELECT.*"+\.\*` | Find all strings starting with "SELECT" where the string literal is followed by a plus sign and more tokens. |

Options
=======
| Flag | Description |
| --- | --- |
| `-h, --help` | Display help |
| `-s, --[no-]single` | Enable or disable single quote strings |
| `-d, --[no-]double` | Enable or disable double quote strings |
| `-b, --[no-]backtick` | Enable or disable backtick strings |
| `-o, --only-matching` | Print only the matched parts. |

Compiling from source
=====================

1) Install Rust eg. using [Rustup](https://rustup.rs/).
2) `cargo run -- pattern-here file-here`

Testing
=======

1) `cargo test`

Unimplemented features
======================

- Handle multiple file arguments
- Handle directories as arguments (recursively match every file under that directory)
- File extension based language detection for language-spesicifc defaults (eg. no single-quote strings for Rust/Clojure)
- More parsing strategies
    - Comments:
        - Comments starting with `--`
        - Comments starting with `#`
    - Strings:
        - Support for strings with various prefixes, eg. Python's `f`
        - Rust's raw strings
    - Literals:
        - Support for more number literals
        - Clojure/Ruby keyword literal
        - Arbitrary number suffixes (eg. CSS: 15px)
- More query improvements
    - Match group end with `\$`
    - Or-pattern with `\|`
    - Pattern grouping with `\(\)`
    - Backtracking matching
        - Currently the query doesn't backtrack, so it doesn't match everything it should
    - Linear time matching
        - This should be possible with the linear time regex state machine algorithms
- CLI improvements
    - Fine-tune output
    - JSON output

Comparison to other software
============================

`grep` and derivatives: Regular expressions are not powerful enough to parse
arbitrary paren-delimited expressions. It's possible to use grep extensions to
match eg. matching brackets. However, the syntax for that is quite clunky.
https://unix.stackexchange.com/questions/147662/grep-upto-matching-brackets

`semgrep` https://semgrep.dev/ Semgrep is implemented by having
language-specific parsers. This enables language-specific semantic analysis,
but with the tradeoff of supporting only a handful of languages.

License
=======

GNU Affero General Public License Version 3. See `LICENSE` for more details.
