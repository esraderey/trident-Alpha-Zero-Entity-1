use std::path::PathBuf;
use std::process;

use clap::Args;

use super::resolve_input;

#[derive(Args)]
pub struct RunArgs {
    /// Input .tri file or directory with trident.toml
    pub input: PathBuf,
    /// Target VM or OS (default: triton)
    #[arg(long, default_value = "triton")]
    pub target: String,
    /// Compilation profile (debug or release)
    #[arg(long, default_value = "debug")]
    pub profile: String,
    /// Public input values (comma-separated field elements)
    #[arg(long, value_delimiter = ',')]
    pub input_values: Option<Vec<u64>>,
    /// Secret/divine input values (comma-separated field elements)
    #[arg(long, value_delimiter = ',')]
    pub secret: Option<Vec<u64>>,
}

pub fn cmd_run(args: RunArgs) {
    let ri = resolve_input(&args.input);
    let target = &args.target;

    if let Some(warrior_bin) = super::find_warrior(target) {
        let mut extra: Vec<String> = vec![
            args.input.display().to_string(),
            "--target".to_string(),
            args.target.clone(),
            "--profile".to_string(),
            args.profile.clone(),
        ];
        if let Some(ref vals) = args.input_values {
            extra.push("--input-values".to_string());
            let s: Vec<String> = vals.iter().map(|v| v.to_string()).collect();
            extra.push(s.join(","));
        }
        if let Some(ref vals) = args.secret {
            extra.push("--secret".to_string());
            let s: Vec<String> = vals.iter().map(|v| v.to_string()).collect();
            extra.push(s.join(","));
        }
        let refs: Vec<&str> = extra.iter().map(|s| s.as_str()).collect();
        super::delegate_to_warrior(&warrior_bin, "run", &refs);
        return;
    }

    let options = super::resolve_options(target, &args.profile, ri.project.as_ref());
    match trident::compile_to_bundle(&ri.entry, &options) {
        Ok(bundle) => {
            let op_count = bundle.assembly.lines().count();
            eprintln!("Compiled {} ({} ops)", bundle.name, op_count);
            eprintln!();
            eprintln!("No runtime warrior found for target '{}'.", target);
            eprintln!("Warriors handle execution, proving, and deployment.");
            eprintln!();
            eprintln!("Install a warrior for this target:");
            eprintln!("  cargo install trident-trisha   # Triton VM + Neptune");
            eprintln!();
            eprintln!("Or use 'trident build' to produce TASM output directly.");
        }
        Err(_) => {
            eprintln!("error: compilation failed");
            process::exit(1);
        }
    }
}
