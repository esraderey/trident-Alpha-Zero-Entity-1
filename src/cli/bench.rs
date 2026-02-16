use std::path::PathBuf;
use std::process;

use clap::Args;

#[derive(Args)]
pub struct BenchArgs {
    /// Directory containing baseline .tasm files (mirrors source tree)
    #[arg(default_value = "benches")]
    pub dir: PathBuf,
}

pub fn cmd_bench(args: BenchArgs) {
    let bench_dir = resolve_bench_dir(&args.dir);
    if !bench_dir.is_dir() {
        eprintln!("error: '{}' is not a directory", args.dir.display());
        process::exit(1);
    }

    // Find the project root (parent of benches/)
    let project_root = bench_dir
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));

    // Recursively find all .baseline.tasm files
    let mut baselines = find_baseline_files(&bench_dir);
    baselines.sort();

    if baselines.is_empty() {
        eprintln!("No .baseline.tasm files found in '{}'", bench_dir.display());
        process::exit(1);
    }

    let options = trident::CompileOptions::default();
    let mut results: Vec<trident::ModuleBenchmarkResult> = Vec::new();

    for baseline_path in &baselines {
        // Map baseline to source: benches/std/crypto/auth.baseline.tasm -> std/crypto/auth.tri
        let rel = baseline_path
            .strip_prefix(&bench_dir)
            .unwrap_or(baseline_path);
        let rel_str = rel.to_string_lossy();
        let source_rel = rel_str.replace(".baseline.tasm", ".tri");
        let source_path = project_root.join(&source_rel);
        let module_name = source_rel.trim_end_matches(".tri").replace('/', ".");

        if !source_path.exists() {
            eprintln!(
                "  SKIP  {}  (source not found: {})",
                module_name,
                source_path.display()
            );
            continue;
        }

        // Compile the module (no linking, no DCE)
        let compiled_tasm = match trident::compile_module(&source_path, &options) {
            Ok(t) => t,
            Err(_) => {
                eprintln!("  FAIL  {}  (compilation error)", module_name);
                continue;
            }
        };

        // Read baseline
        let baseline_tasm = match std::fs::read_to_string(baseline_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("  FAIL  {}  (read error: {})", module_name, e);
                continue;
            }
        };

        // Parse both into per-function instruction maps
        let compiled_fns = trident::parse_tasm_functions(&compiled_tasm);
        let baseline_fns = trident::parse_tasm_functions(&baseline_tasm);

        // Compare: only functions present in the baseline
        let mut fn_results: Vec<trident::FunctionBenchmark> = Vec::new();
        let mut total_compiled: usize = 0;
        let mut total_baseline: usize = 0;

        for (name, &baseline_count) in &baseline_fns {
            let compiled_count = compiled_fns.get(name).copied().unwrap_or(0);
            let ratio = if baseline_count > 0 {
                compiled_count as f64 / baseline_count as f64
            } else {
                0.0
            };
            total_compiled += compiled_count;
            total_baseline += baseline_count;
            fn_results.push(trident::FunctionBenchmark {
                name: name.clone(),
                compiled_instructions: compiled_count,
                baseline_instructions: baseline_count,
                overhead_ratio: ratio,
            });
        }

        let overall_ratio = if total_baseline > 0 {
            total_compiled as f64 / total_baseline as f64
        } else {
            0.0
        };

        results.push(trident::ModuleBenchmarkResult {
            module_path: module_name,
            functions: fn_results,
            total_compiled,
            total_baseline,
            overall_ratio,
        });
    }

    if results.is_empty() {
        eprintln!("No benchmarks could be compiled.");
        process::exit(1);
    }

    // Print results table
    eprintln!();
    eprintln!("{}", trident::ModuleBenchmarkResult::format_header());
    for (i, result) in results.iter().enumerate() {
        if i > 0 {
            eprintln!("{}", result.format_module_header());
        } else {
            // First module: print header row directly
            eprintln!(
                "\u{2502} {:<28} \u{2502} {:>8} \u{2502} {:>8} \u{2502} {:>7} \u{2502} {} \u{2502}",
                result.module_path,
                fmt_num(result.total_compiled),
                fmt_num(result.total_baseline),
                fmt_ratio(result.overall_ratio),
                status_icon(result.overall_ratio),
            );
        }
        for f in &result.functions {
            eprintln!("{}", result.format_function(f));
        }
    }
    eprintln!("{}", trident::ModuleBenchmarkResult::format_separator());

    // Summary
    if !results.is_empty() {
        let avg_ratio: f64 =
            results.iter().map(|r| r.overall_ratio).sum::<f64>() / results.len() as f64;
        let max_ratio = results
            .iter()
            .map(|r| r.overall_ratio)
            .fold(0.0f64, f64::max);
        eprintln!(
            "{}",
            trident::ModuleBenchmarkResult::format_summary(avg_ratio, max_ratio, results.len())
        );
    }
    eprintln!();
}

fn fmt_num(n: usize) -> String {
    if n == 0 {
        return "\u{2014}".to_string();
    }
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result
}

fn fmt_ratio(ratio: f64) -> String {
    if ratio <= 0.0 {
        "\u{2014}".to_string()
    } else {
        format!("{:.2}x", ratio)
    }
}

fn status_icon(ratio: f64) -> &'static str {
    if ratio <= 0.0 {
        " "
    } else if ratio <= 2.0 {
        "\u{2713}"
    } else {
        "\u{25b3}"
    }
}

/// Recursively find all .baseline.tasm files in a directory.
fn find_baseline_files(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(find_baseline_files(&path));
            } else if path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with(".baseline.tasm"))
            {
                files.push(path);
            }
        }
    }
    files
}

/// Resolve the bench directory by searching ancestor directories.
fn resolve_bench_dir(dir: &std::path::Path) -> PathBuf {
    if dir.is_dir() {
        return dir.to_path_buf();
    }
    if dir.is_relative() {
        if let Ok(cwd) = std::env::current_dir() {
            let mut ancestor = cwd.as_path();
            loop {
                let candidate = ancestor.join(dir);
                if candidate.is_dir() {
                    return candidate;
                }
                match ancestor.parent() {
                    Some(parent) => ancestor = parent,
                    None => break,
                }
            }
        }
    }
    dir.to_path_buf()
}
