use std::path::{Path, PathBuf};
use std::process;

use clap::Args;

use super::prepare_artifact;

#[derive(Args)]
pub struct PackageArgs {
    /// Input .tri file or directory with trident.toml
    pub input: PathBuf,
    /// Output directory for the .deploy/ artifact (default: project root or cwd)
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    /// Target VM or OS (default: triton)
    #[arg(long, default_value = "triton")]
    pub target: String,
    /// Compilation profile for cfg flags (default: release)
    #[arg(long, default_value = "release")]
    pub profile: String,
    /// Run formal audit before packaging
    #[arg(long)]
    pub audit: bool,
    /// Show what would be produced without writing files
    #[arg(long)]
    pub dry_run: bool,
}

pub fn cmd_package(args: PackageArgs) {
    let PackageArgs {
        input,
        output,
        target,
        profile,
        audit,
        dry_run,
    } = args;
    let art = prepare_artifact(&input, &target, &profile, audit);

    // Determine output base directory
    let output_base = output.unwrap_or_else(|| {
        if let Some(ref proj) = art.project {
            proj.root_dir.clone()
        } else {
            art.entry.parent().unwrap_or(Path::new(".")).to_path_buf()
        }
    });

    // Target display string
    let target_display = if let Some(ref os) = art.resolved.os {
        format!("{} ({})", os.name, art.resolved.vm.name)
    } else {
        art.resolved.vm.name.clone()
    };

    if dry_run {
        let program_digest =
            trident::hash::ContentHash(trident::poseidon2::hash_bytes(art.tasm.as_bytes()));
        eprintln!("Dry run — would package:");
        eprintln!("  Name:            {}", art.name);
        eprintln!("  Version:         {}", art.version);
        eprintln!("  Target:          {}", target_display);
        eprintln!("  Program digest:  {}", program_digest.to_hex());
        eprintln!("  Padded height:   {}", art.cost.padded_height);
        eprintln!(
            "  Artifact:        {}/{}.deploy/",
            output_base.display(),
            art.name
        );
        return;
    }

    // Generate artifact
    let result = match trident::deploy::generate_artifact(
        &art.name,
        &art.version,
        &art.tasm,
        &art.file,
        &art.cost,
        &art.resolved.vm,
        art.resolved.os.as_ref(),
        &output_base,
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {}", e);
            process::exit(1);
        }
    };

    eprintln!("Packaged -> {}", result.artifact_dir.display());
    eprintln!("  program.tasm:   {}", result.tasm_path.display());
    eprintln!("  manifest.json:  {}", result.manifest_path.display());
    eprintln!("  digest:         {}", result.manifest.program_digest);
    eprintln!("  padded height:  {}", result.manifest.cost.padded_height);
    eprintln!("  target:         {}", target_display);
}
