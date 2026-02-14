use std::path::PathBuf;
use std::process;

use clap::Args;

use super::{find_program_source, load_dep_dirs, resolve_input, resolve_options};

#[derive(Args)]
pub struct BuildArgs {
    /// Input .tri file or directory with trident.toml
    pub input: PathBuf,
    /// Output .tasm file (default: <input>.tasm)
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    /// Print cost analysis report
    #[arg(long)]
    pub costs: bool,
    /// Show top cost contributors (implies --costs)
    #[arg(long)]
    pub hotspots: bool,
    /// Show optimization hints (H0001-H0004)
    #[arg(long)]
    pub hints: bool,
    /// Output per-line cost annotations
    #[arg(long)]
    pub annotate: bool,
    /// Save cost analysis to a JSON file
    #[arg(long, value_name = "PATH")]
    pub save_costs: Option<PathBuf>,
    /// Compare costs with a previous cost JSON file
    #[arg(long, value_name = "PATH")]
    pub compare: Option<PathBuf>,
    /// Target VM (default: triton)
    #[arg(long, default_value = "triton")]
    pub target: String,
    /// Compilation profile for cfg flags (debug or release)
    #[arg(long, default_value = "debug")]
    pub profile: String,
}

pub fn cmd_build(args: BuildArgs) {
    let BuildArgs {
        input,
        output,
        costs,
        hotspots,
        hints,
        annotate,
        save_costs,
        compare,
        target,
        profile,
    } = args;
    let ri = resolve_input(&input);

    let mut options = resolve_options(&target, &profile, ri.project.as_ref());
    if let Some(ref proj) = ri.project {
        options.dep_dirs = load_dep_dirs(proj);
    }

    let tasm = match trident::compile_project_with_options(&ri.entry, &options) {
        Ok(t) => t,
        Err(_) => process::exit(1),
    };

    let default_output = if let Some(ref proj) = ri.project {
        proj.root_dir.join(format!("{}.tasm", proj.name))
    } else {
        input.with_extension("tasm")
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
            let cost_options = resolve_options(&target, &profile, None);
            if let Ok(program_cost) = trident::analyze_costs_project(&source_path, &cost_options) {
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
