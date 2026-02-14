use std::path::PathBuf;
use std::process;

use clap::Args;

use super::{resolve_input, resolve_options};

#[derive(Args)]
pub struct TestArgs {
    /// Input .tri file or directory with trident.toml
    pub input: PathBuf,
    /// Target VM (default: triton)
    #[arg(long, default_value = "triton")]
    pub target: String,
    /// Compilation profile for cfg flags (debug or release)
    #[arg(long, default_value = "debug")]
    pub profile: String,
}

pub fn cmd_test(args: TestArgs) {
    let TestArgs {
        input,
        target,
        profile,
    } = args;
    let ri = resolve_input(&input);

    let options = resolve_options(&target, &profile, ri.project.as_ref());
    let result = trident::run_tests(&ri.entry, &options);

    match result {
        Ok(report) => {
            eprintln!("{}", report);
        }
        Err(_) => {
            process::exit(1);
        }
    }
}
