pub(crate) use std::collections::{HashMap, HashSet};
pub(crate) use std::path::{Path, PathBuf};

pub(crate) use crate::diagnostic::Diagnostic;
pub(crate) use crate::span::Span;

/// Information about a discovered module.
#[derive(Clone, Debug)]
pub(crate) struct ModuleInfo {
    /// Dotted module name (e.g. "crypto.sponge").
    pub(crate) name: String,
    /// Filesystem path to the .tri file.
    pub(crate) file_path: PathBuf,
    /// Source code.
    pub(crate) source: String,
    /// Modules this module depends on (from `use` statements).
    pub(crate) dependencies: Vec<String>,
}

/// Resolve all modules reachable from an entry point.
/// Returns modules in topological order (dependencies first).

mod resolver;
use resolver::*;

pub(crate) fn resolve_modules(entry_path: &Path) -> Result<Vec<ModuleInfo>, Vec<Diagnostic>> {
    let mut resolver = ModuleResolver::new(entry_path)?;
    resolver.discover_all()?;
    resolver.topological_sort()
}

/// Resolve modules with additional dependency search directories.
/// Used when a project has locked dependencies cached on disk.
pub(crate) fn resolve_modules_with_deps(
    entry_path: &Path,
    dep_dirs: Vec<PathBuf>,
) -> Result<Vec<ModuleInfo>, Vec<Diagnostic>> {
    let mut resolver = ModuleResolver::new(entry_path)?;
    resolver.dep_dirs = dep_dirs;
    resolver.discover_all()?;
    resolver.topological_sort()
}

/// Find the standard library directory.
/// Search order:
///   1. TRIDENT_STDLIB environment variable
///   2. `std/` relative to the compiler binary
///   3. `std/` in the repository root (development)
pub(crate) fn find_stdlib_dir() -> Option<PathBuf> {
    // 1. Environment variable
    if let Ok(p) = std::env::var("TRIDENT_STDLIB") {
        let path = PathBuf::from(p);
        if path.is_dir() {
            return Some(path);
        }
    }

    // 2. Relative to the compiler binary
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let path = dir.join("std");
            if path.is_dir() {
                return Some(path);
            }
            // Also check one level up (e.g. target/debug/../std)
            if let Some(parent) = dir.parent() {
                let path = parent.join("std");
                if path.is_dir() {
                    return Some(path);
                }
                // Two levels up for target/debug/../../std
                if let Some(grandparent) = parent.parent() {
                    let path = grandparent.join("std");
                    if path.is_dir() {
                        return Some(path);
                    }
                }
            }
        }
    }

    // 3. Current working directory
    let cwd_std = PathBuf::from("std");
    if cwd_std.is_dir() {
        return Some(cwd_std);
    }

    None
}

/// Find the OS library directory.
/// OS-specific extension code lives in `os/<os_name>/`.
/// Search order mirrors `find_stdlib_dir` but looks for `os/`.
///   1. TRIDENT_OSLIB environment variable (or legacy TRIDENT_EXTLIB)
///   2. `os/` relative to the compiler binary (and ancestors)
///   3. `os/` in the current working directory (development)
pub(crate) fn find_os_dir() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("TRIDENT_OSLIB") {
        let path = PathBuf::from(p);
        if path.is_dir() {
            return Some(path);
        }
    }
    // Legacy env var
    if let Ok(p) = std::env::var("TRIDENT_EXTLIB") {
        let path = PathBuf::from(p);
        if path.is_dir() {
            return Some(path);
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let path = dir.join("os");
            if path.is_dir() {
                return Some(path);
            }
            if let Some(parent) = dir.parent() {
                let path = parent.join("os");
                if path.is_dir() {
                    return Some(path);
                }
                if let Some(grandparent) = parent.parent() {
                    let path = grandparent.join("os");
                    if path.is_dir() {
                        return Some(path);
                    }
                }
            }
        }
    }

    let cwd_os = PathBuf::from("os");
    if cwd_os.is_dir() {
        return Some(cwd_os);
    }

    None
}

/// Legacy flat-path fallback map for backward compatibility.
/// Maps old module names to their new layered locations.
fn legacy_stdlib_fallback(name: &str) -> Option<&'static str> {
    match name {
        // Legacy flat std.* → vm.* or std.* (final destination)
        "std.assert" => Some("vm.core.assert"),
        "std.convert" => Some("vm.core.convert"),
        "std.field" => Some("vm.core.field"),
        "std.u32" => Some("vm.core.u32"),
        "std.io" => Some("vm.io.io"),
        "std.mem" => Some("vm.io.mem"),
        "std.storage" => Some("std.io.storage"),
        "std.hash" => Some("vm.crypto.hash"),
        "std.merkle" => Some("std.crypto.merkle"),
        "std.auth" => Some("std.crypto.auth"),
        // Legacy std.* intrinsics → vm.* (intrinsics moved out of std)
        "std.core.field" => Some("vm.core.field"),
        "std.core.convert" => Some("vm.core.convert"),
        "std.core.u32" => Some("vm.core.u32"),
        "std.core.assert" => Some("vm.core.assert"),
        "std.io.io" => Some("vm.io.io"),
        "std.io.mem" => Some("vm.io.mem"),
        "std.crypto.hash" => Some("vm.crypto.hash"),
        // Legacy std.xfield/kernel/utxo → os.neptune.*
        "std.xfield" => Some("os.neptune.xfield"),
        "std.kernel" => Some("os.neptune.kernel"),
        "std.utxo" => Some("os.neptune.utxo"),
        // Backward compatibility: ext.triton.* → os.neptune.*
        "ext.triton.xfield" => Some("os.neptune.xfield"),
        "ext.triton.kernel" => Some("os.neptune.kernel"),
        "ext.triton.utxo" => Some("os.neptune.utxo"),
        "ext.triton.proof" => Some("os.neptune.proof"),
        "ext.triton.recursive" => Some("os.neptune.recursive"),

        // Backward compatibility: <os>.ext.* → os.<os>.*
        "neptune.ext.kernel" => Some("os.neptune.kernel"),
        "neptune.ext.utxo" => Some("os.neptune.utxo"),
        "neptune.ext.xfield" => Some("os.neptune.xfield"),
        "neptune.ext.proof" => Some("os.neptune.proof"),
        "neptune.ext.recursive" => Some("os.neptune.recursive"),

        // Backward compatibility: ext.<os>.* → os.<os>.*
        _ if name.starts_with("ext.") => {
            None // handled by resolve_path directly
        }
        _ => None,
    }
}


#[cfg(test)]
mod tests;
