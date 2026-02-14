use std::path::PathBuf;
use std::process;

pub fn cmd_generate(input: PathBuf, output: Option<PathBuf>) {
    if !input.extension().is_some_and(|e| e == "tri") {
        eprintln!("error: input must be a .tri file");
        process::exit(1);
    }

    let source = match std::fs::read_to_string(&input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read '{}': {}", input.display(), e);
            process::exit(1);
        }
    };

    let filename = input.to_string_lossy().to_string();
    let file = match trident::parse_source_silent(&source, &filename) {
        Ok(f) => f,
        Err(errors) => {
            trident::diagnostic::render_diagnostics(&errors, &filename, &source);
            eprintln!("error: parse errors in '{}'", input.display());
            process::exit(1);
        }
    };

    let scaffold = trident::scaffold::generate_scaffold(&file);

    if let Some(out_path) = output {
        if let Err(e) = std::fs::write(&out_path, &scaffold) {
            eprintln!("error: cannot write '{}': {}", out_path.display(), e);
            process::exit(1);
        }
        eprintln!("Generated scaffold -> {}", out_path.display());
    } else {
        print!("{}", scaffold);
    }
}
