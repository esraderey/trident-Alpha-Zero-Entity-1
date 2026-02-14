use std::path::PathBuf;
use std::process;

use clap::Args;

use super::{resolve_input, resolve_options};

#[derive(Args)]
pub struct DocArgs {
    /// Input .tri file or directory with trident.toml
    pub input: PathBuf,
    /// Output markdown file (default: stdout)
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    /// Target VM (default: triton)
    #[arg(long, default_value = "triton")]
    pub target: String,
    /// Compilation profile for cfg flags (debug or release)
    #[arg(long, default_value = "debug")]
    pub profile: String,
}

pub fn cmd_doc(args: DocArgs) {
    let DocArgs {
        input,
        output,
        target,
        profile,
    } = args;
    let ri = resolve_input(&input);

    let options = resolve_options(&target, &profile, ri.project.as_ref());
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
