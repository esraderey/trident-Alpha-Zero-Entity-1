# Trident — Claude Code Instructions

## Source of Truth

`docs/reference/ir.md` is the canonical reference for:
- TIROp variant names, counts, and tier assignments
- Lowering path names (StackLowering, RegisterLowering, KernelLowering)
- Naming conventions (Reveal/Seal, verb-first, symmetric pairs)
- Architecture diagrams (pipeline, file layout)

Any change to the IR, lowering traits, or op set MUST update ir.md first,
then propagate to code. If ir.md and code disagree, ir.md wins.

## Build & Test

```
cargo check          # type-check
cargo test           # 731+ tests
cargo build --release
```

## License

Cyber License: Don't trust. Don't fear. Don't beg.
