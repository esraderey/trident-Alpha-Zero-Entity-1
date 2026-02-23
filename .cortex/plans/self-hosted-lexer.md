# Self-Hosted Lexer: Proven Tokenization

## Context

North star: the compiler proves its own compilation. Self-hosting is
the forcing function — it forces the language to handle general
computation, forces the codegen to be good (you eat your own cooking),
and makes multi-target a consequence (compiler written in .tri compiles
to any backend).

The lexer is the first component. It's small (458 LOC Rust), well-defined
(bytes in → tokens out), and exercises the right primitives: RAM iteration,
byte classification, state machines, bounded loops.

When compiled and executed on Triton VM, the lexer produces a STARK proof
that the tokenization was correct. Proven lexing — something that doesn't
exist anywhere else.

## Design

### Memory Layout

Source bytes in RAM as Field values [0-255]. Tokens in RAM as 4-word structs.
All addresses passed as parameters — no hardcoded memory regions.

```
RAM[src_base .. src_base + src_len]     Source bytes (one Field per byte)
RAM[tok_base .. tok_base + n*4]         Token output (kind, start, end, int_val)
RAM[err_base .. err_base + n*3]         Error output (code, start, end)
RAM[state_base .. state_base + 8]       Lexer state (pos, counts, base addrs)
```

Token struct: `{kind: Field, start: Field, end: Field, int_val: Field}`.
Identifiers are `(start, end)` references into source — no string copies.
Errors are `(code, start, end)` — no diagnostic strings.

### Token Kind Constants

51 variants as Field constants (1-56). Keywords 1-22, type keywords 23-27,
symbols 28-52, Integer=53, Ident=54, AsmBlock=55, Eof=56.
Error codes 100+.

### Keyword Recognition: Length-First Trie

Branch on identifier length, then first byte, then remaining bytes.
All 28 keywords (22 + 5 types + `_`) have unique signatures at depth ≤3.

Two collisions to resolve:
- Length 2, first='i' (105): `if` vs `in` — disambiguate on second byte
- Length 6, first='r' (114): `return` vs `reveal` — same second byte 'e',
  disambiguate on third: 't' (116) vs 'v' (118)

All byte comparisons use Field equality (`==`), which is a single `eq`
instruction on Triton VM. No U32 conversion needed for byte matching.

### Loop Pattern: Done-Flag in RAM

Trident has no `break`. Every `for i in 0..n bounded K` runs K iterations.
Pattern: store a `done` flag in scratch RAM; each iteration checks it first,
skips work if set.

The main whitespace+comment skipper uses a 3-state machine in a single
bounded loop: `{0: scanning, 1: inside comment, 2: done}`. One byte
processed per iteration, at most `MAX_SRC_LEN` iterations total.

### Field Subtraction

`end - start` = `end + field.neg(start)`. Correct for small non-negative
integers where `end >= start`. Digit parsing: `byte + field.neg(48)`.

## Public API

```
// std/compiler/lexer.tri
pub fn lex(
    src_base: Field,      // RAM address of source bytes
    src_len: Field,        // number of source bytes
    tok_base: Field,       // RAM address for token output
    err_base: Field,       // RAM address for error output
    state_base: Field      // RAM address for lexer state (8 words)
)
// After: tok_count = mem.read(state_base + 1)
//        err_count = mem.read(state_base + 2)
//        token[i]  = RAM[tok_base + i*4 .. tok_base + i*4 + 3]
```

## Files to Create

### 1. `std/compiler/lexer.tri` (~600-800 lines)

Module: `std.compiler.lexer`. Imports: `vm.core.field`, `vm.core.convert`,
`vm.core.assert`, `vm.io.mem`.

Functions:
- `lex()` — main entry, initializes state, runs main loop
- `skip_ws_and_comments()` — single-loop 3-state machine
- `scan_ident_or_keyword()` — collect ident bytes, classify
- `classify_keyword()` — length-first trie (~200 lines of if/else)
- `scan_number()` — collect digits, parse via multiply-add
- `scan_symbol()` — single/two-char dispatch + error cases
- `scan_asm_block()` — annotation parsing + brace-depth tracking
- Helpers: `is_whitespace`, `is_alpha`, `is_digit`, `is_ident_start`,
  `is_ident_continue`, `src_byte`, `get_pos`, `set_pos`, `emit_token`,
  `emit_error`, `parse_integer`

### 2. `benches/std/compiler/lexer.reference.rs`

Rust ground truth. Takes a test source string, tokenizes with both:
- Trident's Rust lexer (`Lexer::new(source).tokenize()`)
- Converts each Lexeme to TK_* integer

Outputs: `rust_ns: N` on stdout. Emits `values:` and expected token
sequence on stderr for .inputs file generation.

### 3. `benches/std/compiler/lexer_bench.tri`

Benchmark program:
```
program lexer_bench
use vm.io.io, vm.io.mem, vm.core.convert, vm.core.assert
use std.compiler.lexer

fn main() {
    // Read params: src_base, src_len, tok_base, err_base,
    //              state_base, expected_tok_count
    // Load source bytes from public input into RAM
    // Call lexer.lex(...)
    // Assert tok_count == expected
    // io.write(tok_count)
}
```

### 4. `benches/std/compiler/lexer.inputs`

Test source: a small Trident program (~100-200 bytes).
Format: `values: src_base, src_len, tok_base, err_base, state_base,
expected_tok_count, byte0, byte1, ...`

### 5. `Cargo.toml` — register example

```toml
[[example]]
name = "ref_std_compiler_lexer"
path = "benches/std/compiler/lexer.reference.rs"
```

### 6. `tests/audit_stdlib.rs` — compilation test

```rust
#[test]
fn test_std_compiler_lexer_compiles() {
    // Compile a program that uses std.compiler.lexer
    // Assert __lex: appears in TASM output
}
```

## Implementation Order

### Step 1: Skeleton + byte classifiers (1 session)

Create `std/compiler/lexer.tri` with:
- All TK_* and ERR_* constants
- Helper functions: `is_whitespace`, `is_alpha`, `is_digit`,
  `is_ident_start`, `is_ident_continue`
- RAM accessors: `src_byte`, `get_pos`, `set_pos`, `emit_token`,
  `emit_error`, `parse_integer`
- Stub `lex()` that just emits Eof

Verify: `trident build std/compiler/lexer.tri` compiles.
Add compilation test to `audit_stdlib.rs`. `cargo test`.

### Step 2: Keyword trie (1 session)

Implement `classify_keyword(state_base, start, len) -> Field`.
~200-300 lines of nested if/else branching on length, first byte,
remaining bytes. All 28 keywords covered.

### Step 3: Scanners (1 session)

- `skip_ws_and_comments()` — single-loop state machine
- `scan_ident_or_keyword()` — collect + classify
- `scan_number()` — collect digits + parse_integer
- `scan_symbol()` — 15 single-char + 5 two-char dispatches
- Wire into `lex()` main loop

### Step 4: ASM blocks (1 session)

- `scan_asm_block()` — annotation parsing, brace depth
- `scan_effect_number()` — +N / -N parsing
- Handle all 4 forms: bare, effect, target, target+effect

### Step 5: Reference + bench (1 session)

- `lexer.reference.rs` — Rust ground truth + input generation
- `lexer_bench.tri` — benchmark program
- `lexer.inputs` — test data for a small program
- Register example in Cargo.toml
- End-to-end: `trident bench benches/std/compiler`

### Step 6: Prove it (1 session)

- Run through trisha: execute, prove, verify
- Fix any assert failures (divine values if needed — lexer is
  pure computation, should need none)
- Self-application test: feed `lexer.tri` as input to itself
- Commit + update .cortex

## Verification

- `cargo check` — zero warnings
- `cargo test` — all pass (incl. new compilation test)
- `trident build std/compiler/lexer.tri` — compiles to TASM
- `cargo run --example ref_std_compiler_lexer` — Rust ground truth
- `trident bench benches/std/compiler` — instruction counts + ratio
- `trident bench benches/std/compiler --full` — exec, prove, verify PASS
- Token output matches Rust lexer for all 21 test cases
- Self-application: lexer tokenizes its own source correctly

## Critical Files

- `src/syntax/lexer/mod.rs` — Rust lexer (canonical behavior)
- `src/syntax/lexeme.rs` — 51 lexeme variants + from_keyword()
- `src/syntax/lexer/tests.rs` — 21 test cases (ground truth)
- `std/nn/tensor.tri` — RAM iteration + scratch state pattern
- `std/trinity/inference.tri` — complex RAM parameter passing
- `benches/std/trinity/` — reference + bench + inputs pattern
- `vm/core/field.tri` — field.sub, field.neg for subtraction
- `vm/core/convert.tri` — as_u32, as_field, split
