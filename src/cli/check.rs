use std::path::PathBuf;
use std::process;

use clap::Args;

use super::{find_program_source, resolve_input};

#[derive(Args)]
pub struct CheckArgs {
    /// Input .tri file or directory with trident.toml
    pub input: PathBuf,
    /// Print cost analysis report
    #[arg(long)]
    pub costs: bool,
    /// Target VM (default: triton)
    #[arg(long, default_value = "triton")]
    pub target: String,
    /// Compilation profile for cfg flags (debug or release)
    #[arg(long, default_value = "debug")]
    pub profile: String,
}

pub fn cmd_check(args: CheckArgs) {
    let CheckArgs { input, costs, .. } = args;
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
