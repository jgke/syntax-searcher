#![warn(missing_docs)]
// This file exists mainly to enable doctests.
//! syntax-searcher -- Generic source code searcher for paren-delimited languages.

mod argparse;
mod options;
mod parser;
pub mod psi;
mod query;
mod run;
mod tokenizer;

pub use run::run;
