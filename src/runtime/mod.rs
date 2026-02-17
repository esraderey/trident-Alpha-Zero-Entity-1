//! Runtime traits for VM execution, proving, and deployment.
//!
//! Trident defines these traits; heroes implement them. A hero is a
//! target-specific tool (separate crate) that takes compiled Trident
//! output and handles execution, proving, and deployment for a
//! particular VM+OS combination.
//!
//! Example: **Trisha** (Triton + Neptune) implements `Runner` via
//! `triton_vm::simulate()`, `Prover` via `triton_vm::prove()`, and
//! `Deployer` via Neptune RPC — all using Trident's `PrimeField`,
//! `Poseidon2`, and `Claim` primitives from `crate::field`.
//!
//! No heavy dependencies here — only the interface contract and
//! the serializable `ProgramBundle` artifact format.

pub mod artifact;

use crate::field::proof::Claim;
pub use artifact::ProgramBundle;

// ─── Types ─────────────────────────────────────────────────────────

/// VM execution result.
#[derive(Clone, Debug)]
pub struct ExecutionResult {
    /// Output field elements (from `write_io` instructions).
    pub output: Vec<u64>,
    /// Number of VM cycles consumed.
    pub cycle_count: u64,
}

/// Proof artifact: claim + opaque proof bytes.
///
/// The `claim` is universal (defined in `field::proof`). The
/// `proof_bytes` are hero-specific — their format depends on
/// the proving system (STARK, SNARK, etc.).
#[derive(Clone, Debug)]
pub struct ProofData {
    /// What the proof asserts (program hash, input, output).
    pub claim: Claim,
    /// Serialized proof (format is hero-specific).
    pub proof_bytes: Vec<u8>,
    /// Proof system identifier (e.g. "stark-triton-v2", "groth16-bn254").
    pub format: String,
}

/// Input specification for program execution.
#[derive(Clone, Debug, Default)]
pub struct ProgramInput {
    /// Public input field elements (read via `read_io`).
    pub public: Vec<u64>,
    /// Secret/divine input field elements (read via `divine`).
    pub secret: Vec<u64>,
}

// ─── Hero Traits ───────────────────────────────────────────────────

/// Run a compiled program on a VM.
///
/// Heroes implement this to execute TASM (or other target assembly)
/// using the actual VM runtime.
pub trait Runner {
    /// Execute the program with the given inputs, returning output
    /// values and cycle count.
    fn run(&self, bundle: &ProgramBundle, input: &ProgramInput) -> Result<ExecutionResult, String>;
}

/// Generate a proof of correct execution.
///
/// Heroes implement this to produce a cryptographic proof that the
/// program executed correctly on the given inputs.
pub trait Prover {
    /// Execute and prove, returning the proof artifact.
    fn prove(&self, bundle: &ProgramBundle, input: &ProgramInput) -> Result<ProofData, String>;
}

/// Verify a proof against its claim.
///
/// Heroes implement this to check that a proof is valid for its
/// claimed program, input, and output.
pub trait Verifier {
    /// Returns true if the proof is valid.
    fn verify(&self, proof: &ProofData) -> Result<bool, String>;
}

/// Deploy a program to a chain or runtime.
///
/// Heroes implement this for chain-specific deployment (e.g.,
/// constructing Neptune LockScript/TypeScript, broadcasting
/// transactions via RPC).
pub trait Deployer {
    /// Deploy the program, optionally with a proof.
    /// Returns a deployment identifier (tx hash, contract address, etc.).
    fn deploy(&self, bundle: &ProgramBundle, proof: Option<&ProofData>) -> Result<String, String>;
}
