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

use ignore::WalkBuilder;
use log::info;
use std::env;
use std::fs::{self, File};
use std::io;

use options::*;

fn run_file(
    options: &Options,
    file: Result<ignore::DirEntry, ignore::Error>,
) -> Result<(), Box<dyn std::error::Error>> {
    let file = file?;
    let path = file.path();
    let attr = fs::metadata(&path)?;
    if !attr.is_dir() {
        let fp = File::open(&path)?;
        run::run(options, &path, fp);
    }
    Ok(())
}

#[cfg(not(tarpaulin_include))]
fn main() -> io::Result<()> {
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "warn"),
    );
    let args: Vec<String> = env::args().collect();
    let options = Options::new(&args);
    info!("Using options: {:#?}", options);
    let default_path = "./".into();
    let mut walker = WalkBuilder::new(options.paths.get(0).unwrap_or(&default_path));
    for path in options.paths.iter().skip(1) {
        walker.add(path);
    }
    for f in walker.build() {
        if let Err(e) = run_file(&options, f) {
            println!("Err: {}", e);
        }
    }
    Ok(())
}
