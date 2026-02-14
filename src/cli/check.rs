use std::path::PathBuf;
use std::process;

use super::{find_program_source, resolve_input};

pub fn cmd_check(input: PathBuf, costs: bool, _target: &str, _profile: &str) {
    let ri = resolve_input(&input);

    match trident::check_project(&ri.entry) {
        Ok(()) => {
            eprintln!("OK: {}", input.display());
        }
        Err(_) => {
            process::exit(1);
        }
    }

    if costs {
        if let Some(source_path) = find_program_source(&input) {
            let options = trident::CompileOptions::default();
            if let Ok(program_cost) = trident::analyze_costs_project(&source_path, &options) {
                eprintln!("\n{}", program_cost.format_report());
            }
        }
    }
}
