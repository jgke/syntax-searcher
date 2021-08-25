// This file exists mainly to enable doctests.
//! syntax-searcher -- Generic source code searcher for paren-delimited languages.

#[macro_use]
pub mod collection;

pub mod argparse;
pub mod compiler;
pub mod options;
pub mod parser;
pub mod psi;
pub mod query;
pub mod run;
pub mod tokenizer;
pub mod wrappers;

pub use run::run_cached;
