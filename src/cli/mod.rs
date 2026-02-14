pub mod bench;
pub mod build;
pub mod check;
pub mod deploy;
pub mod deps;
pub mod doc;
pub mod fmt;
pub mod generate;
pub mod hash;
pub mod init;
pub mod package;
pub mod registry;
pub mod test;
pub mod ucm;
pub mod verify;
pub mod view;

use std::path::{Path, PathBuf};
use std::process;

/// Resolved input: entry file and optional project.
#[allow(dead_code)]
pub struct ResolvedInput {
    pub entry: PathBuf,
    pub project: Option<trident::project::Project>,
}

/// Resolve an input path (file or project directory) to an entry file and optional project.
///
/// This is the common "is-it-a-dir? find-toml? load-project?" boilerplate.
#[allow(dead_code)]
pub fn resolve_input(input: &Path) -> ResolvedInput {
    if input.is_dir() {
        let toml_path = input.join("trident.toml");
        if !toml_path.exists() {
            eprintln!("error: no trident.toml found in '{}'", input.display());
            process::exit(1);
        }
        let project = match trident::project::Project::load(&toml_path) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("error: {}", e.message);
                process::exit(1);
            }
        };
        let entry = project.entry.clone();
        ResolvedInput {
            entry,
            project: Some(project),
        }
    } else if input.extension().is_some_and(|e| e == "tri") {
        if let Some(toml_path) =
            trident::project::Project::find(input.parent().unwrap_or(Path::new(".")))
        {
            let project = match trident::project::Project::load(&toml_path) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("error: {}", e.message);
                    process::exit(1);
                }
            };
            let entry = project.entry.clone();
            ResolvedInput {
                entry,
                project: Some(project),
            }
        } else {
            ResolvedInput {
                entry: input.to_path_buf(),
                project: None,
            }
        }
    } else {
        eprintln!("error: input must be a .tri file or project directory");
        process::exit(1);
    }
}

/// Resolve a VM target + profile to CompileOptions.
///
/// - `target`: VM target name (e.g. "triton"). For backward compat, if
///   "debug" or "release" is passed as target, treat it as profile with a
///   deprecation warning.
/// - `profile`: compilation profile for cfg flags (e.g. "debug", "release").
pub fn resolve_options(
    target: &str,
    profile: &str,
    project: Option<&trident::project::Project>,
) -> trident::CompileOptions {
    // Backward compatibility: if --target was "debug" or "release", the user
    // is using the old semantics where --target meant profile.
    let (vm_target, actual_profile) = match target {
        "debug" | "release" => {
            eprintln!(
                "warning: --target {} is deprecated for profile selection; use --profile {} --target triton",
                target, target
            );
            ("triton", target)
        }
        _ => (target, profile),
    };

    // Use project's target if CLI target is the default and project specifies one
    let effective_target = if vm_target == "triton" {
        if let Some(proj) = project {
            if let Some(ref proj_target) = proj.target {
                proj_target.as_str()
            } else {
                vm_target
            }
        } else {
            vm_target
        }
    } else {
        vm_target
    };

    // Resolve the VM target config
    let target_config = if effective_target == "triton" {
        trident::target::TargetConfig::triton()
    } else {
        match trident::target::TargetConfig::resolve(effective_target) {
            Ok(config) => config,
            Err(e) => {
                eprintln!("error: {}", e.message);
                process::exit(1);
            }
        }
    };

    // Resolve cfg flags from project targets or default to profile name
    let cfg_flags = if let Some(proj) = project {
        // Check project [target] field first
        // Then check project [targets.PROFILE] for cfg flags
        if let Some(flags) = proj.targets.get(actual_profile) {
            flags.iter().cloned().collect()
        } else {
            std::collections::HashSet::from([actual_profile.to_string()])
        }
    } else {
        std::collections::HashSet::from([actual_profile.to_string()])
    };

    trident::CompileOptions {
        profile: actual_profile.to_string(),
        cfg_flags,
        target_config,
        dep_dirs: Vec::new(),
    }
}

/// Load dependency search directories from a project's lockfile (if present).
pub fn load_dep_dirs(project: &trident::project::Project) -> Vec<PathBuf> {
    let lock_path = project.root_dir.join("trident.lock");
    if !lock_path.exists() {
        return Vec::new();
    }
    match trident::manifest::load_lockfile(&lock_path) {
        Ok(lockfile) => trident::manifest::dependency_search_paths(&project.root_dir, &lockfile),
        Err(_) => Vec::new(),
    }
}

pub fn find_program_source(input: &Path) -> Option<PathBuf> {
    if input.is_file() && input.extension().is_some_and(|e| e == "tri") {
        return Some(input.to_path_buf());
    }
    if input.is_dir() {
        let main_tri = input.join("main.tri");
        if main_tri.exists() {
            return Some(main_tri);
        }
    }
    None
}

/// Recursively collect all .tri files in a directory, skipping hidden dirs and target/.
pub fn collect_tri_files(dir: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    collect_tri_files_recursive(dir, &mut result);
    result.sort();
    result
}

pub fn collect_tri_files_recursive(dir: &Path, result: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip hidden directories and target/
        if name_str.starts_with('.') || name_str == "target" {
            continue;
        }

        if path.is_dir() {
            collect_tri_files_recursive(&path, result);
        } else if path.extension().is_some_and(|e| e == "tri") {
            result.push(path);
        }
    }
}
