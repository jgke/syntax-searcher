#![warn(missing_docs)]
// This file exists mainly to enable doctests.
//! syntax-searcher -- Generic source code searcher for paren-delimited languages.

#[macro_use]
mod collection;

mod argparse;
mod compiler;
mod options;
mod parser;
pub mod psi;
mod query;
mod run;
mod tokenizer;
mod wrappers;

pub use run::run_cached;
