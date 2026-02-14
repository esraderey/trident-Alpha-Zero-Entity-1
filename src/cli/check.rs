use std::path::{Path, PathBuf};
use std::process;

use super::find_program_source;

pub fn cmd_check(input: PathBuf, costs: bool, _target: &str, _profile: &str) {
    let entry = if input.is_dir() {
        let toml_path = input.join("trident.toml");
        if !toml_path.exists() {
            eprintln!("error: no trident.toml found in '{}'", input.display());
            process::exit(1);
        }
        let project = match trident::project::Project::load(&toml_path) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("error: {}", e.message);
                process::exit(1);
            }
        };
        project.entry
    } else if input.extension().is_some_and(|e| e == "tri") {
        if let Some(toml_path) =
            trident::project::Project::find(input.parent().unwrap_or(Path::new(".")))
        {
            let project = match trident::project::Project::load(&toml_path) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("error: {}", e.message);
                    process::exit(1);
                }
            };
            project.entry
        } else {
            input.clone()
        }
    } else {
        eprintln!("error: input must be a .tri file or project directory");
        process::exit(1);
    };

    match trident::check_project(&entry) {
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
