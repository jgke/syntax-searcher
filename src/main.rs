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
use log::{debug, info};
use std::collections::HashMap;
use std::env;
use std::fs::{self, File};
use std::io;

use options::*;

fn run_file(
    query: &Query,
    options: &Options,
    file: ignore::DirEntry,
) -> Result<bool, Box<dyn std::error::Error>> {
    let path = file.path();
    let fp = File::open(path)?;
    Ok(run::run_cached(query, options, path, fp))
}

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
    let mut walker = WalkBuilder::new(options.paths.first().unwrap_or(&default_path));
    if options.follow_symlinks {
        walker.follow_links(true);
    }
    for path in options.paths.iter().skip(1) {
        walker.add(path);
    }
    let mut retval = 1;
    for f in walker.build() {
        let res = match f {
            Ok(f) => {
                let file_path = std::path::Path::new(f.path());
                let lossy_filename = file_path.to_string_lossy();
                if let Some(r) = &options.only_files_matching {
                    if !r.is_match(&lossy_filename) {
                        info!(
                            "Ignoring file {} as it didn't match regex '{:?}'",
                            &lossy_filename, &r
                        );
                        continue;
                    }
                }
                if let Some(r) = &options.ignore_files_matching {
                    if r.is_match(&lossy_filename) {
                        info!(
                            "Ignoring file {} as it matches regex '{:?}'",
                            &lossy_filename, &r
                        );
                        continue;
                    }
                }
                if let Ok(attr) = fs::metadata(file_path) {
                    if attr.is_dir() {
                        continue;
                    }
                }
                if let Ok(attr) = fs::symlink_metadata(file_path) {
                    if attr.is_symlink() && !options.follow_symlinks {
                        continue;
                    }
                }

                info!("Scanning file {}", lossy_filename);

                let ext = file_path.extension().unwrap_or(&txt).to_owned();

                let options = opt_cache.entry(ext.clone()).or_insert_with_key(|ext| {
                    // This options accounts for proper file extensions
                    let opts = Options::new(ext, &args);
                    debug!(
                        "Created new options for extension .{}:  {:#?}",
                        ext.to_string_lossy(),
                        opts
                    );
                    opts
                });
                let query = query_cache
                    .entry(ext)
                    .or_insert_with(|| Query::new(options));

                if options.dump_machine {
                    println!("{}", query.machine.to_dot_graph());
                    break;
                }

                run_file(query, options, f)
            }
            Err(e) => Err(e.into()),
        };
        match res {
            Ok(did_match) => {
                if retval == 1 && did_match {
                    retval = 0;
                }
            }
            Err(e) => {
                eprintln!("Err: {}", e);
                retval = 2;
            }
        }
    }

    std::process::exit(retval);
}
