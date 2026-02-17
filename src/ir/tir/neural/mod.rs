//! Neural TIR→TASM optimizer.
//!
//! A 91K-parameter encoder-decoder model that generates TASM from TIR blocks.
//! All arithmetic in fixed-point Goldilocks field. Trained by evolutionary
//! search. Verified by semantic equivalence checking. Strictly speculative —
//! classical lowering always runs as fallback.

pub mod evolve;
pub mod model;
pub mod report;
pub mod weights;
