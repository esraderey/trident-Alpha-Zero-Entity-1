use std::path::PathBuf;
use std::process;

use super::{resolve_input, resolve_options};

pub fn cmd_doc(input: PathBuf, output: Option<PathBuf>, target: &str, profile: &str) {
    let ri = resolve_input(&input);

    let options = resolve_options(target, profile, ri.project.as_ref());
    let markdown = match trident::generate_docs(&ri.entry, &options) {
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
