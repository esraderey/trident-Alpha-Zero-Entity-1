use std::path::PathBuf;
use std::process;

use clap::Args;

use super::resolve_input;

#[derive(Args)]
pub struct ProveArgs {
    /// Input .tri file or directory with trident.toml
    pub input: PathBuf,
    /// Target VM or OS (default: triton)
    #[arg(long, default_value = "triton")]
    pub target: String,
    /// Compilation profile (debug or release)
    #[arg(long, default_value = "release")]
    pub profile: String,
    /// Public input values (comma-separated field elements)
    #[arg(long, value_delimiter = ',')]
    pub input_values: Option<Vec<u64>>,
    /// Secret/divine input values (comma-separated field elements)
    #[arg(long, value_delimiter = ',')]
    pub secret: Option<Vec<u64>>,
    /// Output path for the proof file
    #[arg(long)]
    pub output: Option<PathBuf>,
}

pub fn cmd_prove(args: ProveArgs) {
    let ri = resolve_input(&args.input);
    let target = &args.target;

    if let Some(hero_bin) = super::find_hero(target) {
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
        if let Some(ref out) = args.output {
            extra.push("--output".to_string());
            extra.push(out.display().to_string());
        }
        let refs: Vec<&str> = extra.iter().map(|s| s.as_str()).collect();
        super::delegate_to_hero(&hero_bin, "prove", &refs);
        return;
    }

    let options = super::resolve_options(target, &args.profile, ri.project.as_ref());
    match trident::compile_to_bundle(&ri.entry, &options) {
        Ok(bundle) => {
            let op_count = bundle.assembly.lines().count();
            eprintln!("Compiled {} ({} ops)", bundle.name, op_count);
            eprintln!();
            eprintln!("No proving hero found for target '{}'.", target);
            eprintln!("Heroes handle proof generation using target-specific provers.");
            eprintln!();
            eprintln!("Install a hero for this target:");
            eprintln!("  cargo install trident-trisha   # Triton VM + Neptune");
        }
        Err(_) => {
            eprintln!("error: compilation failed");
            process::exit(1);
        }
    }
}
