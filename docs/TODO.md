# Trident TODO

## Done

- [x] Editor support: Zed, Helix, VS Code/Cursor
- [x] Tree-sitter grammar with highlights
- [x] LSP: diagnostics + formatting
- [x] LSP: multi-module project-aware type checking
- [x] Formatter: 80-col, comment preservation, idempotent
- [x] CLI: build, check, fmt
- [x] Cost analysis: all 6 Triton VM tables
- [x] Events system: emit (open) + seal (hashed)
- [x] Multi-module compilation with trident.toml
- [x] Stack spilling to RAM

## Compiler

- [x] Standard library as .tri modules with #[intrinsic]
      13 modules: std.io, std.hash, std.field, std.convert, std.u32,
      std.xfield, std.mem, std.assert, std.merkle, std.auth,
      std.kernel, std.utxo, std.storage
- [x] Digest destructuring: let (f0, f1, f2, f3, f4) = digest
      Unlocks Merkle verification, kernel field auth, and proper
      hash-preimage binding in token verify_auth
- [x] Neptune blockchain stdlib: std.merkle (verify1-4, authenticate_leaf3),
      std.kernel (MAST tree auth), std.auth (lock script patterns),
      std.utxo, std.storage
- [x] Programming model documentation (docs/programming-model.md)
- [x] Restrict #[intrinsic] to std modules only (spec requirement)
- [x] Recursion detection across all modules (spec: compiler rejects call cycles)
- [x] Module constant resolution (cross-module pub const)
- [x] Struct field access in emitter (type-annotation-based layout)
- [x] Dead code detection (spec: unreachable after unconditional halt/assert)
- [x] Unused import warnings
- [x] Multi-width array element support (emit.rs:585 TODO)
- [x] Runtime index access for arrays (emit.rs:600 TODO)
- [x] Deep variable access beyond stack (emit.rs:475 TODO)
- [x] sec ram declarations (parsed but not emitted)
- [x] Power-of-2 boundary proximity warnings (spec section 12.6)
- [x] Optimization hints H0001-H0004 (spec section 13.10)
      H0001: hash table dominance warning
      H0002: headroom hint (room to grow at zero cost)
      H0003: redundant as_u32 range check detection
      H0004: loop bound waste (declared bound >> actual iterations)
- [x] Surface type-checker warnings in build/check output

## CLI

- [x] `trident init` — scaffold new project/library
- [x] `trident fmt` — directory/project support with recursive .tri discovery
- [x] `trident build --hints` — show optimization hints after build
- [x] `trident lsp` — start LSP server as subcommand
- [ ] `trident test` — testing framework for .tri programs
- [ ] `trident doc` — documentation generation with cost annotations
- [ ] `trident build --annotate` — per-line cost annotations in source
- [ ] `trident build --compare` — compare function costs

## LSP

- [x] Go-to-definition (project-wide symbol index)
- [x] Hover (type signatures for builtins, functions, structs, constants)
- [x] Completions (keywords, types, builtins, dot-triggered module members)
- [x] Document symbols (outline for functions, structs, constants, events)
- [ ] Signature help (function parameter hints)
- [ ] Hover: show cost alongside type

## Tests

197 tests across 14 files:
- [x] Formatter (format.rs — 29 tests)
- [x] Diagnostic rendering (diagnostic.rs — 7 tests)
- [x] LSP helpers (lsp.rs — 27 tests)
- [x] Multi-module type checking (check_silent, check_file_in_project)
- [x] Edge cases: deep nesting, max stack depth, 18-var spilling
- [x] Round-trip: parse -> format -> parse preserves AST
- [x] XField operations, all binary operators
- [ ] Error message quality audit

## Documentation / Website

- [ ] README with quick start
- [ ] Language spec (clean up spec.md for public)
- [ ] Language tutorial / docs
- [ ] Web playground
- [ ] Landing page
- [ ] Extension download links

## Standards

- [ ] Token factory as registry for token deploy
- [ ] Prove language correctness and compiler implementation
- [ ] Library for browser extension integration
- [ ] Browser extension
