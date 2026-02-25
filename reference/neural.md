# Neural Optimizer

GNN encoder + Transformer decoder (~13M parameters) that compiles
TIR to TASM. Operates at the TIR→TASM boundary — the only
non-deterministic stage in the pipeline.

## Architecture

```
TIR ops → TirGraph → GNN Encoder → node embeddings
                                       ↓
              TASM tokens ← Beam Search ← Transformer Decoder
```

**Encoder:** 4-layer GATv2 (Graph Attention Network v2), d=256,
d_edge=32. Encodes a `TirGraph` (typed edges: DataDep, ControlFlow,
MemOrder) into per-node embeddings + global context. ~3M params.

**Decoder:** 6-layer Transformer with self-attention + cross-attention
to GNN node embeddings. Stack-aware: injects stack depth (max 65)
and type window (8 slots) as additional features at each step.
d=256, 8 heads, d_ff=1024, max_seq=256. ~10M params.

**Vocabulary:** 140 tokens (EOS + 139 TASM instructions). Covers the
full Triton VM ISA: push constants, stack ops (dup/swap/pick/place
0-15), arithmetic, comparison, bitwise, control flow, I/O, memory,
crypto, extension field. Token 0 = EOS.

**Grammar mask:** At each decoder step, a stack state machine
restricts the vocabulary to syntactically valid next tokens. Prevents
the model from emitting invalid TASM.

## Inference

Beam search with K=32, max 256 steps. Length normalization (alpha=0.7),
repetition penalty (1.5x over 16-token window).

```
1. Build TirGraph from TIR ops
2. Encode graph → node features [N, 59], typed edges [E]
3. GNN forward → node embeddings [N, 256]
4. Beam search (K=32): autoregressive decoding with grammar mask
5. Validate candidates: stack verify + cost scoring
6. Return cheapest valid candidate, or fallback to compiler output
```

Validation uses two layers:
- **Stack verifier** (`src/cost/stack_verifier.rs`): executes
  straight-line TASM on concrete Goldilocks values, checks stack
  transformation matches compiler output. Fast (~25 instructions
  modeled), used for training feedback.
- **Table profiler** (`src/cost/scorer.rs`): counts actual table
  row increments across 6 Triton VM AETs. Cost = padded height
  (next power of 2 of max table height). The cliff function.

## Training

Three stages, run via `trident train`:

### Stage 1: Supervised pre-training

Teacher forcing with cross-entropy loss on (TirGraph, TASM) pairs.
Training corpus: all `.tri` files from `vm/`, `std/`, `os/`, compiled
to TIR and split into per-function blocks.

- Optimizer: AdamW (lr=3e-4, weight_decay=0.01)
- Cosine LR decay to 1e-5
- Gradient clipping at norm 1.0
- Early stopping: patience 3 epochs
- Checkpoint: `model/general/v2/stage1_best.mpk`

```
trident train --epochs 10           # default: 10 epochs
trident train --stage 1 --epochs 50 # explicit stage 1
```

### Stage 2: GFlowNet fine-tuning

Trajectory Balance loss. The model samples TASM sequences and
receives reward from actual cost improvement over compiler baseline.

- Temperature annealing: tau 2.0 → 0.5 over 10K steps
- Partial credit shaping for first 1K steps (then pure reward)
- Checkpoint: `model/general/v2/stage2_latest.mpk`

```
trident train --stage 2 --epochs 20
```

### Stage 3: Online learning

Micro-finetunes on new build results via replay buffer. Regression
guard prevents deploying checkpoints worse than production.

- Triggers after 50 new results or 24h
- 200 GFlowNet gradient steps per micro-finetune
- 10% historical samples mixed in (prevents forgetting)
- Max 2pp validity regression allowed

```
trident train --stage 3
```

### Reset

```
trident train reset    # deletes model weights + cached .neural.tasm
```

## Data Flow

**TirGraph** (`src/neural/data/tir_graph.rs`): Converts `Vec<TIROp>`
into a graph with 54 op kinds (4 tiers), 3 edge types, 59-dimensional
node features (one-hot opcode + field type + structural flags).

**Training pairs** (`src/neural/data/pairs.rs`): Extracted by
compiling each `.tri` file, splitting TIR into per-function blocks,
lowering each to TASM. Each pair = (TIR ops, TASM lines).

**Replay buffer** (`src/neural/data/replay.rs`): Priority-based
buffer at `model/general/v2/replay.rkyv`. Stores build results
for online learning.

## File Map

```
src/neural/
  mod.rs                Public API: compile(), load_model(), compile_with_model()
  checkpoint.rs         Save/load via burn NamedMpk format
  model/
    composite.rs        NeuralCompilerV2 = encoder + decoder (~13M params)
    encoder.rs          GATv2 GNN encoder (4 layers, d=256)
    decoder.rs          Stack-aware Transformer decoder (6 layers, 8 heads)
    vocab.rs            140-token TASM vocabulary
    grammar.rs          Stack state machine for grammar masking
    grammar_tables.rs   Precomputed grammar transition tables
    gnn_ops.rs          Scatter/gather ops for GNN message passing
  inference/
    beam.rs             Beam search (K=32, max_steps=256)
    execute.rs          Candidate validation and ranking
  training/
    supervised.rs       Stage 1: cross-entropy with teacher forcing
    gflownet.rs         Stage 2: Trajectory Balance fine-tuning
    online.rs           Stage 3: replay buffer micro-finetune
    augment.rs          Data augmentation
  data/
    pairs.rs            Training pair extraction from .tri corpus
    replay.rs           Priority replay buffer (rkyv serialized)
    tir_graph.rs        TIR → typed graph conversion (54 ops, 3 edge types)

src/cost/
  scorer.rs             Table profiler (6 AETs, cliff-aware cost)
  stack_verifier.rs     Concrete-value TASM execution for fast verification
```

## Speculative Compilation

The neural path is strictly speculative. Classical lowering always
runs. Neural output is accepted only when:

1. Stack verifier confirms equivalent stack transformation
2. Table cost is strictly less than compiler output
3. Neural output is not identical to compiler output (no memorization)

This is enforced in `src/cli/bench.rs:compile_neural_tasm_inline()`
and `src/neural/mod.rs:compile()`.
