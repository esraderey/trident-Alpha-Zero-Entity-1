# tir — Trident Intermediate Representation

Target-independent TIR between the AST and backend assembly.

The compiler pipeline is: **parse -> typecheck -> TIRBuilder -> Lowering -> assembly text**.

## Structure

- [`mod.rs`](mod.rs) — [`TIROp`](mod.rs:18) enum (~50 variants): core universal ops (stack, arithmetic, I/O, memory, crypto, control flow, abstract events/storage) plus a small set of target-specific ops (extension field). [`Display`](mod.rs:143) impl for debug printing.
- [`builder/`](builder/) — AST-to-IR translation (target-independent). See [builder/README.md](builder/README.md).
- [`lower/`](lower/) — IR-to-assembly backends (target-specific). See [lower/README.md](lower/README.md).

## Key design

TIROp uses **structural control flow** — `IfElse`, `IfOnly`, `Loop` carry nested `Vec<TIROp>` bodies so each backend can choose its own lowering strategy without a shared CFG.

Abstract ops (`EmitEvent`, `SealEvent`, `StorageRead/Write`, `HashDigest`) keep the IR target-independent while backends map them to native instructions.

## Dependencies

- [`TargetConfig`](../tools/target.rs:20) — VM parameters (stack depth, digest width, hash rate)
- [`MonoInstance`](../typecheck/mod.rs:32) — monomorphized generic function instances from the type checker
- [`StackManager`](../codegen/stack.rs:58) / [`SpillFormatter`](../codegen/stack.rs:16) — stack model with automatic RAM spill/reload

## Entry point

Compilation uses IR via [`src/lib.rs`](../lib.rs) — builds IR with [`TIRBuilder`](builder/mod.rs:37) then lowers with [`create_lowering`](lower/mod.rs:23).
