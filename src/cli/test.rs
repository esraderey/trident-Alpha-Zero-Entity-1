use std::path::PathBuf;
use std::process;

use super::{resolve_input, resolve_options};

pub fn cmd_test(input: PathBuf, target: &str, profile: &str) {
    let ri = resolve_input(&input);

    let options = resolve_options(target, profile, ri.project.as_ref());
    let result = trident::run_tests(&ri.entry, &options);

    match result {
        Ok(report) => {
            eprintln!("{}", report);
        }
        Err(_) => {
            process::exit(1);
        }
    }
}
