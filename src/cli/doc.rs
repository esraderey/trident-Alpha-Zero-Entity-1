use std::path::{Path, PathBuf};
use std::process;

use super::resolve_options;

pub fn cmd_doc(input: PathBuf, output: Option<PathBuf>, target: &str, profile: &str) {
    let (entry, project) = if input.is_dir() {
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
        let entry = project.entry.clone();
        (entry, Some(project))
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
            let entry = project.entry.clone();
            (entry, Some(project))
        } else {
            (input.clone(), None)
        }
    } else {
        eprintln!("error: input must be a .tri file or project directory");
        process::exit(1);
    };

    let options = resolve_options(target, profile, project.as_ref());
    let markdown = match trident::generate_docs(&entry, &options) {
        Ok(md) => md,
        Err(_) => {
            eprintln!("error: documentation generation failed (compilation errors)");
            process::exit(1);
        }
    };

    if let Some(out_path) = output {
        if let Err(e) = std::fs::write(&out_path, &markdown) {
            eprintln!("error: cannot write '{}': {}", out_path.display(), e);
            process::exit(1);
        }
        eprintln!("Documentation written to {}", out_path.display());
    } else {
        print!("{}", markdown);
    }
}
