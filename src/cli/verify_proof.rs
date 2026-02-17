use std::path::PathBuf;

use clap::Args;

#[derive(Args)]
pub struct VerifyProofArgs {
    /// Path to the proof file
    pub proof: PathBuf,
    /// Target VM or OS (default: triton)
    #[arg(long, default_value = "triton")]
    pub target: String,
}

pub fn cmd_verify_proof(args: VerifyProofArgs) {
    let target = &args.target;

    if let Some(hero_bin) = super::find_hero(target) {
        let extra = [
            args.proof.display().to_string(),
            "--target".to_string(),
            args.target.clone(),
        ];
        let refs: Vec<&str> = extra.iter().map(|s| s.as_str()).collect();
        super::delegate_to_hero(&hero_bin, "verify-proof", &refs);
        return;
    }

    eprintln!("No verification hero found for target '{}'.", target);
    eprintln!("Heroes handle proof verification using target-specific verifiers.");
    eprintln!();
    eprintln!("Install a hero for this target:");
    eprintln!("  cargo install trident-trisha   # Triton VM + Neptune");
}
