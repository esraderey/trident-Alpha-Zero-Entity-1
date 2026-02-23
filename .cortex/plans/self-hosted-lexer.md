# Self-Hosted Lexer: COMPLETE

First self-hosting component. Trident lexer written in Trident,
compiled to TASM, proven correct via STARK on Triton VM.

## Result

- `std/compiler/lexer.tri` — 824 lines, 51 token kinds, 28 keywords
- `trident bench --full` → execute 39ms, prove 10.2s, verify 42ms — PASS
- 229-byte test program → 69 tokens, all verified against Rust lexer
- No divine values needed (pure computation)

## Files

| File | Purpose |
|------|---------|
| `std/compiler/lexer.tri` | Self-hosted lexer (main deliverable) |
| `benches/std/compiler/lexer.reference.rs` | Rust ground truth |
| `benches/std/compiler/lexer_bench.tri` | Benchmark program |
| `benches/std/compiler/lexer.inputs` | Test data (229 bytes, 69 tokens) |
| `benches/std/compiler/lexer.baseline.tasm` | Placeholder baseline |
| `tests/audit_stdlib.rs` | Compilation test added |

## Design Decisions

- RAM-based: source bytes + token structs in RAM, all addrs as params
- Length-first trie for keyword recognition (28 keywords, depth ≤3)
- Done-flag pattern for bounded loops (no break in Trident)
- 3-state machine for whitespace+comment skipper
- Field subtraction via `a + field.neg(b)`

## Open

- Self-application (lexer tokenizing own 30KB source) needs larger
  bounds or streaming — deferred to parser phase
- Hand TASM baseline is placeholder stubs — optimize after parser
- Next component: self-hosted parser
