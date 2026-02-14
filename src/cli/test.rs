use std::path::{Path, PathBuf};
use std::process;

use super::resolve_options;

pub fn cmd_test(input: PathBuf, target: &str, profile: &str) {
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

    let options = resolve_options(target, profile, None);
    let result = trident::run_tests(&entry, &options);

    match result {
        Ok(report) => {
            eprintln!("{}", report);
        }
        Err(_) => {
            process::exit(1);
        }
    }
}
