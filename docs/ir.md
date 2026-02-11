# Trident IR: Intermediate Representation

The Trident compiler uses an intermediate representation (IR) between the AST and target-specific code generation. This document describes the IR design, its role in the pipeline, and how to add new backends.

## Pipeline

```
Source → Lexer → Parser → AST → TypeChecker → IRBuilder → Vec<IROp> → Lowering → assembly text
```

The IRBuilder walks the type-checked AST and produces a flat list of `IROp` values. A target-specific `Lowering` implementation then converts these into assembly text for the chosen VM.

## Why an IR?

Different proof VMs have fundamentally different architectures:

| Target | Architecture | Control flow |
|--------|-------------|--------------|
| Triton VM | Stack machine | Deferred subroutines + `skiz` |
| Miden VM | Stack machine | Inline `if.true/else/end` |
| OpenVM/SP1 | Register (RISC-V) | Branch instructions |
| Cairo | SSA registers | `branch_align` + enum dispatch |

Without an IR, the code generator must embed every target's control flow conventions directly in its AST walk. The IR separates _what to compute_ (stack operations with structural control flow) from _how to emit it_ (target-specific instruction selection and control flow lowering).

## IROp Enum

The IR is a list of `IROp` variants. There are roughly 35, organized into categories:

### Stack operations
`Push(u64)`, `PushNegOne`, `Pop(u32)`, `Dup(u32)`, `Swap(u32)`

### Arithmetic
`Add`, `Mul`, `Eq`, `Lt`, `And`, `Xor`, `DivMod`, `Invert`, `Split`, `Log2`, `Pow`, `PopCount`

### Extension field
`XbMul`, `XInvert`, `XxDotStep`, `XbDotStep`

### I/O
`ReadIo(u32)`, `WriteIo(u32)`, `Divine(u32)`

### Memory
`ReadMem(u32)`, `WriteMem(u32)`

### Cryptographic
`Hash`, `SpongeInit`, `SpongeAbsorb`, `SpongeSqueeze`, `SpongeAbsorbMem`, `MerkleStep`, `MerkleStepMem`

### Assertions
`Assert`, `AssertVector`

### Flat control flow
`Call(String)`, `Return`, `Halt`

### Structural control flow
```rust
IfElse { then_body: Vec<IROp>, else_body: Vec<IROp> }
IfOnly { then_body: Vec<IROp> }
Loop { label: String, body: Vec<IROp> }
```

### Program structure
`Label(String)`, `FnStart(String)`, `FnEnd`, `Preamble(String)`, `BlankLine`

### Passthrough
`Comment(String)`, `RawAsm { lines: Vec<String>, effect: i32 }`

## Design Decisions

### Structural control flow

`IfElse`, `IfOnly`, and `Loop` contain nested `Vec<IROp>` bodies rather than flat jump targets. This lets each backend choose its own lowering strategy:

- **Triton**: extracts bodies into deferred subroutines, emits `skiz` + `call` at the branch point
- **Miden**: emits inline `if.true / {body} / else / {body} / end`
- **RISC-V**: could emit conditional branches to labels
- **Cairo**: could emit `branch_align` blocks

If the IR used flat basic blocks with jumps, stack-machine backends would need to reconstruct the nesting — unnecessary work that the source language already provides.

### Stack-level, not variable-level

The IR contains explicit `Push`/`Pop`/`Dup`/`Swap` operations. The IRBuilder resolves variable names to stack positions using the existing `StackManager`. This makes Triton lowering trivial (1:1 mapping) and avoids inventing register allocation for stack machines.

For register-based targets (RISC-V, Cairo), the lowering would need to track a virtual stack and map operations to register moves. This is more work per backend but keeps the IR simple and the stack-machine path fast.

### No target-specific instructions in IR

The IR has no `Skiz`, `Recurse`, or `if.true`. These are target-specific and belong in the lowering. The IR represents the intent (_conditional branch_, _loop iteration_) and each lowering chooses the mechanism.

### RawAsm passthrough

Inline assembly blocks (`asm { ... }`) pass through as `RawAsm` with a declared stack effect. The IRBuilder preserves them unchanged. Target filtering (`#[cfg(target = "...")]`) happens before IR building, so only relevant assembly reaches the IR.

## IRBuilder

The `IRBuilder` (`src/codegen/ir/builder.rs`) replaces the old Emitter's AST-walking logic. It takes the same inputs (AST, type-checker exports, target config) and produces `Vec<IROp>`.

Key methods:
- `build_file(&File) -> Vec<IROp>` — entry point
- `build_fn(&FnDef)` — function body
- `build_stmt(&Stmt)` — statements (let, if, for, assign, etc.)
- `build_expr(&Expr)` — expressions (literals, binops, calls, etc.)

Builder-pattern configuration:
```rust
IRBuilder::new(target_config)
    .with_cfg_flags(flags)
    .with_intrinsics(map)
    .with_module_aliases(aliases)
    .with_constants(constants)
    .with_mono_instances(instances)
    .with_call_resolutions(resolutions)
    .build_file(&file)
```

## Lowering Trait

```rust
pub trait Lowering {
    fn lower(&self, ops: &[IROp]) -> Vec<String>;
}
```

Each target implements `Lowering`. Current implementations:

### TritonLowering
Produces Triton Assembly (TASM). Maps flat IROps 1:1 to instructions. Structural control flow becomes deferred subroutines with `skiz` + `call` branching. Labels get `__` prefix.

### MidenLowering
Produces Miden Assembly (MASM). Uses inline `if.true/else/end` for conditionals, `proc.name/end` for functions, and `exec.self` for loop recursion. Tracks indentation depth for nested control flow.

## Adding a New Backend

1. Add a new struct implementing `Lowering` in `src/codegen/ir/lower.rs`
2. Implement `lower_op()` mapping each `IROp` to your target's instructions
3. Handle structural control flow (`IfElse`, `IfOnly`, `Loop`) according to your target's conventions
4. Add tests verifying the output structure
5. Wire it into `lib.rs` compile functions (behind target config dispatch)

## File Layout

```
src/codegen/ir/
    mod.rs       — IROp enum, Display impl
    builder.rs   — IRBuilder: AST → Vec<IROp>
    lower.rs     — Lowering trait, TritonLowering, MidenLowering
```
