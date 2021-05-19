#![warn(missing_docs)]
#![warn(clippy::unwrap_used)]

//! syntax-searcher -- Generic source code searcher for paren-delimited languages.

mod argparse;
mod options;
mod parser;
mod psi;
mod query;
mod run;
mod tokenizer;

use std::env;
use std::fs::File;
use std::io;
use log::info;

use options::*;

#[cfg(not(tarpaulin_include))]
fn main() -> io::Result<()> {
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "warn"),
    );
    let args: Vec<String> = env::args().collect();
    let options = Options::new(&args);
    info!("Using options: {:#?}", options);
    let fp = File::open(&options.filename)?;
    run::run(options, fp);
    Ok(())
}
