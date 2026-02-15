//! Content-addressed package manager for Trident.
//!
//! Parses `[dependencies]` from `trident.toml`, manages a lockfile
//! (`trident.lock`), and caches dependency sources under `.trident/deps/`.
//!
//! Three dependency kinds:
//!   - **Hash** — pinned by a 64-hex-char BLAKE3 content hash.
//!   - **Registry** — resolved via a `RegistryClient` by name.
//!   - **Path** — local filesystem, re-read every build.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::registry::{PullResult, RegistryClient};

// ─── Data Types ────────────────────────────────────────────────────

/// A declared dependency in trident.toml.
#[derive(Clone, Debug)]
pub enum Dependency {
    /// Pinned by content hash (64 hex chars).
    Hash { hash: String },
    /// Resolved via a registry by name.
    Registry { name: String, registry: String },
    /// Local filesystem path.
    Path { path: PathBuf },
}

/// A resolved (locked) dependency.
#[derive(Clone, Debug)]
pub struct LockedDep {
    pub name: String,
    pub hash: String,
    pub source: String, // "registry:<url>", "path:<relative>", "hash"
}

/// Package manifest: parsed `[dependencies]` from trident.toml.
#[derive(Clone, Debug, Default)]
pub struct Manifest {
    pub dependencies: HashMap<String, Dependency>,
}

/// Lock file contents.
#[derive(Clone, Debug, Default)]
pub struct Lockfile {
    pub locked: HashMap<String, LockedDep>,
}

mod parse;
mod lockfile;
mod resolve;

pub use parse::parse_dependencies;
pub use lockfile::{load_lockfile, save_lockfile};
pub use resolve::{resolve_dependencies, dep_source_path, dependency_search_paths};

#[cfg(test)]
mod tests;
