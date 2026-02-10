# Trident LLM Reference

Machine-optimized reference for AI code generation. Version 0.1.

This document is structured for LLM consumption: each section is self-contained,
uses consistent formatting, and includes complete code patterns. When generating
Trident code, use this as the authoritative reference.

## LANGUAGE IDENTITY

- Name: Trident
- Extension: `.tri`
- Paradigm: Imperative, bounded, first-order, no heap, no recursion
- Domain: Zero-knowledge provable computation
- Field: Goldilocks (p = 2^64 - 2^32 + 1) on Triton VM target
- Compiler: `trident build <file.tri>`
- All arithmetic is modular (mod p). There is no subtraction operator.

## FILE STRUCTURE

Every `.tri` file starts with exactly one of:
```
program <name>      // Executable (has fn main)
module <name>       // Library (no fn main)
```

Then imports, then items (functions, structs, constants, events).

```
program my_program

use std.crypto.hash
use std.io.mem

const MAX: U32 = 32

struct Point {
    x: Field,
    y: Field,
}

event Transfer {
    from: Digest,
    to: Digest,
    amount: Field,
}

fn helper(a: Field) -> Field {
    a + 1
}

fn main() {
    let x: Field = pub_read()
    pub_write(helper(x))
}
```

## TYPES (complete list)

| Type | Width | Description |
|------|-------|-------------|
| `Field` | 1 | Field element mod p |
| `Bool` | 1 | Constrained to {0, 1} |
| `U32` | 1 | Range-checked 0..2^32 |
| `XField` | 3 | Extension field (Triton) |
| `Digest` | 5 | Hash digest [Field; 5] |
| `[T; N]` | N*w | Fixed array, N compile-time |
| `(T1, T2)` | w1+w2 | Tuple |
| `struct S` | sum | Named product type |

NO: enums, sum types, references, pointers, strings, floats, Option, Result.
NO: implicit conversions between types.

## OPERATORS (complete list)

| Op | Types | Result | Notes |
|----|-------|--------|-------|
| `a + b` | Field,Field | Field | Addition mod p |
| `a * b` | Field,Field | Field | Multiplication mod p |
| `a == b` | Field,Field | Bool | Equality test |
| `a < b` | U32,U32 | Bool | Less-than (U32 only) |
| `a & b` | U32,U32 | U32 | Bitwise AND |
| `a ^ b` | U32,U32 | U32 | Bitwise XOR |
| `a /% b` | U32,U32 | (U32,U32) | Divmod (quotient, remainder) |
| `a *. s` | XField,Field | XField | Scalar multiply |

NO: `-`, `/`, `!=`, `>`, `<=`, `>=`, `&&`, `||`, `!`, `%`, `>>`, `<<`.

To subtract: `sub(a, b)` or `a + neg(b)`.
To negate: `neg(a)`.
To invert: `inv(a)`.
To compare not-equal: `(a == b) == false` or negate the equality result.

## VARIABLE DECLARATIONS

```
let x: Field = 42              // Immutable
let mut counter: U32 = 0       // Mutable
let (hi, lo): (U32, U32) = split(x)  // Tuple destructuring
```

Type annotation is REQUIRED on let bindings.

## CONTROL FLOW

```
// If/else (NO else-if; nest instead)
if condition {
    body
} else {
    body
}

// Bounded for-loop (constant bounds)
for i in 0..32 {
    body  // exactly 32 iterations
}

// Bounded for-loop (runtime bound with declared max)
for i in 0..n bounded 64 {
    body  // at most 64 iterations
}

// Match (integer/bool patterns + wildcard only)
match value {
    0 => { handle_zero() }
    1 => { handle_one() }
    _ => { handle_other() }
}

// Return (explicit or tail expression)
fn foo(x: Field) -> Field {
    if x == 0 { return 1 }
    x + x          // tail expression = return value
}
```

NO: `while`, `loop`, `break`, `continue`, `else if`, recursion.

## FUNCTIONS

```
// Private function
fn add(a: Field, b: Field) -> Field {
    a + b
}

// Public function (visible to other modules)
pub fn add(a: Field, b: Field) -> Field {
    a + b
}

// Size-generic function
fn sum<N>(arr: [Field; N]) -> Field {
    let mut total: Field = 0
    for i in 0..N {
        total = total + arr[i]
    }
    total
}

// No return value
fn validate(x: Field) {
    assert(x == 42)
}

// Test function
#[test]
fn test_add() {
    assert_eq(add(1, 2), 3)
}

// Conditional compilation
#[cfg(debug)]
fn debug_helper() { }
```

NO: closures, function pointers, generics on types (only size generics),
default parameters, variadic arguments, method syntax.

## BUILTIN FUNCTIONS (complete list)

### I/O
```
pub_read() -> Field                 // Read 1 public input
pub_read2() -> (Field, Field)       // Read 2
pub_read3() -> (Field, Field, Field) // Read 3
pub_read5() -> Digest               // Read 5 (as Digest)
pub_write(v: Field)                 // Write 1 public output
pub_write2(a: Field, b: Field)      // Write 2
pub_write5(a..e: Digest)            // Write 5
divine() -> Field                   // Read 1 secret (nondeterministic) input
divine3() -> (Field, Field, Field)  // Read 3 secret inputs
divine5() -> Digest                 // Read 5 secret inputs (as Digest)
```

### Field arithmetic
```
sub(a: Field, b: Field) -> Field    // Subtraction: a - b (mod p)
neg(a: Field) -> Field              // Negation: p - a
inv(a: Field) -> Field              // Multiplicative inverse: 1/a (mod p)
```

### U32 operations
```
split(a: Field) -> (U32, U32)       // Split to (hi, lo) u32 pair
as_u32(a: Field) -> U32             // Range-checked cast
as_field(a: U32) -> Field           // Type cast (zero cost)
log2(a: U32) -> U32                 // Floor of log base 2
pow(base: U32, exp: U32) -> U32     // Exponentiation
popcount(a: U32) -> U32             // Hamming weight
```

### Hash
```
hash(a,b,c,d,e,f,g,h,i,j) -> Digest  // Tip5 hash of 10 fields
sponge_init()                          // Initialize sponge
sponge_absorb(a,b,c,d,e,f,g,h,i,j)   // Absorb 10 fields
sponge_absorb_mem(ptr: Field)          // Absorb from RAM
sponge_squeeze() -> [Field; 10]        // Squeeze 10 fields
```

### Merkle tree
```
merkle_step(idx: U32, d: Digest) -> (U32, Digest)
merkle_step_mem(ptr, idx, d) -> (Field, U32, Digest)
```

### Assertions
```
assert(cond: Bool)                       // Halt if false
assert_eq(a: Field, b: Field)            // Assert a == b
assert_digest(a: Digest, b: Digest)      // Assert digest equality
```

### RAM
```
ram_read(addr: Field) -> Field
ram_write(addr: Field, val: Field)
ram_read_block(addr: Field) -> [Field; 5]
ram_write_block(addr: Field, vals: [Field; 5])
```

### Extension field (Triton only)
```
xfield(x0: Field, x1: Field, x2: Field) -> XField
xinvert(a: XField) -> XField
xx_dot_step(acc: XField, ptr_a: Field, ptr_b: Field) -> (XField, Field, Field)
xb_dot_step(acc: XField, ptr_a: Field, ptr_b: Field) -> (XField, Field, Field)
```

## STRUCTS

```
struct Config {
    max_depth: U32,
    root: Digest,
    owner: Field,
}

pub struct PubConfig {
    pub max_depth: U32,
    pub root: Digest,
}

// Construction
let cfg: Config = Config { max_depth: 32, root: my_digest, owner: 0 }

// Field access
let d: U32 = cfg.max_depth

// Destructuring is NOT supported; access fields individually.
```

## EVENTS

```
event Transfer {
    from: Digest,
    to: Digest,
    amount: Field,
}

// Open emit (public, fields written to output)
emit Transfer { from: sender, to: receiver, amount: value }

// Sealed emit (private, hash commitment written to output)
seal Transfer { from: sender, to: receiver, amount: value }
```

## SPECIFICATION ANNOTATIONS

```
#[requires(amount > 0)]
#[requires(balance >= amount)]
#[ensures(result == balance - amount)]
fn withdraw(balance: Field, amount: Field) -> Field {
    sub(balance, amount)
}
```

- `#[requires(P)]` — precondition that must hold on entry
- `#[ensures(P)]` — postcondition that must hold on exit
- Use `result` to refer to the return value in ensures clauses
- Verified with `trident verify`

## INLINE ASSEMBLY

```
// Basic (zero net stack effect)
asm { dup 0 add }

// With stack effect annotation
asm(+1) { push 42 }
asm(-2) { pop 1 pop 1 }

// Target-tagged
asm(triton) { dup 0 add }
asm(triton)(+1) { push 42 }
```

## MODULES AND IMPORTS

```
// In module file (std/crypto/hash.tri):
module std.crypto.hash

pub fn hash_pair(a: Digest, b: Digest) -> Digest {
    // implementation
}

// In program file:
program my_program

use std.crypto.hash

fn main() {
    let h: Digest = hash.hash_pair(a, b)
}
```

Import with `use <module_path>`. Access with `<short_name>.<function>`.

## COMMON PATTERNS

### Read inputs, compute, write output
```
program example

fn main() {
    let a: Field = pub_read()
    let b: Field = pub_read()
    let result: Field = a + b
    pub_write(result)
}
```

### Merkle proof verification
```
program verify_proof

use std.crypto.merkle

fn main() {
    let root: Digest = divine5()
    let leaf: Digest = divine5()
    let index: U32 = as_u32(pub_read())
    let depth: U32 = as_u32(pub_read())

    // Verify Merkle authentication path
    let mut node: Digest = leaf
    let mut idx: U32 = index
    for i in 0..32 bounded 32 {
        if as_field(i) < as_field(depth) {
            let sibling: Digest = divine5()
            let result: (U32, Digest) = merkle_step(idx, node)
            idx = result.0
            node = result.1
        }
    }

    assert_digest(node, root)
    pub_write(as_field(index))
}
```

### Hash computation with sponge
```
fn hash_many(values: [Field; 20]) -> Digest {
    sponge_init()
    sponge_absorb(
        values[0], values[1], values[2], values[3], values[4],
        values[5], values[6], values[7], values[8], values[9]
    )
    sponge_absorb(
        values[10], values[11], values[12], values[13], values[14],
        values[15], values[16], values[17], values[18], values[19]
    )
    let squeezed: [Field; 10] = sponge_squeeze()
    hash(
        squeezed[0], squeezed[1], squeezed[2], squeezed[3], squeezed[4],
        squeezed[5], squeezed[6], squeezed[7], squeezed[8], squeezed[9]
    )
}
```

### Token balance transfer
```
fn transfer(sender_bal: Field, receiver_bal: Field, amount: Field) -> (Field, Field) {
    // Range check: amount fits in u32
    let amt: U32 = as_u32(amount)
    let sbal: U32 = as_u32(sender_bal)

    // Verify sufficient balance
    assert(amount < sender_bal + 1)

    // Compute new balances
    let new_sender: Field = sub(sender_bal, amount)
    let new_receiver: Field = receiver_bal + amount

    (new_sender, new_receiver)
}
```

### Accumulator with bounded loop
```
fn sum_array<N>(arr: [Field; N]) -> Field {
    let mut total: Field = 0
    for i in 0..N {
        total = total + arr[i]
    }
    total
}

fn dot_product<N>(a: [Field; N], b: [Field; N]) -> Field {
    let mut total: Field = 0
    for i in 0..N {
        total = total + a[i] * b[i]
    }
    total
}
```

### Conditional logic (no else-if)
```
fn classify(x: U32) -> Field {
    if x < 10 {
        1
    } else {
        if x < 100 {
            2
        } else {
            if x < 1000 {
                3
            } else {
                4
            }
        }
    }
}
```

### Using match
```
fn dispatch(op: Field, a: Field, b: Field) -> Field {
    match op {
        0 => { a + b }
        1 => { a * b }
        2 => { sub(a, b) }
        _ => {
            assert(false)
            0
        }
    }
}
```

## ERRORS TO AVOID

1. WRONG: `a - b` -- No subtraction operator. Use `sub(a, b)`.
2. WRONG: `a / b` -- No division operator. Use `a * inv(b)`.
3. WRONG: `a != b` -- No not-equal operator. Use `(a == b) == false`.
4. WRONG: `a > b` -- No greater-than for Field. For U32: `b < a`.
5. WRONG: `while condition { }` -- No while loops. Use bounded `for`.
6. WRONG: `let x = 5` -- Type annotation required: `let x: Field = 5`.
7. WRONG: `else if` -- Not supported. Nest `if` inside `else { if ... }`.
8. WRONG: `fn foo(x: Field) { return x }` -- Return type missing: `-> Field`.
9. WRONG: recursive function calls -- Not allowed. All call graphs must be acyclic.
10. WRONG: `let x: Bool = 1` -- Type mismatch. Use `let x: Bool = true`.
11. WRONG: `pub_read() + pub_read()` -- Side effects in expressions: bind to let first.
12. WRONG: unbounded `for i in 0..n { }` -- Must declare bound: `bounded N`.

## CLI COMMANDS

```
trident build <file>              # Compile to TASM
trident build <file> --costs      # With cost analysis
trident build <file> --annotate   # Per-line cost annotations
trident build <file> --hints      # Optimization hints
trident check <file>              # Type-check only
trident fmt <file>                # Format source
trident fmt <dir> --check         # Check formatting
trident test <file>               # Run #[test] functions
trident verify <file>             # Symbolic verification
trident verify <file> --json      # JSON verification report
trident verify <file> --z3        # Formal verification via Z3
trident doc <file>                # Generate docs
trident hash <file>               # Show function content hashes
trident generate <spec.tri>       # Generate scaffold from spec
trident init <name>               # Create new project
trident lsp                       # Start language server
```

## PROJECT STRUCTURE

```
my_project/
  trident.toml        # Project manifest
  main.tri            # Entry point (program)
  utils.tri           # Helper module
  std/                # Standard library (auto-resolved)
  ext/triton/         # Triton-specific extensions
```

trident.toml:
```toml
[project]
name = "my_project"
version = "0.1.0"
entry = "main.tri"
```

## COST MODEL (Triton VM)

Every instruction has a deterministic cost in table rows:
- `+`, `*`, `==`: 1 processor row
- `hash`, `sponge_*`: 6 hash rows (dominant)
- `<`, `&`, `^`, `split`, `/%`: 33 u32 rows (dominant)
- Function call+return: 2 jump_stack rows
- RAM read/write: 1 ram row each

Proving cost is determined by the TALLEST table, padded to the next power of 2.
Minimize the dominant table, not total instructions.

Use `trident build --costs --hotspots` to identify the dominant table and top
cost contributors.
