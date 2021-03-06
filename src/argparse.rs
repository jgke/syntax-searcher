//! Argument parsing.

use std::ffi::{OsStr, OsString};
use std::os::unix::prelude::OsStrExt;

/// A single command-line argument.
#[derive(Debug, Clone, PartialEq)]
pub enum Arg {
    /// (parsed_char, entire_match, index)
    Short(char, OsString, usize),
    /// (parsed_match, entire_match, index)
    Long(String, OsString, usize),
    /// (positional_arg, index)
    Positional(OsString, usize),
}

/// A reference to [`Arg`].
#[derive(Debug, Clone, PartialEq)]
pub enum ArgRef<'a> {
    /// A single matched char.
    Short(char),
    /// A long option.
    Long(&'a str),
    /// Positional argument.
    Positional,
}

impl Arg {
    /// Convert into a [`ArgRef`].
    pub fn as_ref(&self) -> ArgRef<'_> {
        match self {
            Arg::Short(c, _, _) => ArgRef::Short(*c),
            Arg::Long(ref s, _, _) => ArgRef::Long(s),
            Arg::Positional(_, _) => ArgRef::Positional,
        }
    }

    /// Get the entire argument (eg. --foo).
    pub fn entire_match(self) -> OsString {
        match self {
            Arg::Short(_, s, _) => s,
            Arg::Long(_, s, _) | Arg::Positional(s, _) => s,
        }
    }

    /// Get the index of this argument.
    pub fn index(&self) -> usize {
        match self {
            Arg::Short(_, _, i) | Arg::Long(_, _, i) | Arg::Positional(_, i) => *i,
        }
    }
}

impl From<Arg> for OsString {
    fn from(arg: Arg) -> OsString {
        match arg {
            Arg::Short(c, _, _) => c.to_string().into(),
            Arg::Long(s, _, _) => s.into(),
            Arg::Positional(s, _) => s,
        }
    }
}

/// Parse arguments from `args`.
pub fn parse_args<S: AsRef<OsStr>>(args: &[S]) -> Vec<Arg> {
    let mut result = Vec::new();
    let double_dash = OsString::from("--").len();
    let mut index = 0;

    let mut iter = args.iter();

    for s in &mut iter {
        index += 1;
        let s = s.as_ref();
        let lossy = s.to_string_lossy();
        if lossy == "-" {
            // This means 'stdin'
            result.push(Arg::Positional(s.to_os_string(), index));
        } else if lossy == "--" {
            // The rest of parameters are positionals
            break;
        } else if lossy.starts_with("--") {
            let os_str = String::from_utf8(s.as_bytes()[double_dash..].to_vec())
                .expect("Argument contained invalid UTF-8");
            result.push(Arg::Long(os_str, s.to_os_string(), index));
        } else if lossy.starts_with('-') {
            let mut iter = lossy.chars();
            iter.next();
            result.push(Arg::Short(
                iter.next().expect("unreachable"),
                s.to_os_string(),
                index,
            ));
            for c in iter {
                result.push(Arg::Short(c, s.to_os_string(), index));
            }
        } else {
            result.push(Arg::Positional(s.to_os_string(), index));
        }
    }

    for s in &mut iter {
        result.push(Arg::Positional(s.as_ref().to_os_string(), index));
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple() {
        assert_eq!(parse_args(&["foo"]), vec![Arg::Positional("foo".into(), 1)]);
        assert_eq!(
            parse_args(&["foo", "bar baz"]),
            vec![
                Arg::Positional("foo".into(), 1),
                Arg::Positional("bar baz".into(), 2)
            ]
        );
        assert_eq!(
            parse_args(&["--foo"]),
            vec![Arg::Long("foo".into(), "--foo".into(), 1),]
        );
        assert_eq!(
            parse_args(&["--", "--foo"]),
            vec![Arg::Positional("--foo".into(), 1),]
        );
        assert_eq!(
            parse_args(&["-ab"]),
            vec![
                Arg::Short('a', "-ab".into(), 1),
                Arg::Short('b', "-ab".into(), 1),
            ]
        );
    }
}
