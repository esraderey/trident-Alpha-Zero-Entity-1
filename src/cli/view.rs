use std::path::PathBuf;
use std::process;

pub fn cmd_view(name: String, input: Option<PathBuf>, full: bool) {
    // Resolve the source file to parse
    let source_path = if let Some(ref path) = input {
        if path.is_dir() {
            let toml_path = path.join("trident.toml");
            if !toml_path.exists() {
                eprintln!("error: no trident.toml found in '{}'", path.display());
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
        } else if path.extension().is_some_and(|e| e == "tri") {
            path.clone()
        } else {
            eprintln!("error: input must be a .tri file or project directory");
            process::exit(1);
        }
    } else {
        // Try current directory for trident.toml, then look for .tri files
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let toml_path = cwd.join("trident.toml");
        if toml_path.exists() {
            let project = match trident::project::Project::load(&toml_path) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("error: {}", e.message);
                    process::exit(1);
                }
            };
            project.entry
        } else {
            let main_tri = cwd.join("main.tri");
            if main_tri.exists() {
                main_tri
            } else {
                eprintln!("error: no trident.toml or main.tri found in current directory");
                eprintln!("  use --input to specify a .tri file or project directory");
                process::exit(1);
            }
        }
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

    let fn_hashes = trident::hash::hash_file(&file);

    // Try to find the function: by hash prefix or by name
    let (fn_name, func) = if trident::view::looks_like_hash(&name) {
        // Try hash prefix first, fall back to name lookup
        if let Some((found_name, found_func)) =
            trident::view::find_function_by_hash(&file, &fn_hashes, &name)
        {
            (found_name, found_func.clone())
        } else if let Some(found_func) = trident::view::find_function(&file, &name) {
            (name.clone(), found_func.clone())
        } else {
            eprintln!("error: no function matching '{}' found", name);
            process::exit(1);
        }
    } else if let Some(found_func) = trident::view::find_function(&file, &name) {
        (name.clone(), found_func.clone())
    } else {
        eprintln!("error: function '{}' not found in '{}'", name, filename);
        eprintln!("\nAvailable functions:");
        for item in &file.items {
            if let trident::ast::Item::Fn(f) = &item.node {
                if let Some(hash) = fn_hashes.get(&f.name.node) {
                    eprintln!("  {}  {}", hash, f.name.node);
                }
            }
        }
        process::exit(1);
    };

    // Pretty-print the function
    let formatted = trident::view::format_function(&func);

    // Show hash
    if let Some(hash) = fn_hashes.get(&fn_name) {
        if full {
            eprintln!("Hash: {}", hash.to_hex());
        } else {
            eprintln!("Hash: {}", hash);
        }
    }

    print!("{}", formatted);
}
