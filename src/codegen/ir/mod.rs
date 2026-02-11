//! Codegen IR — re-exports from the standalone `crate::tir` module.
//!
//! The canonical TIR definitions live in `src/tir/`. This module provides
//! backward-compatible paths for existing code.

pub mod builder;

// Re-export everything from the canonical ir module.
pub use crate::tir::lower;
pub use crate::tir::lower::{create_lowering, Lowering};
pub use crate::tir::TIROp;
