# Annotation Errors

[Back to Error Catalog](../errors.md)

---

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

### Unknown attribute **(planned)**

```
error: unknown attribute '#[foo]'
  help: valid attributes are: cfg, test, pure, intrinsic, requires, ensures
```

**Spec:** language.md Section 7 (closed set of attributes).

---

### Duplicate attribute **(planned)**

```
error: duplicate attribute '#[pure]' on function 'foo'
```

**Spec:** language.md Section 7.

---

### Unknown cfg flag **(planned)**

```
error: unknown cfg flag 'unknown_flag'
  help: valid cfg flags are target-specific and project-defined
```

**Spec:** language.md Section 7 (cfg conditional compilation).
