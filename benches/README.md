# Trident Benchmarks

Per-function compiler overhead analysis: compiles real library modules
from `std/`, `vm/`, and `os/`, then compares instruction counts and
execution costs against hand-optimized TASM baselines.

## Directory Structure

```
benches/
  end_to_end.rs              Criterion bench
  harnesses/                 Live execution programs (.tri + .inputs)
    std/compiler/lexer.tri
    std/compiler/lexer.inputs
    std/trinity/inference.tri
    ...
  references/                Rust ground truth
    std/crypto/poseidon2.rs
    std/nn/tensor.rs
    ...

baselines/triton/            Hand-optimized TASM (separate top-level dir)
  std/crypto/poseidon2.tasm
  vm/core/field.tasm
  os/neptune/kernel.tasm
  ...
```

- **`harnesses/`** — `.tri` source + `.inputs` files for live execution benchmarks
- **`references/`** — Rust programs that generate inputs and expected outputs (ground truth)
- **`baselines/triton/`** — hand-optimized TASM baselines (top-level, outside `benches/`)

## How It Works

1. `trident bench` scans `baselines/triton/` for `.tasm` files
2. Each baseline maps to a source module by path:
   `baselines/triton/std/crypto/auth.tasm` -> `std/crypto/auth.tri`
3. The source module is compiled through the full pipeline (resolve, parse,
   typecheck, TIR, optimize, lower) without linking
4. Both compiled output and baseline are parsed into per-function
   instruction maps
5. Functions are matched by label name and instruction counts compared

## Metrics

| Column | Meaning |
|--------|---------|
| Tri    | Compiler-generated instruction count |
| Hand   | Hand-optimized baseline instruction count |
| Ratio  | Tri / Hand (1.00x = compiler matches expert) |

## Running

```nu
trident bench                         # from project root
trident bench baselines/triton/       # explicit directory
trident bench baselines/triton/std/   # subdirectory
```

Works from any subdirectory -- walks up to find `baselines/`.

## Adding a Baseline

1. Write the `.tri` module in `std/`, `vm/`, or `os/` (real library code)
2. Create the matching baseline path:
   `baselines/triton/std/crypto/newmod.tasm`
3. Write hand-optimized TASM with `__funcname:` labels matching the
   module's public functions
4. Run `trident bench` to see the comparison

## Baseline Format

```tasm
// Hand-optimized TASM baseline: std.crypto.example

__function_name:
    instruction1
    instruction2
    return

__another_function:
    instruction1
    return
```

Rules:
- Labels use `__funcname:` format (matching compiler output)
- Comments (`//`) are not counted
- Labels (ending with `:`) are not counted
- `halt` is not counted
- Blank lines are not counted
- Everything else is counted
