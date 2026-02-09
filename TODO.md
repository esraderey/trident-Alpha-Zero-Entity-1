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
- [ ] Restrict #[intrinsic] to std modules only (spec requirement)
- [ ] Recursion detection across all modules (spec: compiler rejects call cycles)
- [ ] Dead code detection (spec: unreachable after unconditional halt/assert)
- [ ] Unused import warnings
- [ ] Multi-width array element support (emit.rs:585 TODO)
- [ ] Runtime index access for arrays (emit.rs:600 TODO)
- [ ] Module constant resolution (emit.rs:447 TODO)
- [ ] Deep variable access beyond stack (emit.rs:475 TODO)
- [ ] Struct field access in emitter (emit.rs:567 TODO)
- [ ] sec ram declarations (parsed but not emitted)
- [ ] Power-of-2 boundary proximity warnings (spec section 12.6)
- [ ] Optimization hints (spec section 13.10 — H0001..H0004)

## CLI

- [ ] `trident init` — scaffold new project/library
- [ ] `trident test` — testing framework for .tri programs
- [ ] `trident doc` — documentation generation with cost annotations
- [ ] `trident build --annotate` — per-line cost annotations in source
- [ ] `trident build --compare` — compare function costs

## LSP

- [ ] Go-to-definition
- [ ] Hover (show type + cost)
- [ ] Completions (keywords, builtins, module members)
- [ ] Document symbols (outline)
- [ ] Signature help (function parameter hints)

## Tests

83 tests across 10 files. Missing coverage for:
- [ ] Formatter (format.rs — 0 tests)
- [ ] Diagnostic rendering (diagnostic.rs — 0 tests)
- [ ] LSP server (trident-lsp.rs — 0 tests)
- [ ] Multi-module type checking (check_file_in_project)
- [ ] Edge cases: deeply nested expressions, max stack depth
- [ ] Round-trip: parse -> format -> parse produces same AST
- [ ] Error message quality audit

## Token / Applications

- [ ] Token factory as registry for token deploy
- [ ] Prove language correctness and compiler implementation
- [ ] Library for browser extension integration
- [ ] Browser extension

## Documentation / Website

- [ ] README with quick start
- [ ] Language spec (clean up spec.md for public)
- [ ] Language tutorial / docs
- [ ] Web playground
- [ ] Landing page
- [ ] Extension download links
