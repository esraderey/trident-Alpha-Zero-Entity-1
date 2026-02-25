# Trident Audit — 2026-02-16

9 parallel audits, 210 files, 12 passes each.
151 findings: 18 critical, 68 major, 65 minor.

## Top Critical

1. **Symbolic verifier field arithmetic wrong** (F1-SYM, F2-SYM) — wrapping_add/sub on u64
2. **bigint.tri mul_mod truncates to 256 bits** (TRI-MULMOD) — cascades to ed25519, secp256k1
3. **Registry HTTP no body size cap** (P7-HTTP-1/2) — OOM via unlimited Content-Length
4. **NTT 3 bugs** (TRI-NTT) — wrong bounds, twiddle, cyclic mode
5. **Poseidon2 reduce128 may truncate** (P3-RED) — u128→u64 edge case

## Systemic

- **HashSet determinism** (9 locations): typecheck, analysis, TIR builder, SMT, resolve, scaffold, store, LIR
- **Files >500 lines** (6): grammar/trident.rs, tir/stack.rs, tir/optimize.rs, tir/builder/{mod,stmt,tests}.rs

## Full Log

See git history for detailed per-finding breakdown (was .claude/audits/2026-02-16.md).
