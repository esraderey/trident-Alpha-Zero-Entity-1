use std::path::Path;
use std::process;

use clap::Args;

#[derive(Args)]
pub struct TrainArgs {
    /// Epochs over the full corpus (default: 10)
    #[arg(short, long, default_value = "10")]
    pub epochs: u64,
    /// Generations per file per epoch (default: 10)
    #[arg(short, long, default_value = "10")]
    pub generations: u64,
    /// Use GPU acceleration (default: CPU parallel)
    #[arg(long)]
    pub gpu: bool,
}

pub fn cmd_train(args: TrainArgs) {
    // Discover all .tri files in vm/, std/, os/
    let corpus = discover_corpus();
    if corpus.is_empty() {
        eprintln!("error: no .tri files found in vm/, std/, os/");
        process::exit(1);
    }

    use trident::ir::tir::neural::weights;

    let meta = weights::load_best_meta().ok();
    let gen_start = meta.as_ref().map_or(0, |m| m.generation);

    let trainable: Vec<_> = corpus.iter().filter(|f| has_trainable_blocks(f)).collect();

    eprintln!(
        "Training neural optimizer: {} epochs x {} files x {} gens/file",
        args.epochs,
        trainable.len(),
        args.generations,
    );
    eprintln!(
        "  corpus: {} files ({} trainable, {} intrinsic)",
        corpus.len(),
        trainable.len(),
        corpus.len() - trainable.len(),
    );
    if gen_start > 0 {
        eprintln!("  resuming from generation {}", gen_start);
    }
    eprintln!();

    let start = std::time::Instant::now();
    let mut total_trained = 0u64;

    for epoch in 0..args.epochs {
        // Shuffle file order each epoch for diverse training signal
        let mut epoch_files = trainable.clone();
        shuffle(&mut epoch_files, gen_start + epoch);

        let epoch_start = std::time::Instant::now();
        let mut epoch_cost_sum = 0u64;
        let mut epoch_count = 0u64;

        for (i, file) in epoch_files.iter().enumerate() {
            let short = short_path(file);
            eprint!(
                "\r  epoch {}/{} [{}/{}] {}                    ",
                epoch + 1,
                args.epochs,
                i + 1,
                epoch_files.len(),
                short,
            );
            use std::io::Write;
            let _ = std::io::stderr().flush();

            match train_one(file, args.generations, args.gpu) {
                TrainResult::Trained { score, .. } => {
                    epoch_cost_sum += score;
                    epoch_count += 1;
                    total_trained += 1;
                }
                _ => {}
            }
        }

        let epoch_elapsed = epoch_start.elapsed();
        let avg_cost = if epoch_count > 0 {
            epoch_cost_sum / epoch_count
        } else {
            0
        };
        eprintln!(
            "\r  epoch {}/{} done — avg cost: {}, {:.1}s                    ",
            epoch + 1,
            args.epochs,
            avg_cost,
            epoch_elapsed.as_secs_f64(),
        );
    }

    let elapsed = start.elapsed();
    let meta = weights::load_best_meta().ok();
    let gen_end = meta.as_ref().map_or(0, |m| m.generation);

    eprintln!();
    eprintln!(
        "Done: {} file-trainings, {} total generations ({:.1}s)",
        total_trained,
        gen_end - gen_start,
        elapsed.as_secs_f64(),
    );

    if let Some(meta) = meta {
        eprintln!(
            "  model: gen {}, score {}, status: {}",
            meta.generation, meta.best_score, meta.status,
        );
    }
}

/// Discover all .tri files in vm/, std/, os/ relative to repo root.
fn discover_corpus() -> Vec<std::path::PathBuf> {
    let root = find_repo_root();
    let mut files = Vec::new();
    for dir in &["vm", "std", "os"] {
        let dir_path = root.join(dir);
        if dir_path.is_dir() {
            files.extend(super::resolve_tri_files(&dir_path));
        }
    }
    files.sort();
    files
}

/// Find the repository root by looking for Cargo.toml upward.
fn find_repo_root() -> std::path::PathBuf {
    let mut dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    loop {
        if dir.join("Cargo.toml").exists() && dir.join("vm").is_dir() {
            return dir;
        }
        if !dir.pop() {
            // Fallback to cwd
            return std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        }
    }
}

/// Check if a file has any trainable TIR blocks (without full training).
fn has_trainable_blocks(file: &Path) -> bool {
    let options = super::resolve_options("triton", "debug", None);
    let ir = match trident::build_tir_project(file, &options) {
        Ok(ir) => ir,
        Err(_) => return false,
    };
    let blocks = trident::ir::tir::encode::encode_blocks(&ir);
    !blocks.is_empty()
}

/// Deterministic shuffle using a simple hash.
fn shuffle(files: &mut Vec<&std::path::PathBuf>, seed: u64) {
    let n = files.len();
    if n <= 1 {
        return;
    }
    let mut state = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    for i in (1..n).rev() {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let j = (state >> 33) as usize % (i + 1);
        files.swap(i, j);
    }
}

/// Shorten a path for display (show from vm/, std/, or os/).
fn short_path(path: &Path) -> String {
    let s = path.to_string_lossy();
    for prefix in &["vm/", "std/", "os/"] {
        if let Some(pos) = s.find(prefix) {
            return s[pos..].to_string();
        }
    }
    s.to_string()
}

enum TrainResult {
    Trained {
        #[allow(dead_code)]
        blocks: usize,
        score: u64,
    },
    NoBlocks,
    Failed,
}

fn train_one(file: &Path, generations: u64, gpu: bool) -> TrainResult {
    use trident::field::PrimeField;
    use trident::ir::tir::encode;
    use trident::ir::tir::lower::decode_output;
    use trident::ir::tir::neural::evolve::Population;
    use trident::ir::tir::neural::model::NeuralModel;
    use trident::ir::tir::neural::weights::{self, OptimizerMeta, OptimizerStatus};

    let options = super::resolve_options("triton", "debug", None);

    let ir = match trident::build_tir_project(file, &options) {
        Ok(ir) => ir,
        Err(_) => return TrainResult::Failed,
    };

    let blocks = encode::encode_blocks(&ir);
    if blocks.is_empty() {
        return TrainResult::NoBlocks;
    }

    // Load current model (reloaded each file — picks up prior file's improvements)
    let (model, meta) = match weights::load_best_weights() {
        Ok(w) => {
            let meta = weights::load_best_meta().unwrap_or(OptimizerMeta {
                generation: 0,
                weight_hash: weights::hash_weights(&w),
                best_score: 0,
                prev_score: 0,
                baseline_score: 0,
                status: OptimizerStatus::Improving,
            });
            (NeuralModel::from_weight_vec(&w), meta)
        }
        Err(_) => {
            let meta = OptimizerMeta {
                generation: 0,
                weight_hash: String::new(),
                best_score: 0,
                prev_score: 0,
                baseline_score: 0,
                status: OptimizerStatus::Improving,
            };
            (NeuralModel::zeros(), meta)
        }
    };

    let gen_start = meta.generation;
    let current_weights = model.to_weight_vec();
    let mut pop = if current_weights.iter().all(|w| w.to_f64() == 0.0) {
        Population::new_random(gen_start.wrapping_add(42))
    } else {
        Population::from_weights(&current_weights, gen_start.wrapping_add(42))
    };

    // Classical baselines
    let lowering = trident::ir::tir::lower::create_stack_lowering(&options.target_config.name);
    let baseline_tasm = lowering.lower(&ir);
    let baseline_profile = trident::cost::scorer::profile_tasm_str(&baseline_tasm.join("\n"));
    let baseline_cost = baseline_profile.cost();

    let score_before = if meta.best_score > 0 {
        meta.best_score
    } else {
        baseline_cost
    };

    let per_block_baselines: Vec<u64> = blocks
        .iter()
        .map(|block| {
            let block_ops = &ir[block.start_idx..block.end_idx];
            if block_ops.is_empty() {
                return 1;
            }
            let block_tasm = lowering.lower(block_ops);
            if block_tasm.is_empty() {
                return 1;
            }
            let profile = trident::cost::scorer::profile_tasm(
                &block_tasm.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            );
            profile.cost().max(1)
        })
        .collect();

    // GPU acceleration
    let gpu_accel = if gpu {
        trident::gpu::neural_accel::NeuralAccelerator::try_new(
            &blocks,
            trident::ir::tir::neural::evolve::POP_SIZE as u32,
        )
    } else {
        None
    };

    // Train
    let mut best_seen = i64::MIN;
    for gen in 0..generations {
        if let Some(ref accel) = gpu_accel {
            let weight_vecs: Vec<Vec<u64>> = pop
                .individuals
                .iter()
                .map(|ind| ind.weights.iter().map(|w| w.raw().to_u64()).collect())
                .collect();
            let gpu_outputs = accel.batch_forward(&weight_vecs);
            for (i, ind) in pop.individuals.iter_mut().enumerate() {
                let mut total = 0i64;
                for (b, _block) in blocks.iter().enumerate() {
                    total -= score_neural_output(&gpu_outputs[i][b], per_block_baselines[b]) as i64;
                }
                ind.fitness = total;
            }
            pop.update_best();
        } else {
            pop.evaluate_with_baselines(
                &blocks,
                &per_block_baselines,
                |m: &mut NeuralModel,
                 block: &trident::ir::tir::encode::TIRBlock,
                 block_baseline: u64| {
                    let output = m.forward(block);
                    if output.is_empty() {
                        return -(block_baseline as i64);
                    }
                    let candidate_lines = decode_output(&output);
                    if candidate_lines.is_empty() {
                        return -(block_baseline as i64);
                    }
                    let profile = trident::cost::scorer::profile_tasm(
                        &candidate_lines
                            .iter()
                            .map(|s| s.as_str())
                            .collect::<Vec<_>>(),
                    );
                    -(profile.cost().min(block_baseline) as i64)
                },
            );
        }

        let gen_best = pop
            .individuals
            .iter()
            .map(|i| i.fitness)
            .max()
            .unwrap_or(i64::MIN);
        if gen_best > best_seen {
            best_seen = gen_best;
        }

        pop.evolve(gen_start.wrapping_add(gen));
    }

    // Save
    let best = pop.best_weights();
    let score_after = if best_seen > i64::MIN {
        (-best_seen) as u64
    } else {
        baseline_cost
    };

    let weight_hash = weights::hash_weights(best);
    let project_root = file.parent().unwrap_or(Path::new("."));
    let _ = weights::save_weights(best, &weights::weights_path(project_root));

    let mut tracker = weights::ConvergenceTracker::new();
    let status = tracker.record(score_after);

    let new_meta = OptimizerMeta {
        generation: gen_start + generations,
        weight_hash,
        best_score: score_after,
        prev_score: score_before,
        baseline_score: baseline_cost,
        status,
    };
    let _ = weights::save_meta(&new_meta, &weights::meta_path(project_root));

    TrainResult::Trained {
        blocks: blocks.len(),
        score: score_after,
    }
}

fn score_neural_output(raw_codes: &[u32], block_baseline: u64) -> u64 {
    use trident::ir::tir::lower::decode_output;

    let codes: Vec<u64> = raw_codes
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u64)
        .collect();
    if codes.is_empty() {
        return block_baseline;
    }
    let candidate_lines = decode_output(&codes);
    if candidate_lines.is_empty() {
        return block_baseline;
    }
    let profile = trident::cost::scorer::profile_tasm(
        &candidate_lines
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>(),
    );
    profile.cost().min(block_baseline)
}
