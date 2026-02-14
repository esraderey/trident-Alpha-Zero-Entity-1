use std::path::PathBuf;
use std::process;

pub fn cmd_hash(input: PathBuf, full: bool) {
    let source_path = if input.is_dir() {
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
        input.clone()
    } else {
        eprintln!("error: input must be a .tri file or project directory");
        process::exit(1);
    };

    let source = match std::fs::read_to_string(&source_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read '{}': {}", source_path.display(), e);
            process::exit(1);
        }
    };

    let filename = source_path.to_string_lossy().to_string();
    let file = match trident::parse_source_silent(&source, &filename) {
        Ok(f) => f,
        Err(_) => {
            eprintln!("error: parse errors in '{}'", source_path.display());
            process::exit(1);
        }
    };

    // Hash all functions
    let fn_hashes = trident::hash::hash_file(&file);
    let file_hash = trident::hash::hash_file_content(&file);

    // Print file hash
    if full {
        eprintln!("File: {} {}", file_hash.to_hex(), source_path.display());
    } else {
        eprintln!("File: {} {}", file_hash, source_path.display());
    }

    // Print function hashes in sorted order
    let mut sorted: Vec<_> = fn_hashes.iter().collect();
    sorted.sort_by_key(|(name, _)| (*name).clone());
    for (name, hash) in sorted {
        if full {
            println!("  {} {}", hash.to_hex(), name);
        } else {
            println!("  {} {}", hash, name);
        }
    }
}
