# Error Catalog

All Trident compiler diagnostics — errors, warnings, and optimization hints.
Derived from the language specification ([language.md](language.md)), target
constraints ([targets.md](targets.md)), and IR tier rules ([ir.md](ir.md)).

This catalog is the source of truth for diagnostics. If a rule in the reference
can be violated, the error must exist here. Entries marked **(planned)** are
specification-required but not yet implemented in the compiler.

---

## Categories

| Category | File | Total | Impl | Planned |
|----------|------|------:|-----:|--------:|
| Lexer | [lexer.md](errors/lexer.md) | 19 | 7 | 12 |
| Parser | [parser.md](errors/parser.md) | 24 | 8 | 16 |
| Type | [types.md](errors/types.md) | 34 | 24 | 10 |
| Control flow | [control-flow.md](errors/control-flow.md) | 8 | 6 | 2 |
| Size generics | [size-generics.md](errors/size-generics.md) | 6 | 4 | 2 |
| Events | [events.md](errors/events.md) | 7 | 5 | 2 |
| Annotations | [annotations.md](errors/annotations.md) | 6 | 3 | 3 |
| Module | [modules.md](errors/modules.md) | 10 | 4 | 6 |
| Target | [targets.md](errors/targets.md) | 14 | 3 | 11 |
| Builtin type | [builtins.md](errors/builtins.md) | 6 | 0 | 6 |
| Inline assembly | [assembly.md](errors/assembly.md) | 2 | 0 | 2 |
| Warnings | [warnings.md](errors/warnings.md) | 7 | 3 | 4 |
| Hints | [hints.md](errors/hints.md) | 5 | 4 | 1 |
| **Total** | | **148** | **71** | **77** |

---

## See Also

- [Language Reference](language.md) — Types, operators, builtins, grammar
- [Target Reference](targets.md) — Target profiles, cost models, and OS model
- [IR Reference](ir.md) — 54 operations, 4 tiers, lowering paths
- [Tutorial](../tutorials/tutorial.md) — Step-by-step guide with working examples
- [For Developers](../tutorials/for-developers.md) — Why bounded loops? Why no heap?
- [Optimization Guide](../guides/optimization.md) — Cost reduction strategies
