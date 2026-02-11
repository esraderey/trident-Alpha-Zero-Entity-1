# Error Catalog

All Trident compiler diagnostics — errors, warnings, and optimization hints.

For the language reference see [language.md](language.md). For target details
see [targets.md](targets.md).

---

## Lexer Errors

### Unexpected character

```
error: unexpected character '@' (U+0040)
  help: this character is not recognized as part of Trident syntax
```

A character outside the Trident grammar was found. Source files must be ASCII.

**Fix:** Remove the character. Check for copy-paste artifacts or encoding issues.

---

### No subtraction operator

```
error: unexpected '-'; Trident has no subtraction operator
  help: use the `sub(a, b)` function instead of `a - b`
```

Trident deliberately omits `-` (see [language.md](language.md) Section 4).
Subtraction in a prime field is addition by the additive inverse. Making it
explicit prevents the `(1 - 2) == p - 1` footgun.

**Fix:**

```
let diff: Field = sub(a, b)
```

---

### No division operator

```
error: unexpected '/'; Trident has no division operator
  help: use the `/% (divmod)` operator instead: `let (quot, rem) = a /% b`
```

Field division is multiplication by the modular inverse. The `/%` operator
makes the cost explicit.

**Fix:**

```
let (quotient, remainder) = a /% b
```

---

### Integer too large

```
error: integer literal '999999999999999999999' is too large
  help: maximum integer value is 18446744073709551615
```

The literal exceeds `u64::MAX` (2^64 - 1).

**Fix:** Use a smaller value. Values are reduced modulo p at runtime.

---

### Unterminated asm block

```
error: unterminated asm block: missing closing '}'
  help: every `asm { ... }` block must have a matching closing brace
```

**Fix:** Add the closing `}`.

---

### Invalid asm annotation

```
error: expected ')' after asm annotation
  help: asm annotations: `asm(+1) { ... }`, `asm(triton) { ... }`, or `asm(triton, +1) { ... }`
```

The `asm` block has a malformed annotation.

**Fix:** Use one of the valid forms:

```
asm { ... }                     // zero effect, default target
asm(+1) { ... }                // effect only
asm(triton) { ... }            // target only
asm(triton, +1) { ... }        // target + effect
```

---

### Expected asm block body

```
error: expected '{' after `asm` keyword
  help: inline assembly syntax is `asm { instructions }` or `asm(triton) { instructions }`
```

**Fix:** Add `{ ... }` after the asm keyword or annotation.

---

## Parser Errors

### Expected program or module

```
error: expected 'program' or 'module' declaration at the start of file
  help: every .tri file must begin with `program <name>` or `module <name>`
```

**Fix:**

```
program my_app

fn main() { }
```

---

### Nesting depth exceeded

```
error: nesting depth exceeded (maximum 256 levels)
  help: simplify your program by extracting deeply nested code into functions
```

More than 256 levels of nested blocks. Extract inner logic into functions.

---

### Expected item

```
error: expected item (fn, struct, event, or const)
  help: top-level items must be function, struct, event, or const definitions
```

A top-level construct is not a valid item.

**Fix:** Only `fn`, `struct`, `event`, and `const` are valid at module scope.

---

### Expected type

```
error: expected type
  help: valid types are: Field, XField, Bool, U32, Digest, [T; N], (T, U), or a struct name
```

A type annotation contains something that is not a recognized type.

---

### Expected array size

```
error: expected array size (integer literal or size parameter name)
  help: array sizes are written as `N`, `3`, `M + N`, or `N * 2`
```

The array size expression is invalid.

---

### Expected expression

```
error: expected expression, found <token>
  help: expressions include literals (42, true), variables, function calls, and operators
```

---

### Invalid field pattern

```
error: expected field pattern (identifier, literal, or _)
  help: use `field: var` to bind, `field: 0` to match, or `field: _` to ignore
```

A struct pattern field has an invalid pattern.

---

### Attribute validation

```
error: #[intrinsic] can only be applied to functions
error: #[test] can only be applied to functions
error: #[pure] can only be applied to functions
error: #[requires] can only be applied to functions
error: #[ensures] can only be applied to functions
```

Attributes are only valid on function definitions.

---

## Type Errors

### Binary operator type mismatch

```
error: operator '+' requires both operands to be Field (or both XField), got Field and Bool
error: operator '==' requires same types, got Field and U32
error: operator '<' requires U32 operands, got Field and Field
error: operator '&' requires U32 operands, got Field and Field
error: operator '/%' requires U32 operands, got Field and Field
error: operator '*.' requires XField and Field, got Field and Field
```

Each operator has specific type requirements. See [language.md](language.md)
Section 4 for the operator table.

---

### Type mismatch in let binding

```
error: type mismatch: declared Field but expression has type Bool
```

The expression type does not match the declared type annotation.

---

### Type mismatch in assignment

```
error: type mismatch in assignment: expected Field but got Bool
```

---

### Cannot assign to immutable variable

```
error: cannot assign to immutable variable
  help: declare the variable with `let mut` to make it mutable
```

**Fix:**

```
let mut x: Field = 0
x = 42
```

---

### Undefined variable

```
error: undefined variable 'x'
  help: check that the variable is declared with `let` before use
```

---

### Undefined function

```
error: undefined function 'foo'
  help: check the function name and ensure the module is imported with `use`
```

---

### Function arity mismatch

```
error: function 'foo' expects 2 arguments, got 3
```

---

### Function argument type mismatch

```
error: argument 1 of 'foo': expected Field but got Bool
```

---

### Return type mismatch

```
error: function 'foo' declared return type Field, but body returns Bool
```

---

### Undefined struct

```
error: undefined struct 'Point'
  help: check the struct name spelling, or import the module that defines it
```

---

### Struct missing field

```
error: missing field 'y' in struct init
```

All fields must be provided in a struct literal.

---

### Struct unknown field

```
error: unknown field 'z' in struct 'Point'
```

---

### Struct field type mismatch

```
error: field 'x': expected Field but got Bool
```

---

### Field access on non-struct

```
error: field access on non-struct type Field
```

---

### Private field access

```
error: field 'secret' of struct 'Account' is private
```

**Fix:** Mark the field `pub` or provide a public accessor function.

---

### Index on non-array

```
error: index access on non-array type Field
```

---

### Array element type mismatch

```
error: array element type mismatch: expected Field got Bool
```

All elements of an array literal must have the same type.

---

### Tuple destructuring mismatch

```
error: tuple destructuring: expected 3 elements, got 2 names
```

---

### Digest destructuring mismatch

```
error: digest destructuring requires exactly D names, got N
```

The number of names in a digest destructuring must match the target's
digest width.

---

### Cannot destructure non-tuple

```
error: cannot destructure non-tuple type Field
```

---

### Tuple assignment mismatch

```
error: tuple assignment: expected 3 elements, got 2 names
```

---

### If condition type

```
error: if condition must be Bool or Field, got Digest
```

---

### Recursion detected

```
error: recursive call cycle detected: main -> foo -> main
  help: stack-machine targets do not support recursion; use loops (`for`) or iterative algorithms instead
```

Trident prohibits recursion because all target VMs require deterministic
trace lengths. Rewrite using `for` loops with `bounded`:

```
fn fib(n: Field) -> Field {
    let mut a: Field = 0
    let mut b: Field = 1
    for i in 0..n bounded 100 {
        let tmp: Field = b
        b = a + b
        a = tmp
    }
    a
}
```

---

### Unreachable code after return

```
error: unreachable code after return statement
  help: remove this code or move it before the return
```

---

## Control Flow Errors

### For loop without bounded

```
error: loop end must be a compile-time constant, or annotated with a bound
  help: use a literal like `for i in 0..10 { }` or add a bound: `for i in 0..n bounded 100 { }`
```

All loops must have compile-time-known or declared upper bounds for
deterministic trace length computation.

---

### Non-exhaustive match

```
error: non-exhaustive match: not all possible values are covered
  help: add a wildcard `_ => { ... }` arm to handle all remaining values
```

---

### Unreachable pattern after wildcard

```
error: unreachable pattern after wildcard '_'
  help: the wildcard `_` already matches all values; remove this arm or move it before `_`
```

---

### Match pattern type mismatch

```
error: integer pattern on Bool scrutinee; use `true` or `false`
error: Bool pattern on non-Bool scrutinee
```

---

### Struct pattern type mismatch

```
error: struct pattern `Point` does not match scrutinee type `Config`
```

---

### Unknown struct field in pattern

```
error: struct `Point` has no field `z`
```

---

## Size Generic Errors

### Size argument to non-generic function

```
error: function 'foo' is not generic but called with size arguments
```

**Fix:** Remove the angle bracket arguments.

---

### Size parameter count mismatch

```
error: function 'foo' expects 2 size parameters, got 1
```

---

### Cannot infer size argument

```
error: cannot infer size parameter 'N'; provide explicit size argument
```

**Fix:** Provide the size argument explicitly:

```
let result: Field = sum<5>(arr)
```

---

### Expected concrete size

```
error: expected concrete size, got 'N'
```

A size parameter could not be resolved to a concrete integer.

---

## Event Errors

### Undefined event

```
error: undefined event 'Transfer'
```

**Fix:** Declare the event before using `reveal` or `seal`:

```
event Transfer { from: Digest, to: Digest, amount: Field }
```

---

### Event field count limit

```
error: event 'BigEvent' has 12 fields, max is 9
```

Events are limited to 9 Field-width fields.

---

### Event field type restriction

```
error: event field 'data' must be Field type, got [Field; 3]
```

All event fields must be `Field` type.

---

### Missing event field

```
error: missing field 'amount' in event 'Transfer'
```

---

### Unknown event field

```
error: unknown field 'extra' in event 'Transfer'
```

---

## Annotation Errors

### #[intrinsic] restriction

```
error: #[intrinsic] is only allowed in std.*/ext.* modules, not in 'my_module'
```

The `#[intrinsic]` attribute is reserved for standard library and extension
modules shipped with the compiler. User code cannot use it.

---

### #[test] validation

```
error: #[test] function 'test_add' must have no parameters
error: #[test] function 'test_add' must not have a return type
```

Test functions take no arguments and return nothing.

---

### #[pure] I/O restriction

```
error: #[pure] function cannot call 'pub_read' (I/O side effect)
error: #[pure] function cannot use 'reveal' (I/O side effect)
error: #[pure] function cannot use 'seal' (I/O side effect)
```

Functions annotated `#[pure]` cannot perform any I/O operations.

---

## Module Errors

### Cannot find module

```
error: cannot find module 'helpers' (looked at 'path/to/helpers.tri'): No such file
  help: create the file 'path/to/helpers.tri' or check the module name in the `use` statement
```

---

### Circular dependency

```
error: circular dependency detected involving module 'a'
  help: break the cycle by extracting shared definitions into a separate module
```

---

### Duplicate function

```
error: duplicate function 'main'
```

---

### Cannot read entry file

```
error: cannot read 'main.tri': No such file or directory
  help: check that the file exists and is readable
```

---

## Target Errors

### Unknown target

```
error: unknown target 'wasm' (looked for 'targets/wasm.toml')
  help: available targets: triton, miden, openvm, sp1, cairo
```

---

### Cannot read target config

```
error: cannot read target config 'targets/foo.toml': No such file
```

---

### Invalid target name

```
error: invalid target name '../../../etc/passwd'
```

Target names cannot contain path traversal characters.

---

## Warnings

### Unused import

```
warning: unused import 'std.crypto.hash'
```

**Fix:** Remove the unused `use` statement.

---

### Asm block target mismatch

```
warning: asm block tagged for 'risc_v' will be skipped (current target: 'triton')
```

An `asm` block tagged for a different target is silently skipped. This is
informational when using multi-target `asm` blocks intentionally.

---

### Power-of-2 boundary proximity

```
warning: program is 3 rows below padded height boundary
  help: consider optimizing to stay well below 1024
```

The program is close to a power-of-2 table height boundary. A small code
change could double proving cost.

---

## Optimization Hints

The compiler produces hints (not errors) when it detects cost antipatterns.
These appear with `trident build --hints`.

### H0001: Hash table dominance

```
hint[H0001]: hash table is 3.2x taller than processor table
```

The hash table dominates proving cost. Processor-level optimizations will
not reduce proving time.

**Action:** Batch data before hashing, reduce Merkle depth, use
`sponge_absorb_mem` instead of repeated `sponge_absorb`.

---

### H0002: Power-of-2 headroom

```
hint[H0002]: padded height is 1024, but max table height is only 519
```

Significant headroom below the next power-of-2 boundary. The program could
be more complex at zero additional proving cost.

---

### H0003: Redundant range check

```
hint[H0003]: as_u32(x) is redundant — value is already proven U32
```

A value that was already range-checked is being checked again.

**Action:** Remove the redundant `as_u32()` call.

---

### H0004: Loop bound waste

```
hint[H0004]: loop in 'process' bounded 128 but iterates only 10 times
```

The declared loop bound is much larger than the actual constant iteration
count. This inflates worst-case cost analysis.

**Action:** Tighten the `bounded` declaration to match actual usage.

---

## See Also

- [Language Reference](language.md) — Types, operators, builtins, grammar
- [Target Reference](targets.md) — Target profiles, cost models, and OS model
- [Tutorial](../tutorials/tutorial.md) — Step-by-step guide with working examples
- [For Developers](../tutorials/for-developers.md) — Why bounded loops? Why no heap?
- [Optimization Guide](../guides/optimization.md) — Cost reduction strategies
