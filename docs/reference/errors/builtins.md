# Builtin Type Errors

[Back to Error Catalog](../errors.md)

These errors enforce the type signatures of builtin functions. Some may be
caught by generic function type checking (T07/T08), but builtins have
target-dependent signatures that deserve explicit diagnostics.

---

### Builtin argument type mismatch **(planned)**

```
error: builtin 'sub' expects (Field, Field), got (U32, U32)
  help: sub() operates on Field values; convert with as_field() first
```

**Spec:** language.md Section 6 (each builtin has specific argument types).

---

### Builtin argument count mismatch **(planned)**

```
error: builtin 'split' expects 1 argument, got 2
```

**Spec:** language.md Section 6.

---

### Assert argument type **(planned)**

```
error: assert() requires Bool argument, got Digest
```

**Spec:** language.md Section 6 (assert(cond: Bool)).

---

### Assert_eq argument type **(planned)**

```
error: assert_eq() requires (Field, Field), got (Bool, Bool)
  help: use `assert(a == b)` for Bool equality
```

**Spec:** language.md Section 6 (assert_eq takes Field, Field).

---

### Assert_digest argument type **(planned)**

```
error: assert_digest() requires (Digest, Digest), got (Field, Field)
```

**Spec:** language.md Section 6.

---

### RAM address type **(planned)**

```
error: ram_read() address must be Field, got Bool
```

**Spec:** language.md Section 6, Section 8 (RAM: word-addressed by Field).
