# Trident Audit Summary — 2026-02-16

## Scope

| Audit                     | Modules                                       | Files |
|---------------------------|-----------------------------------------------|-------|
| ast-typecheck             | src/ast/, src/typecheck/                      | 13    |
| cli-deploy-api            | src/cli/, src/deploy/, src/api/               | 31    |
| cost-verify               | src/cost/, src/verify/                        | 29    |
| ir                        | src/ir/ (KIR, TIR, LIR, Tree)                | 23    |
| lsp                       | src/lsp/                                      | 18    |
| package                   | src/package/ (store, registry, hash, poseidon)| 23    |
| root-config-diagnostic    | src/lib.rs, main.rs, diagnostic/, config/     | 12    |
| syntax                    | src/syntax/ (lexer, parser, format, grammar)  | 20    |
| tri-sources               | vm/, std/, os/ (.tri files)                   | 41    |
| **TOTAL**                 |                                               |**210**|

## Findings

| Severity  | Count |
|-----------|-------|
| Critical  |   18  |
| Major     |   68  |
| Minor     |   65  |
| **Total** |**151**|

## Top 5 Critical

### 1. Symbolic verifier field arithmetic is wrong (F1-SYM, F2-SYM)
`verify/sym/mod.rs:93,117` — `wrapping_add`/`wrapping_sub` on u64 produce
wrong Goldilocks results on overflow. All symbolic verification unreliable.
Fix: u128-widened arithmetic or call field_add/field_sub from solve/eval.rs.

### 2. bigint.tri mul_mod truncates to 256 bits (TRI-MULMOD)
`std/crypto/bigint.tri` — 256-bit modular multiplication mathematically wrong.
Cascades to ed25519, secp256k1. All elliptic curve verification broken.
Fix: full 512-bit multiplication with Barrett or Montgomery reduction.

### 3. Registry HTTP client has no body size cap (P7-HTTP-1, P7-HTTP-2)
`package/registry/client.rs:188,194` — malicious registry causes OOM via
unlimited Content-Length or chunked data. Fix: MAX_BODY_SIZE constant.

### 4. NTT has 3 separate bugs (TRI-NTT)
`std/private/poly.tri` — wrong loop bounds, wrong twiddle progression,
standard NTT where negacyclic required. All FHE polynomial math broken.
Fix: rewrite Cooley-Tukey butterfly with negacyclic roots.

### 5. Poseidon2 reduce128 may truncate (P3-RED)
`package/poseidon2.rs:54` — second-round reduction truncates u128 to u64
for extreme inputs. All content hashes potentially affected.
Fix: exhaustive edge-case tests; add third reduction round if confirmed.

## Systemic Issues

**HashSet determinism violations** (9 locations): typecheck/mod.rs,
typecheck/analysis.rs, ir/tir/builder/mod.rs, verify/smt/mod.rs,
config/resolve/mod.rs, config/resolve/resolver.rs, config/scaffold/mod.rs,
store/deps.rs, ir/lir/mod.rs (Hash derive).

**Files exceeding 500-line limit** (6): grammar/trident.rs (594),
ir/tir/stack.rs (506), ir/tir/optimize.rs (633), ir/tir/builder/mod.rs (529),
ir/tir/builder/stmt.rs (502), ir/tir/builder/tests.rs (654).

## Clean Modules (zero findings)

src/lib.rs, src/main.rs, syntax/span.rs, syntax/lexeme.rs,
syntax/parser/stmts.rs, syntax/parser/items.rs, syntax/grammar/mod.rs,
syntax/grammar/dsl.rs, syntax/format/expr.rs, syntax/format/stmts.rs,
syntax/format/items.rs, ir/mod.rs, ir/kir/mod.rs,
ir/tir/builder/helpers.rs, ir/tree/lower/mod.rs, api/pipeline.rs,
vm/core/assert.tri, vm/core/convert.tri, vm/io/io.tri, vm/io/mem.tri,
std/crypto/auth.tri, std/crypto/merkle.tri, std/io/storage.tri,
std/target.tri, os/neptune/kernel.tri, os/neptune/utxo.tri,
os/neptune/xfield.tri, os/neptune/recursive.tri,
os/neptune/programs/proof_aggregator.tri,
os/neptune/programs/proof_relay.tri, os/neptune/types/native_currency.tri.
