use std::ffi::{OsStr, OsString};
use std::os::unix::prelude::OsStrExt;
use std::os::unix::prelude::OsStringExt;

#[derive(Debug, Clone, PartialEq)]
pub enum ArgRef<'a> {
    Short(char),
    Long(&'a str),
    Positional(&'a str),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Arg {
    Short(char, OsString, usize),
    Long(OsString, OsString, usize),
    Positional(OsString, OsString, usize),
}

impl Arg {
    #[allow(clippy::unwrap_used)]
    pub fn as_ref(&self) -> ArgRef<'_> {
        match self {
            Arg::Short(c, _, _) => ArgRef::Short(*c),
            Arg::Long(ref s, _, _) => ArgRef::Long(s.to_str().unwrap()),
            Arg::Positional(ref s, _, _) => ArgRef::Positional(s.to_str().unwrap()),
        }
    }

    pub fn entire_match(self) -> OsString {
        match self {
            Arg::Short(_, s, _) | Arg::Long(_, s, _) | Arg::Positional(_, s, _) => s,
        }
    }

    pub fn index(&self) -> usize {
        match self {
            Arg::Short(_, _, i) | Arg::Long(_, _, i) | Arg::Positional(_, _, i) => *i,
        }
    }
}

impl From<Arg> for OsString {
    fn from(arg: Arg) -> OsString {
        match arg {
            Arg::Short(c, _, _) => c.to_string().into(),
            Arg::Long(s, _, _) | Arg::Positional(s, _, _) => s,
        }
    }
}

pub fn parse_args<S: AsRef<OsStr>>(args: &[S]) -> Vec<Arg> {
    let mut result = Vec::new();
    let double_dash = OsString::from("--").len();
    let mut rest_positional = false;
    let mut index = 0;

    for s in args {
        index += 1;
        let s = s.as_ref();
        let lossy = s.to_string_lossy();
        if lossy == "-" {
            result.push(Arg::Positional(s.to_os_string(), s.to_os_string(), index));
        } else if !rest_positional && lossy == "--" {
            rest_positional = true;
            result.push(Arg::Positional(s.to_os_string(), s.to_os_string(), index));
        } else if !rest_positional && lossy.starts_with("--") {
            result.push(Arg::Long(
                OsString::from_vec(s.as_bytes()[double_dash..].iter().copied().collect()),
                s.to_os_string(),
                index,
            ));
        } else if !rest_positional && lossy.starts_with('-') {
            let mut iter = lossy.chars();
            iter.next();
            result.push(Arg::Short(iter.next().expect("unreachable"), s.to_os_string(), index));
            loop {
                let s = iter.as_str().to_string();
                if let Some(c) = iter.next() {
                    result.push(Arg::Short(c, s.into(), index));
                } else {
                    break;
                }
            }
        } else {
            result.push(Arg::Positional(s.to_os_string(), s.to_os_string(), index));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple() {
        assert_eq!(parse_args(&["foo"]), vec![Arg::Positional(
                "foo".into(),
                "foo".into(),
                1
                )]);
        assert_eq!(
            parse_args(&["foo", "bar baz"]),
            vec![
                Arg::Positional("foo".into(), "foo".into(), 1),
                Arg::Positional("bar baz".into(), "bar baz".into(), 2)
            ]
        );
    }
}
