use std::path::{Path, PathBuf};
use std::process;

use super::{find_program_source, load_dep_dirs, resolve_options};

#[allow(clippy::too_many_arguments)]
pub fn cmd_build(
    input: PathBuf,
    output: Option<PathBuf>,
    costs: bool,
    hotspots: bool,
    hints: bool,
    annotate: bool,
    save_costs: Option<PathBuf>,
    compare: Option<PathBuf>,
    target: &str,
    profile: &str,
) {
    let (tasm, default_output) = if input.is_dir() {
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
        let mut options = resolve_options(target, profile, Some(&project));
        options.dep_dirs = load_dep_dirs(&project);
        let tasm = match trident::compile_project_with_options(&project.entry, &options) {
            Ok(t) => t,
            Err(_) => process::exit(1),
        };
        let out = input.join(format!("{}.tasm", project.name));
        (tasm, out)
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
            let mut options = resolve_options(target, profile, Some(&project));
            options.dep_dirs = load_dep_dirs(&project);
            let tasm = match trident::compile_project_with_options(&project.entry, &options) {
                Ok(t) => t,
                Err(_) => process::exit(1),
            };
            let out = project.root_dir.join(format!("{}.tasm", project.name));
            (tasm, out)
        } else {
            let options = resolve_options(target, profile, None);
            let tasm = match trident::compile_project_with_options(&input, &options) {
                Ok(t) => t,
                Err(_) => process::exit(1),
            };
            let out = input.with_extension("tasm");
            (tasm, out)
        }
    } else {
        eprintln!("error: input must be a .tri file or project directory");
        process::exit(1);
    };

    let out_path = output.unwrap_or(default_output);
    if let Err(e) = std::fs::write(&out_path, &tasm) {
        eprintln!("error: cannot write '{}': {}", out_path.display(), e);
        process::exit(1);
    }
    eprintln!("Compiled -> {}", out_path.display());

    // --annotate: print per-line cost annotations
    if annotate {
        if let Some(source_path) = find_program_source(&input) {
            let source = std::fs::read_to_string(&source_path).unwrap_or_default();
            let filename = source_path.to_string_lossy().to_string();
            match trident::annotate_source(&source, &filename) {
                Ok(annotated) => {
                    println!("{}", annotated);
                }
                Err(_) => {
                    eprintln!("error: could not annotate source (compilation errors)");
                }
            }
        }
    }

    // Cost analysis, hotspots, and optimization hints
    if costs || hotspots || hints || save_costs.is_some() || compare.is_some() {
        if let Some(source_path) = find_program_source(&input) {
            let options = resolve_options(target, profile, None);
            if let Ok(program_cost) = trident::analyze_costs_project(&source_path, &options) {
                if costs || hotspots {
                    eprintln!("\n{}", program_cost.format_report());
                    if hotspots {
                        eprintln!("{}", program_cost.format_hotspots(5));
                    }
                }
                if hints {
                    let opt_hints = program_cost.optimization_hints();
                    let boundary = program_cost.boundary_warnings();
                    let all_hints: Vec<_> = opt_hints.into_iter().chain(boundary).collect();
                    if all_hints.is_empty() {
                        eprintln!("\nNo optimization hints.");
                    } else {
                        eprintln!("\nOptimization hints:");
                        for hint in &all_hints {
                            eprintln!("  {}", hint.message);
                            for note in &hint.notes {
                                eprintln!("    note: {}", note);
                            }
                            if let Some(help) = &hint.help {
                                eprintln!("    help: {}", help);
                            }
                        }
                    }
                }

                // --save-costs: write cost JSON to file
                if let Some(ref save_path) = save_costs {
                    if let Err(e) = program_cost.save_json(save_path) {
                        eprintln!("error: {}", e);
                        process::exit(1);
                    }
                    eprintln!("Saved costs -> {}", save_path.display());
                }

                // --compare: load previous costs and show diff
                if let Some(ref compare_path) = compare {
                    match trident::cost::ProgramCost::load_json(compare_path) {
                        Ok(old_cost) => {
                            eprintln!("\n{}", old_cost.format_comparison(&program_cost));
                        }
                        Err(e) => {
                            eprintln!("error: {}", e);
                            process::exit(1);
                        }
                    }
                }
            }
        }
    }
}
