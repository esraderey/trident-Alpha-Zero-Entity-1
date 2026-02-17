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

    if let Some(warrior_bin) = super::find_warrior(target) {
        let extra = [
            args.proof.display().to_string(),
            "--target".to_string(),
            args.target.clone(),
        ];
        let refs: Vec<&str> = extra.iter().map(|s| s.as_str()).collect();
        super::delegate_to_warrior(&warrior_bin, "verify-proof", &refs);
        return;
    }

    eprintln!("No verification warrior found for target '{}'.", target);
    eprintln!("Warriors handle proof verification using target-specific verifiers.");
    eprintln!();
    eprintln!("Install a warrior for this target:");
    eprintln!("  cargo install trident-trisha   # Triton VM + Neptune");
}
