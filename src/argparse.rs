use std::ffi::{OsStr, OsString};
use std::os::unix::prelude::OsStrExt;
use std::os::unix::prelude::OsStringExt;

#[derive(Debug, Clone, PartialEq)]
pub enum ArgRef<'a> {
    Short(&'a str),
    Long(&'a str),
    Positional(&'a str),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Arg {
    Short(OsString),
    Long(OsString),
    Positional(OsString),
}

impl Arg {
    pub fn as_ref(&self) -> ArgRef<'_> {
        match self {
            Arg::Short(ref s) => ArgRef::Short(s.to_str().unwrap()),
            Arg::Long(ref s) => ArgRef::Long(s.to_str().unwrap()),
            Arg::Positional(ref s) => ArgRef::Positional(s.to_str().unwrap()),
        }
    }
}

impl From<Arg> for OsString {
    fn from(arg: Arg) -> OsString {
        match arg {
            Arg::Short(s) | Arg::Long(s) | Arg::Positional(s) => s,
        }
    }
}

pub fn parse_args<S: AsRef<OsStr>>(args: &[S]) -> Vec<Arg> {
    let mut result = Vec::new();
    let double_dash = OsString::from("--").len();
    let mut rest_positional = false;

    for s in args {
        let s = s.as_ref();
        let lossy = s.to_string_lossy();
        if lossy == "-" {
            result.push(Arg::Positional(s.to_os_string()))
        } else if !rest_positional && lossy == "--" {
            rest_positional = true;
            result.push(Arg::Positional(s.to_os_string()))
        } else if !rest_positional && lossy.starts_with("--") {
            result.push(Arg::Long(OsString::from_vec(
                s.as_bytes()[double_dash..].iter().copied().collect(),
            )));
        } else if !rest_positional && lossy.starts_with('-') {
            result.extend(
                lossy
                    .chars()
                    .skip(1)
                    .map(|c| Arg::Short(c.to_string().into())),
            );
        } else {
            result.push(Arg::Positional(s.to_os_string()))
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple() {
        assert_eq!(parse_args(&["foo"]), vec![Arg::Positional("foo".into())]);
        assert_eq!(
            parse_args(&["foo", "bar baz"]),
            vec![
                Arg::Positional("foo".into()),
                Arg::Positional("bar baz".into())
            ]
        );
    }
}
