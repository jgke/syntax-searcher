#![warn(missing_docs)]
#![warn(clippy::unwrap_used)]

//! syntax-searcher -- Generic source code searcher for paren-delimited languages.

#[macro_use]
mod collection;

mod argparse;
mod compiler;
mod options;
mod parser;
mod psi;
mod query;
mod run;
mod tokenizer;
mod wrappers;

use crate::query::Query;
use ignore::WalkBuilder;
use log::info;
use std::collections::HashMap;
use std::env;
use std::fs::{self, File};
use std::io;

use options::*;

fn run_file(
    query: &Query,
    options: &Options,
    file: ignore::DirEntry,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = file.path();
    let attr = fs::metadata(&path)?;
    if !attr.is_dir() {
        let fp = File::open(&path)?;
        run::run_cached(query, options, &path, fp);
    }
    Ok(())
}

#[cfg(not(tarpaulin_include))]
fn main() -> io::Result<()> {
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "warn"),
    );
    let args: Vec<String> = env::args().collect();
    let mut opt_cache = HashMap::new();
    let mut query_cache = HashMap::new();
    let txt: std::ffi::OsString = "txt".to_string().into();
    // This options is only used for enumerating paths
    let options = Options::new(&txt, &args);
    let default_path = "./".into();
    let mut walker = WalkBuilder::new(options.paths.get(0).unwrap_or(&default_path));
    for path in options.paths.iter().skip(1) {
        walker.add(path);
    }
    for f in walker.build() {
        if let Err(e) = {
            match f {
                Ok(f) => {
                    let file_path = std::path::Path::new(f.path());

                    let ext = file_path.extension().unwrap_or(&txt).to_owned();

                    let options = opt_cache.entry(ext.clone()).or_insert_with_key(|ext| {
                        // This options accounts for proper file extensions
                        let opts = Options::new(ext, &args);
                        info!("Using options: {:#?}", opts);
                        opts
                    });
                    let query = query_cache.entry(ext).or_insert_with(|| {
                        // This options accounts for proper file extensions
                        let opts = Query::new(options);
                        opts
                    });

                    run_file(query, options, f)
                }
                Err(e) => Err(e.into()),
            }
        } {
            println!("Err: {}", e);
        }
    }
    Ok(())
}
