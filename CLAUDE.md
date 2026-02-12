# Trident — Claude Code Instructions

## Source of Truth

`docs/reference/` is the canonical reference for all Trident design decisions.
Each file owns a specific domain:

- **`language.md`** — syntax, types, operators, builtins, attributes,
  memory model, type checking rules, permanent exclusions
- **`ir.md`** — TIROp variant names, counts, tier assignments, lowering paths,
  naming conventions, architecture diagrams, pipeline
- **`targets.md`** — OS model, integration tracking, how-to-add checklists
- **`vm.md`** — VM registry, lowering paths, tier/type/builtin tables,
  cost models
- **`os.md`** — OS concepts (neuron/signal/token), `os.*` portable APIs,
  `os.<os>.*` OS-specific extensions, OS registry
- **`stdlib.md`** — `std.*` library modules, common patterns
- **`provable.md`** — Tier 2-3 builtins (sponge, Merkle, extension field,
  proof composition)
- **`errors.md`** — error codes and diagnostic messages
- **`grammar.md`** — EBNF grammar
- **`cli.md`** — compiler commands and flags
- **`briefing.md`** — AI-optimized compact cheat-sheet

Any change to the IR, language, or target model MUST update the corresponding
reference doc first, then propagate to code. If docs/reference/ and code
disagree, docs/reference/ wins.

## Four-Tier Namespace

```
vm.*              Compiler intrinsics       TIR ops (hash, sponge, pub_read, assert)
std.*             Real libraries            Implemented in Trident (sha256, bigint, ecdsa)
os.*              Portable runtime          os.signal, os.neuron, os.state, os.time
os.<os>.*         OS-specific APIs          os.neptune.xfield, os.solana.pda
```

- `vm/{name}/` — per-VM directory: `target.toml` (config) + `README.md` (docs)
- `vm/core/`, `vm/io/`, `vm/crypto/` — shared VM intrinsic `.tri` files
- `os/{name}/` — per-OS directory: `target.toml` (config) + `README.md` (docs) + `.tri` extensions
- `std/` — pure Trident library code (no `#[intrinsic]`)
- Module resolution: `src/tools/resolve.rs`

## Build & Test

```
cargo check          # type-check
cargo test           # 743+ tests
cargo build --release
```

## License

Cyber License: Don't trust. Don't fear. Don't beg.
