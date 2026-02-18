# GPU Neural Optimizer Rewrite Plan

## Diagnosis: Why the Current Implementation Fails

### Problem 1: Memory-Bound Monolithic Shader (800ms/gen)
The current `neural.wgsl` (561 LOC) dispatches 928 threads (16 individuals x 58 blocks).
Each thread independently reads the ENTIRE weight buffer for its individual: 62,080 weights x 8 bytes = ~484KB per thread.
With 928 threads, that is ~440MB of global memory reads per dispatch. GPU L2 cache is ~8-16MB on Apple Silicon -- massive thrashing.

Additionally, each thread allocates 11,264 vec2<u32> entries (88KB) in a global scratch buffer.
Total scratch: 928 * 88KB = ~80MB. This exceeds GPU memory bandwidth and causes severe stalls.

### Problem 2: Broken Fitness Function
In `/Users/mastercyb/git/trident/src/cli/build.rs` line 254:
```rust
let per_block_baseline = baseline_cost / blocks.len().max(1) as u64;
```
This averages the total baseline cost uniformly across all blocks. But blocks have wildly different costs
(a hash-heavy block might cost 10x more than a simple arithmetic block). When the neural optimizer
produces output for a specific block, it gets compared against this averaged baseline -- not the actual
cost of classically lowering THAT specific block. The model gets no useful gradient signal: it could
produce perfect output for an easy block and still appear to lose against the inflated average.

### Problem 3: Vocabulary Too Limited (64 tokens)
The output vocabulary in `decode_output()` has exactly 64 entries (indices 0-63). The model's decoder output
dimension is DIM=64, and argmax over 64 positions selects one of 64 tokens. This means:
- `push` only supports literals 0, 1, -1 (no variable immediates)
- `dup`/`swap` only go up to indices 7/9 (missing higher stack positions)
- Missing: `read_mem 2..4`, `write_mem 2..4`, `read_io 2..4`, `write_io 2..4`, `divine 2..4`
- No way to encode `push <arbitrary_immediate>` which is critical for real programs

### Problem 4: No Semantic Verification
The training loop in build.rs (both GPU and CPU paths) never verifies that neural output is
semantically equivalent to the classical lowering. It just scores the cost. The model can achieve
low cost by emitting fewer instructions that do the wrong thing.

### Problem 5: Hardcoded Model Architecture
DIM=64, HEADS=2, HEAD_DIM=32, LAYERS=2, FFN_HIDDEN=64 are all constants baked into both the
Rust model (`model.rs`) and the WGSL shader. Changing any hyperparameter requires synchronized
edits to both files and complete recompilation of the shader.

---

## Architecture Decision: One Individual At A Time

### Why the current approach fails on GPU
The fundamental issue is that 16 individuals x 58 blocks = 928 independent forward passes,
each needing to read 484KB of weights. GPU caches cannot serve 928 concurrent readers of
different data regions (16 different weight vectors x 484KB = 7.7MB just for weights).

### The fix: Sequential individuals, parallel blocks
Upload ONE individual's weights (484KB) to a uniform/storage buffer.
Dispatch 58 threads (one per block). All 58 threads read the SAME weights from the same
memory locations. 484KB fits entirely in L2 cache. After one dispatch, upload next individual's
weights. 16 dispatches total.

Expected performance:
- Weight upload: 484KB per individual, 16 uploads = ~7.7MB total, negligible at PCIe/unified memory speeds
- Per-dispatch compute: 58 threads, each doing ~62K multiply-accumulates = ~3.6M field muls
- At ~500 GFLOPS effective on Apple M-series for u32 workloads: ~1-2ms per dispatch
- Total: ~16-32ms per generation (vs 800ms current, 250ms parallel CPU)

### Alternative considered: Shared memory tiling
16 workgroups (one per individual), 64 threads each. Each workgroup cooperatively loads its
individual's weights into workgroup shared memory, then all threads read from shared. Single dispatch.
Problem: WGSL workgroup shared memory limit is typically 16KB on Apple Silicon (Metal).
484KB of weights >> 16KB. Would require tiled loading with many barrier syncs.
Not worth the complexity. Sequential dispatch is simpler and fast enough.

---

## New Shader Architecture

### File: `src/gpu/shaders/goldilocks.wgsl` (~120 LOC) -- NEW
Extract canonical Goldilocks field arithmetic from the current monolithic shader into a standalone
shared library. This is the canonical form (NOT Montgomery), matching trident's `Fixed` type.

Contents (extracted from current `neural.wgsl` lines 79-193):
- `gl_add(a, b)` -- modular addition
- `gl_sub(a, b)` -- modular subtraction
- `mul32(a, b)` -- 32x32->64 helper
- `gl_reduce(v)` -- single reduction
- `canon_reduce128(lo, hi)` -- 128->64 bit reduction via Goldilocks structure
- `canon_mul(a, b)` -- full canonical multiply

Note: We use canonical form (not Montgomery from trisha's `goldilocks.wgsl`) because:
1. Trident's `Fixed` type uses canonical representation
2. Zero conversion overhead at Rust/GPU boundary
3. The canonical `canon_reduce128` is already battle-tested in the existing shader
4. Montgomery's faster multiply is offset by conversion cost at upload/download boundaries

### File: `src/gpu/shaders/fixed_point.wgsl` (~60 LOC) -- NEW
Fixed-point operations built on top of `goldilocks.wgsl`.

Contents (extracted from current `neural.wgsl` lines 195-250):
- `fp_mul(a, b)` -- fixed-point multiply with inv_scale rescale
- `fp_relu(x)` -- ReLU (positive half of field)
- `fp_gt(a, b)` -- signed comparison
- `fp_inv(x)` -- field inverse via Fermat's little theorem
- `fp_inv_u32(n)` -- fast integer reciprocal
- Constants: `FP_ZERO`, `fp_one()`, `inv_scale()`, `half_p()`

### File: `src/gpu/shaders/neural_forward.wgsl` (~300 LOC) -- REPLACES `neural.wgsl`
Composable forward pass using the shared libraries. Key structural changes:

1. **One individual per dispatch**: Weights in a single shared buffer, no individual indexing
2. **Block-parallel**: `global_invocation_id.x` = block index (0..num_blocks)
3. **Scratch per block**: Each block thread gets its own scratch region, but only num_blocks threads
   active (58 instead of 928), so scratch total is 58 * 88KB = ~5MB (vs 80MB before)
4. **Configurable dimensions**: Model hyperparameters passed via uniform buffer, not hardcoded constants

Buffer layout:
```
@group(0) @binding(0) var<storage, read>       weights: array<vec2<u32>>;   // one individual
@group(0) @binding(1) var<storage, read>       blocks: array<vec2<u32>>;    // all blocks
@group(0) @binding(2) var<storage, read>       block_meta: array<u32>;      // node counts
@group(0) @binding(3) var<storage, read_write> outputs: array<u32>;         // per-block output
@group(0) @binding(4) var<uniform>             params: Params;              // config + constants
@group(0) @binding(5) var<storage, read_write> scratch: array<vec2<u32>>;   // per-block scratch
```

The `Params` uniform now includes model hyperparameters:
```wgsl
struct Params {
    num_blocks: u32,
    dim: u32,
    heads: u32,
    head_dim: u32,
    layers: u32,
    ffn_hidden: u32,
    max_output: u32,
    vocab_size: u32,
    inv_scale_lo: u32,
    inv_scale_hi: u32,
    half_p_lo: u32,
    half_p_hi: u32,
}
```

WGSL inclusion strategy: Since WGSL has no `#include`, the Rust side concatenates the three
shader files at compile time:
```rust
const NEURAL_SHADER: &str = concat!(
    include_str!("shaders/goldilocks.wgsl"),
    include_str!("shaders/fixed_point.wgsl"),
    include_str!("shaders/neural_forward.wgsl"),
);
```

---

## New Rust-Side GPU Orchestration

### File: `src/gpu/neural_accel.rs` (~250 LOC) -- REWRITE

The `NeuralAccelerator` is restructured for sequential-individual, parallel-block dispatch:

```rust
pub struct NeuralAccelerator {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    // Static buffers (uploaded once)
    block_buf: wgpu::Buffer,
    meta_buf: wgpu::Buffer,
    params_buf: wgpu::Buffer,
    scratch_buf: wgpu::Buffer,       // num_blocks * SCRATCH_PER_THREAD * 8 bytes
    // Per-individual buffers (rewritten each dispatch)
    weight_buf: wgpu::Buffer,        // SINGLE individual's weights
    output_buf: wgpu::Buffer,        // num_blocks * MAX_OUTPUT outputs
    staging_buf: wgpu::Buffer,       // readback
    // Dimensions
    num_blocks: u32,
}
```

Key API change:
```rust
impl NeuralAccelerator {
    /// Create accelerator for a set of blocks.
    pub fn try_new(blocks: &[TIRBlock], model_config: &ModelConfig) -> Option<Self>;

    /// Run forward pass for ONE individual on all blocks.
    /// Returns Vec<Vec<u32>> -- one output sequence per block.
    fn forward_one(&self, weights: &[u64]) -> Vec<Vec<u32>>;

    /// Run forward pass for all individuals (sequential dispatch).
    /// Returns [num_individuals][num_blocks][output_codes].
    pub fn batch_forward(&self, weight_vecs: &[Vec<u64>]) -> Vec<Vec<Vec<u32>>>;
}
```

The `batch_forward` method loops over individuals, calling `forward_one` for each:
```rust
pub fn batch_forward(&self, weight_vecs: &[Vec<u64>]) -> Vec<Vec<Vec<u32>>> {
    let mut results = Vec::with_capacity(weight_vecs.len());
    for wv in weight_vecs {
        results.push(self.forward_one(wv));
    }
    results
}
```

Each `forward_one` call:
1. `queue.write_buffer(&self.weight_buf, 0, ...)` -- upload one individual's weights
2. Create command encoder, dispatch `ceil(num_blocks / 64)` workgroups
3. Copy output buffer to staging
4. Submit and poll
5. Map staging buffer, read results, unmap

Buffer sizes shrink dramatically:
- Scratch: 58 blocks * 88KB = ~5MB (was 80MB)
- Weights: 484KB per dispatch (was 7.7MB all at once)
- Output: 58 * 16 * 4 = ~3.7KB (was 59KB)

### File: `src/gpu/shaders.rs` (~5 LOC) -- MODIFY
Change from single shader embedding to concatenated composition:
```rust
pub const NEURAL_SHADER: &str = concat!(
    include_str!("shaders/goldilocks.wgsl"),
    "\n",
    include_str!("shaders/fixed_point.wgsl"),
    "\n",
    include_str!("shaders/neural_forward.wgsl"),
);
```

### File: `src/gpu/mod.rs` (~32 LOC) -- KEEP AS IS
The `try_create_device()` function is clean and correct.

---

## Fitness Function Fix

### The Bug (in `/Users/mastercyb/git/trident/src/cli/build.rs`)

Current broken code (line 254):
```rust
let per_block_baseline = baseline_cost / blocks.len().max(1) as u64;
```

This computes an averaged per-block cost. When evaluating individual outputs:
```rust
if codes.is_empty() {
    total -= per_block_baseline as i64;  // WRONG: same penalty for all blocks
}
```

A block that would classically cost 512 and a block that costs 32 both get penalized
by the same `per_block_baseline` (~67 for 58 blocks with total cost 3922).

### The Fix

Compute actual per-block baseline costs by running `profile_tasm` on each block's
classical lowering independently:

```rust
// Compute per-block baselines
let lowering = create_stack_lowering(&options.target_config.name);
let per_block_baselines: Vec<u64> = blocks.iter().map(|block| {
    // Reconstruct the TIR ops for this block and lower them classically
    let block_tasm_lines = decode_output_for_block(block);  // or re-lower
    scorer::profile_tasm(&block_tasm_lines).cost()
}).collect();
```

However, there is a subtlety: `encode_blocks` extracts TIR blocks but does not preserve the
original TIR ops needed for re-lowering. Two approaches:

**Approach A (simpler, approximate):** Profile each block's nodes by mapping opcodes back to
TASM instructions and profiling those. This is approximate but gives per-block relative costs.

**Approach B (accurate, recommended):** Change `encode_blocks` to also return the original
`TIROp` slice for each block. Then lower each block independently and profile:

```rust
pub struct TIRBlockWithOps {
    pub encoded: TIRBlock,
    pub ops: Vec<TIROp>,  // original ops for this block
}

pub fn encode_blocks_with_ops(ops: &[TIROp]) -> Vec<TIRBlockWithOps>;
```

Then in the training loop:
```rust
let blocks_with_ops = encode::encode_blocks_with_ops(&ir);
let per_block_baselines: Vec<u64> = blocks_with_ops.iter().map(|bwo| {
    let classical_tasm = lowering.lower(&bwo.ops);
    scorer::profile_tasm_str(&classical_tasm.join("\n")).cost()
}).collect();

// In scoring:
for (b, block) in blocks.iter().enumerate() {
    let baseline = per_block_baselines[b];
    // ... score against this block's actual baseline
}
```

**Fitness function redesign:**
```
fitness(individual) = -sum_over_blocks(min(neural_cost[b], baseline_cost[b]))
```
This rewards the model for beating the per-block baseline. Blocks where neural produces
garbage or nothing default to baseline cost (no penalty beyond what classical already costs).

---

## Training Integration Changes

### File: `src/cli/build.rs` -- MODIFY `run_neural_analysis()`

The training loop needs these changes:

1. **Per-block baselines:** Compute actual per-block costs as described above.

2. **Semantic verification during training (lightweight):**
   Currently training never verifies. Full symbolic verification is too expensive per generation.
   Compromise: use `profile_tasm` validity as a proxy -- if the decoded TASM contains unknown
   instructions or impossible sequences, assign a penalty cost equal to 2x the block baseline.

3. **GPU dispatch restructuring:**
   Replace the current flat dispatch with sequential-individual dispatch:
   ```rust
   if let Some(ref accel) = gpu_accel {
       let weight_vecs: Vec<Vec<u64>> = ...;
       let gpu_outputs = accel.batch_forward(&weight_vecs);
       // Score each individual against per-block baselines
       for (i, ind) in pop.individuals.iter_mut().enumerate() {
           let mut total = 0i64;
           for (b, block) in blocks.iter().enumerate() {
               let codes = &gpu_outputs[i][b];
               let block_cost = score_neural_output(codes, per_block_baselines[b]);
               total -= block_cost as i64;
           }
           ind.fitness = total;
       }
       pop.update_best();
   }
   ```

4. **Score function extracted:**
   ```rust
   fn score_neural_output(codes: &[u32], block_baseline: u64) -> u64 {
       let codes: Vec<u64> = codes.iter()
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
       let profile = scorer::profile_tasm(
           &candidate_lines.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
       );
       // Return the better of neural and baseline
       profile.cost().min(block_baseline)
   }
   ```

---

## Vocabulary Expansion (Future Phase, Not In This Rewrite)

The 64-token vocabulary is a real limitation but expanding it requires changing the model
architecture (DIM must be >= vocab_size for argmax decoding). Options for a later phase:

1. **Factored output:** Two-step decode -- first predict opcode (54 options), then predict
   argument (variable width). This keeps DIM=64 but adds a second decoder step.

2. **Expand DIM to 128:** Doubles param count to ~124K but allows 128-token vocabulary
   with room for `push <imm8>` (push with 8-bit immediate offset), `dup 0..15`, etc.

3. **Hybrid vocabulary:** Use tokens 0-53 for pure opcodes (matching TIROp opcode mapping),
   tokens 54-63 for parameterized instructions with the immediate encoded separately.

For THIS rewrite, keep the 64-token vocabulary but acknowledge it as a known limitation
in the report output.

---

## File-by-File Plan with Estimated LOC

### New Files

| File | LOC | Description |
|------|-----|-------------|
| `src/gpu/shaders/goldilocks.wgsl` | ~120 | Canonical Goldilocks field arithmetic (extracted from neural.wgsl) |
| `src/gpu/shaders/fixed_point.wgsl` | ~60 | Fixed-point ops on top of goldilocks (extracted from neural.wgsl) |
| `src/gpu/shaders/neural_forward.wgsl` | ~300 | Forward pass shader, block-parallel, configurable dims |

### Modified Files

| File | Change | LOC Delta |
|------|--------|-----------|
| `src/gpu/shaders.rs` | Concatenated shader composition | ~5 (was 1) |
| `src/gpu/neural_accel.rs` | Sequential-individual dispatch, smaller buffers | ~250 (was ~250, rewrite) |
| `src/cli/build.rs` | Per-block baselines, extracted score function, new GPU dispatch | ~30 net change |
| `src/ir/tir/encode.rs` | Add `encode_blocks_with_ops()` returning original ops | ~40 added |

### Kept As-Is

| File | LOC | Reason |
|------|-----|--------|
| `src/gpu/mod.rs` | 32 | Device init is clean |
| `src/field/fixed.rs` | 439 | Fixed-point arithmetic is correct |
| `src/field/goldilocks.rs` | 101 | Field arithmetic is correct |
| `src/ir/tir/neural/report.rs` | 272 | Reporting is fine |
| `src/ir/tir/neural/model.rs` | ~350 | CPU model unchanged (GPU must match) |
| `src/ir/tir/neural/evolve.rs` | 297 | Evolutionary algorithm unchanged |
| `src/ir/tir/neural/weights.rs` | 328 | Weight persistence unchanged |
| `src/cost/scorer.rs` | ~250 | Table profiling unchanged |

### Deleted Files

| File | Reason |
|------|--------|
| `src/gpu/shaders/neural.wgsl` | Replaced by three composable files |

---

## Implementation Order

### Step 1: Extract Shader Libraries (goldilocks.wgsl + fixed_point.wgsl)
- Extract `gl_add`, `gl_sub`, `mul32`, `gl_reduce`, `canon_reduce128`, `canon_mul` from current
  `neural.wgsl` into `goldilocks.wgsl`
- Extract `fp_mul`, `fp_relu`, `fp_gt`, `fp_inv`, `fp_inv_sqrt`, `fp_inv_u32`, constants
  into `fixed_point.wgsl`
- Update `shaders.rs` to concatenate
- Run existing GPU tests to verify no regressions

### Step 2: Rewrite neural_forward.wgsl for Block-Parallel Dispatch
- Remove individual indexing (no `ind = pass_id / num_blocks`)
- Thread ID = block index directly
- Weights are a flat buffer for ONE individual
- Scratch indexed by `block_id * SCRATCH_PER_THREAD`
- Make dimensions read from uniform params (not constants)
- Keep the forward pass logic (encoder layers, decoder) identical

### Step 3: Rewrite neural_accel.rs for Sequential Dispatch
- New `forward_one()` method: upload one weight vector, dispatch num_blocks threads, readback
- `batch_forward()` loops over individuals calling `forward_one()`
- Dramatically smaller buffer allocations
- Update buffer layout to match new shader bindings

### Step 4: Fix Fitness Function in build.rs
- Add `encode_blocks_with_ops()` to encode.rs
- Compute per-block baselines via classical lowering + profiling
- Extract `score_neural_output()` function
- Update both GPU and CPU training paths to use per-block baselines

### Step 5: Run GPU-CPU Equivalence Tests
- Verify GPU forward pass still matches CPU forward pass (`gpu_matches_cpu_forward`)
- Verify GPU field arithmetic still matches CPU (`gpu_field_arithmetic_matches_cpu`)
- Run training for 10 generations and verify score improves

### Step 6: Benchmark and Validate
- Time per generation with new GPU dispatch vs old
- Verify cost drops below current 3922 after reasonable training
- Check that per-block baseline scoring produces meaningful gradient signal

---

## Verification Strategy

### Unit Tests (must all pass)

1. **`gpu_field_arithmetic_matches_cpu`** -- Existing test, verifies shader field ops match Rust.
   The extracted `goldilocks.wgsl` must produce identical results.

2. **`gpu_matches_cpu_forward`** -- Existing test, verifies full forward pass GPU=CPU.
   The restructured shader (block-parallel, single individual) must produce identical output
   to the CPU `NeuralModel::forward()`.

3. **`gpu_two_nodes_full_weights`** -- Existing test with 2-node blocks.

4. **New: `per_block_baseline_accuracy`** -- Verify that per-block baselines sum to approximately
   the total baseline (within rounding from power-of-2 padding).

5. **New: `score_neural_output_fallback`** -- Empty/invalid neural output returns block baseline cost.

### Integration Tests

1. **Training convergence:** Run `trident build program.tri --train 50` and verify:
   - Cost decreases over generations (or stays stable, does not increase)
   - GPU path produces same scores as CPU path
   - Weights are saved and can be reloaded

2. **Performance:** Time 50 generations on GPU. Target: <50ms total (<1ms/gen).
   Current: ~40 seconds (800ms/gen). Even 250ms/gen CPU baseline is the bar to beat.

### Regression Guards

- `cargo test` -- all existing tests pass
- `cargo check` -- zero warnings
- The `gpu_matches_cpu_forward` test is the critical correctness check: if the shader
  refactor breaks GPU-CPU equivalence, this test catches it immediately.

---

## Risk Assessment

### Risk 1: WGSL Concatenation Breaks Compilation
WGSL has no include mechanism. Concatenating three files could cause name conflicts or
scope issues. Mitigation: all functions use `gl_` or `fp_` prefixes. No global mutable state
in the library files. Test shader compilation as the first step.

### Risk 2: Sequential Dispatch Overhead
16 separate dispatches have fixed overhead (command encoding, submission, GPU scheduling).
On Apple Silicon with wgpu/Metal, typical dispatch overhead is ~10-50 microseconds.
16 dispatches = 160-800us overhead. Acceptable compared to the ~16ms compute time.

### Risk 3: Per-Block Baseline Computation Cost
Lowering each block independently and profiling adds upfront cost to training.
For 58 blocks, this is 58 calls to `TritonLowering::lower()` + `profile_tasm()`.
These are fast (microseconds each). Total: <1ms, computed once per training session.

### Risk 4: Weight Buffer Rewrite Latency
Uploading 484KB per individual via `queue.write_buffer` on unified memory (Apple Silicon)
should be nearly instant. On discrete GPU with PCIe, 484KB * 16 = 7.7MB over PCIe 4.0
(~25 GB/s) = ~0.3ms. Negligible.

---

## Expected Outcomes

| Metric | Current | After Rewrite | Notes |
|--------|---------|---------------|-------|
| GPU gen time | ~800ms | ~1-2ms | 400-800x improvement |
| CPU gen time | ~250ms | ~250ms (unchanged) | CPU path unaffected |
| Fitness signal | Broken (averaged) | Correct (per-block) | Model can actually learn |
| Neural cost | 3922 | Target: <2048 | With correct fitness, model should converge toward baseline |
| Baseline cost | 1024 | 1024 (unchanged) | Classical lowering unchanged |
| Shader LOC | 561 (monolithic) | ~480 (3 files) | More maintainable, reusable |
| Scratch memory | ~80MB | ~5MB | 16x reduction |
| Weight bandwidth | ~440MB/dispatch | ~484KB/dispatch | 900x reduction |
