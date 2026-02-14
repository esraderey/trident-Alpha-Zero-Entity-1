# Source Architecture

The compiler is organized as a pipeline. Source text enters at the syntax layer, flows through type checking and optional analysis passes, and exits as Triton VM assembly (TASM).

```
source.tri
    |
    v
 syntax/       lexer -> parser -> AST
    |
    v
 typecheck/    type checking, borrow checking, generics
    |
    +----> tir/       Trident IR: instruction selection + stack lowering
    |        +----> triton.rs  (Triton VM TASM output)
    |
    v
 output.tasm
```

Parallel to the main pipeline, several modules provide analysis, tooling, and package management:

```
 cost/         static cost analysis (trace height estimation)
 verify/       formal verification (symbolic execution, SMT, equivalence)
 tools/        LSP, scaffolding, module resolution, introspection
 package/     content-addressed package management, store, registry
```

## Module Map

| Module | LOC | What it does |
|--------|----:|--------------|
| [`syntax/`](syntax/) | 4,392 | [Lexer](syntax/lexer.rs), [parser](syntax/parser/), [token definitions](syntax/lexeme.rs), [formatter](syntax/format/) |
| [`typecheck/`](typecheck/) | 3,007 | [Type checker](typecheck/mod.rs) with borrow analysis, generics, and [builtin registration](typecheck/builtins.rs) |
| [`tir/`](tir/) | 3,678 | Trident IR: [opcode definitions](tir/mod.rs), [AST→TIR builder](tir/builder/), [Triton lowering](tir/lower/triton.rs), [stack manager](tir/stack.rs) |
| [`cost/`](cost/) | 2,335 | Static cost [analyzer](cost/analyzer.rs), per-function breakdown, [optimization hints and reports](cost/report.rs), target [cost models](cost/model/) |
| [`verify/`](verify/) | 5,570 | [Symbolic execution](verify/sym.rs), [constraint solving](verify/solve.rs), [SMT encoding](verify/smt.rs), [equivalence checking](verify/equiv.rs), [invariant synthesis](verify/synthesize.rs), [JSON reports](verify/report.rs) |
| [`package/`](package/) | 6,494 | [BLAKE3 hashing](package/hash.rs), [Poseidon2](package/poseidon2.rs), [definitions store](package/store.rs), [registry server/client](package/registry.rs), [dependency manifests](package/manifest.rs), [compilation cache](package/cache.rs) |
| [`tools/`](tools/) | 5,004 | [Language Server](tools/lsp.rs), [code scaffolding](tools/scaffold.rs), [definition viewer](tools/view.rs), [project config](tools/project.rs), [module resolution](tools/resolve.rs), [target configuration](tools/target.rs), [artifact packaging](tools/package.rs) |

## Top-Level Files

| File | LOC | Role |
|------|----:|------|
| [`ast.rs`](ast.rs) | 371 | AST node definitions shared by every stage |
| [`lib.rs`](lib.rs) | 2,700 | Public API, re-exports, and orchestration functions (`compile`, `analyze_costs`, `check_file`) |
| [`main.rs`](main.rs) | 2,650 | CLI entry point: argument parsing and command dispatch |
| [`linker.rs`](linker.rs) | 134 | Multi-module [linker](linker.rs) for cross-module calls |

**Total: ~36,700 lines across 57 Rust files, 5 runtime dependencies.**

## Compilation Pipeline

Syntax ([`syntax/`](syntax/)). The [lexer](syntax/lexer.rs) tokenizes source into the token types defined in [`lexeme.rs`](syntax/lexeme.rs). The [parser](syntax/parser/) produces a typed AST ([`ast.rs`](ast.rs)). The [formatter](syntax/format/) can pretty-print any AST back to canonical source.

Type Checking ([`typecheck/`](typecheck/)). The [type checker](typecheck/mod.rs) validates types, resolves generics via monomorphization, performs borrow/move analysis, and registers builtin function signatures ([`builtins.rs`](typecheck/builtins.rs)). Diagnostics are emitted for type mismatches, undefined variables, unused bindings, and borrow violations.

TIR Pipeline ([`tir/`](tir/)). The [TIR builder](tir/builder/mod.rs) translates the typed AST into a flat sequence of `TIROp` instructions. The [Triton lowering](tir/lower/triton.rs) produces TASM assembly. The [stack manager](tir/stack.rs) tracks operand positions with automatic RAM spill/reload. The [linker](linker.rs) resolves cross-module calls.

Cost Analysis ([`cost/`](cost/)). The [analyzer](cost/analyzer.rs) walks the AST and sums per-instruction costs using a target-specific [`CostModel`](cost/model/mod.rs). The [report module](cost/report.rs) formats results, generates optimization hints, and provides JSON serialization for `--compare` workflows.

Formal Verification ([`verify/`](verify/)). The [symbolic executor](verify/sym.rs) builds path constraints over the AST. The [solver](verify/solve.rs) uses Schwartz-Zippel randomized testing and bounded model checking. The [SMT module](verify/smt.rs) encodes constraints in SMT-LIB2 for external solvers. The [equivalence checker](verify/equiv.rs) proves two functions compute the same result. The [synthesizer](verify/synthesize.rs) infers loop invariants automatically.

Package Management ([`package/`](package/)). Content-addressed storage using BLAKE3 [hashing](package/hash.rs) with [Poseidon2](package/poseidon2.rs) for in-proof verification. The [definitions store](package/store.rs) manages a local codebase of named, versioned definitions. The [registry](package/registry.rs) provides an HTTP server and client for publishing and pulling definitions.

## Design Principles

Direct mapping. Every language construct maps to a known instruction pattern. The compiler is a thin translation layer, not an optimization engine. This makes proving costs predictable and the compiler auditable.

Target abstraction. The [`StackLowering`](tir/lower/mod.rs) trait and [`CostModel`](cost/model/mod.rs) trait isolate all target-specific knowledge. Adding a new backend means implementing these two traits — the rest of the compiler is shared.

Re-exports for stability. [`lib.rs`](lib.rs) re-exports every module at the crate root so that internal reorganization does not break downstream code or the binary crate.
