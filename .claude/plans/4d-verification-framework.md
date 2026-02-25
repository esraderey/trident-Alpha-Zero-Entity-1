# 4D Verification Framework — IMPLEMENTED

Four code producers (Rust reference, classic compiler, hand TASM, neural TASM)
compared on four metrics (correctness, execution speed, proving time,
verification time) via `trident bench --full`.

Layout after repo restructure:
- `baselines/triton/*.tasm` — hand-written TASM
- `benches/references/*.rs` — Rust ground truth
- `benches/harnesses/*.tri` + `*.inputs` — live execution programs

Metrics via trisha: `trisha run` (correctness + cycles), `trisha prove`
(proving time), `trisha verify` (verification time).

See CLAUDE.md "Verification Framework" for current spec.
