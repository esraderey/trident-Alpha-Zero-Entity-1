# Self-Hosted Parser: Proven Parsing

**Status: COMPLETE**

~2,700 LOC. Compiles to TASM. All grammar constructs covered.

## Completion Summary

Phase 8 (gap-fill): `.field` postfix access, program declarations
(`pub input/output`, `sec input/ram`), attribute recording (`#[attr]` →
NK_ATTR nodes), full expression parsing in struct init/reveal/seal/for-range
fields (replacing parse_simple_expr), let tuple destructuring (`let (a, b) = expr`),
generic args at call sites (`name<N>(...)`).

## Context

The self-hosted lexer is complete and STARK-verified (824 LOC, execute 39ms,
prove 10.2s, verify 42ms — PASS). The parser is the second self-hosting
component. It takes the token stream from the lexer and produces a flat
linearized AST in RAM.

The Rust parser is 1,430 LOC across 5 files using recursive descent with
Pratt precedence climbing. Trident has no recursion, no heap, no dynamic
dispatch. The self-hosted parser must flatten all recursive patterns into
an explicit parse stack + state machine driven by a single bounded loop.

## Architecture

### The Core Problem: Recursion Without Recursion

The Rust parser uses recursive descent everywhere:
- `parse_block()` calls `parse_if_stmt()` which calls `parse_block()`
- `parse_expr_bp()` recurses for right-hand operands
- `parse_primary()` calls `parse_expr()` for parenthesized exprs

Trident forbids recursion. Solution: **explicit parse stack in RAM** with
state codes acting as continuation addresses. A single `for _step in
0..MAX_STEPS bounded MAX_STEPS` loop dispatches on the current state.

### Memory Layout

```
Inputs (from lexer):
  tok_base .. tok_base + tok_count*4   Token stream (kind, start, end, int_val)

Outputs:
  ast_base .. ast_base + node_count*8  AST nodes (stride 8)
  err_base .. err_base + err_count*3   Parse errors (code, start, end)

Working memory:
  state_base .. state_base + 16        Parser state (pos, counts, bases, scratch)
  stack_base .. stack_base + depth*8   Parse stack (stride 8 frames)
```

### Parser State (16 words at state_base)

```
+0  tok_pos         Current token index
+1  tok_count       Total tokens (from lexer)
+2  node_count      AST nodes emitted
+3  err_count       Parse errors
+4  tok_base        Token array base address
+5  ast_base        AST output base address
+6  err_base        Error output base address
+7  stack_base      Parse stack base address
+8  stack_depth     Current stack depth
+9  scratch_0       Last result / return register
+10 scratch_1       Temp storage
+11 scratch_2       Temp storage
+12 scratch_3       Temp storage (min_bp for Pratt)
+13 done            Done flag
+14 item_flags      Accumulated flags (is_pub, is_test, etc.)
+15 reserved
```

### AST Node Format (stride 8)

Every node is 8 Fields at `ast_base + node_idx * 8`:

```
+0  kind            NK_* constant
+1  field_0         Meaning depends on kind (token index, child node, value)
+2  field_1
+3  field_2
+4  field_3
+5  field_4
+6  field_5
+7  field_6
```

Variable-length lists (params, args, stmts) stored as contiguous node
runs. Parent stores `(first_child_idx, count)` in two fields.

### Node Kinds (~38 NK_* constants)

```
// File-level
NK_FILE = 1          // kind, file_kind, name_tok, uses_start, uses_count, items_start, items_count
NK_USE = 2           // kind, path_start_tok, path_end_tok

// Items
NK_FN = 3            // kind, name_tok, params_start, params_count, ret_type_node, body_node, flags
NK_CONST = 4         // kind, name_tok, type_node, value_node, flags
NK_STRUCT = 5        // kind, name_tok, fields_start, fields_count, flags
NK_EVENT = 6         // kind, name_tok, fields_start, fields_count
NK_PARAM = 7         // kind, name_tok, type_node
NK_STRUCT_FIELD = 8  // kind, name_tok, type_node, flags

// Types
NK_TYPE_FIELD = 10
NK_TYPE_XFIELD = 11
NK_TYPE_BOOL = 12
NK_TYPE_U32 = 13
NK_TYPE_DIGEST = 14
NK_TYPE_ARRAY = 15   // kind, inner_type_node, size_value
NK_TYPE_TUPLE = 16   // kind, types_start, types_count
NK_TYPE_NAMED = 17   // kind, path_start_tok, path_end_tok

// Statements
NK_LET = 20          // kind, name_tok, type_node, init_node, flags (mutable)
NK_ASSIGN = 21       // kind, place_node, value_node
NK_IF = 22           // kind, cond_node, then_node, else_node (0 = none)
NK_FOR = 23          // kind, var_tok, start_node, end_node, bound_value, body_node
NK_RETURN = 24       // kind, value_node (0 = void)
NK_EXPR_STMT = 25    // kind, expr_node
NK_BLOCK = 26        // kind, stmts_start, stmts_count, tail_node (0 = none)
NK_REVEAL = 27       // kind, name_tok, fields_start, fields_count
NK_SEAL = 28         // kind, name_tok, fields_start, fields_count
NK_ASM = 29          // kind, tok_idx (original token has body/effect/target)
NK_MATCH = 30        // kind, expr_node, arms_start, arms_count
NK_MATCH_ARM = 31    // kind, pattern_node, body_node

// Expressions
NK_LIT_INT = 40      // kind, value (from token int_val)
NK_LIT_BOOL = 41     // kind, value (0 or 1)
NK_VAR = 42          // kind, path_start_tok, path_end_tok
NK_BINOP = 43        // kind, op_code, lhs_node, rhs_node
NK_CALL = 44         // kind, path_start_tok, path_end_tok, args_start, args_count
NK_FIELD_ACCESS = 45 // kind, expr_node, field_tok
NK_INDEX = 46        // kind, expr_node, index_node
NK_STRUCT_INIT = 47  // kind, path_start_tok, fields_start, fields_count
NK_ARRAY_INIT = 48   // kind, elems_start, elems_count
NK_TUPLE = 49        // kind, elems_start, elems_count
NK_INIT_FIELD = 50   // kind, name_tok, value_node (for struct init / reveal / seal)

// Patterns
NK_PAT_NAME = 51     // kind, tok_idx
NK_PAT_TUPLE = 52    // kind, names_start, names_count
NK_PAT_WILDCARD = 53
NK_PAT_LIT = 54      // kind, value
NK_PAT_STRUCT = 55   // kind, name_tok, fields_start, fields_count
```

### Parse Stack Frame (stride 8)

```
+0  state           STATE_* code (what to do next)
+1  node_idx        Node being built (for backpatching children)
+2  count           Counter (items parsed so far, etc.)
+3  extra_0         State-specific (min_bp for Pratt, etc.)
+4  extra_1
+5  extra_2
+6  extra_3
+7  extra_4
```

### State Machine (~35 STATE_* codes)

```
// Top-level
STATE_FILE = 1              // Parse program/module header, then uses, then items
STATE_USES = 2              // Parse use declarations
STATE_ITEMS = 3             // Parse top-level items until EOF
STATE_PARSE_ITEM = 4        // Dispatch on fn/struct/event/const

// Items
STATE_FN_PARAMS = 5         // Parse fn parameter list
STATE_FN_RETURN = 6         // Parse optional -> Type, then body
STATE_STRUCT_FIELDS = 7     // Parse struct field list
STATE_EVENT_FIELDS = 8      // Parse event field list
STATE_CONST_VALUE = 9       // Parse const type + value

// Block + statements
STATE_BLOCK = 10            // Expect {, then statements
STATE_BLOCK_STMTS = 11      // Parse statements until }
STATE_PARSE_STMT = 12       // Dispatch on let/if/for/return/etc.
STATE_LET_TYPE = 13         // Optional : Type in let
STATE_LET_INIT = 14         // = expr in let
STATE_IF_COND = 15          // Parse if condition
STATE_IF_THEN = 16          // Parse then block
STATE_IF_ELSE = 17          // Check for else
STATE_FOR_RANGE = 18        // Parse in start..end bounded N
STATE_FOR_BODY = 19         // Parse for body block
STATE_RETURN_VALUE = 20     // Parse optional return value
STATE_MATCH_EXPR = 21       // Parse match expression
STATE_MATCH_ARMS = 22       // Parse match arms
STATE_MATCH_ARM_BODY = 23   // Parse => block

// Expressions (Pratt without recursion)
STATE_PARSE_EXPR = 24       // Parse primary, then check for infix
STATE_EXPR_INFIX = 25       // Check for binary operator
STATE_EXPR_INFIX_RHS = 26   // Parse right operand
STATE_PARSE_PRIMARY = 27    // Dispatch on token kind
STATE_CALL_ARGS = 28        // Parse call argument list
STATE_STRUCT_INIT_FIELDS = 29  // Parse struct init fields
STATE_ARRAY_INIT_ELEMS = 30 // Parse array elements
STATE_TUPLE_ELEMS = 31      // Parse tuple elements
STATE_POSTFIX = 32          // Parse .field and [index]

// Types
STATE_PARSE_TYPE = 33       // Dispatch on token kind
STATE_ARRAY_TYPE = 34       // Parse [T; N]
STATE_TUPLE_TYPE = 35       // Parse (T, U)

// Result passing
STATE_DONE = 0              // Parsing complete
```

### Pratt Parsing Without Recursion

The Rust parser's recursive `parse_expr_bp(min_bp)` becomes three states:

1. **STATE_PARSE_EXPR**: Push frame `{state=EXPR_INFIX, min_bp=min_bp}`,
   then set state to STATE_PARSE_PRIMARY.

2. **STATE_PARSE_PRIMARY**: Parse literal/var/call/etc. Store result in
   `scratch_0`. Pop to parent (which is EXPR_INFIX).

3. **STATE_EXPR_INFIX**: Read operator token. If no operator or
   `l_bp < min_bp`, result is in `scratch_0` — pop to parent. Otherwise:
   push frame `{state=EXPR_INFIX_RHS, lhs=scratch_0, op=op_code}`,
   push frame `{state=PARSE_EXPR, min_bp=r_bp}`.

4. **STATE_EXPR_INFIX_RHS**: rhs = scratch_0. Emit NK_BINOP(op, lhs, rhs).
   Set scratch_0 = new node idx. Go to EXPR_INFIX (loop for chained ops).

Result passing: `scratch_0` (state_base + 9) is the "return value register".
When a sub-parse completes, it writes its result node index to scratch_0.
The parent reads it.

### Binding Powers (matching `BinOp::binding_power()`)

```
OP_EQ = 1       l=2, r=3     ==
OP_LT = 2       l=4, r=5     <
OP_ADD = 3      l=6, r=7     +
OP_MUL = 4      l=8, r=9     *
OP_XFMUL = 5    l=8, r=9     *.
OP_BAND = 6     l=10, r=11   &
OP_BXOR = 7     l=10, r=11   ^
OP_DIVMOD = 8   l=12, r=13   /%
```

## Public API

```trident
// std/compiler/parser.tri
pub fn parse(
    tok_base: Field,      // Token array from lexer (stride 4)
    tok_count: Field,      // Number of tokens
    ast_base: Field,       // Output: AST nodes (stride 8)
    err_base: Field,       // Output: parse errors
    state_base: Field,     // Working state (16 words)
    stack_base: Field      // Parse stack memory
)
// After:
//   node_count = mem.read(state_base + 2)
//   err_count  = mem.read(state_base + 3)
//   ast[i]     = RAM[ast_base + i*8 .. ast_base + i*8 + 7]
```

## Files

### 1. `std/compiler/parser.tri` (~2000-2500 lines)

Module: `std.compiler.parser`. Imports: `vm.core.field`, `vm.core.convert`,
`vm.io.mem`, `std.compiler.lexer` (for TK_* constants — must make them pub).

Functions:
- `parse()` — main entry, initializes state, runs main loop
- `dispatch(sb)` — state dispatch (if/else chain on ~35 states)
- Token accessors: `tok_kind(sb, idx)`, `tok_start(sb, idx)`, etc.
- AST output: `emit_node(sb, kind, f0..f6) -> Field` returns node index
- `backpatch(sb, node_idx, field_offset, value)` — fill in child refs
- Stack: `push_frame(sb, state, ...)`, `pop_frame(sb)`, `top_state(sb)`
- State handlers: one function per STATE_* group

### 2. Modify `std/compiler/lexer.tri`

Make TK_* constants `pub fn` so parser can call `lexer.TK_PROGRAM()` etc.

### 3. `benches/std/compiler/parser.reference.rs`

Rust ground truth. Takes a test source, parses with Rust parser, serializes
AST to the flat stride-8 format. Outputs node count and expected node
data as `values:` line.

### 4. `benches/std/compiler/parser_bench.tri`

Benchmark program: reads tokens from input, calls `parser.parse()`,
asserts node count and spot-checks key nodes.

### 5. `benches/std/compiler/parser.inputs`

Test data: token stream for a small Trident program, expected AST nodes.

### 6. `benches/std/compiler/parser.baseline.tasm`

Placeholder baseline (same pattern as lexer).

### 7. `tests/audit_stdlib.rs` — add compilation test

### 8. Register example in `Cargo.toml`

## Implementation Order

### Phase 1: Skeleton + Infrastructure (1 session)

Create `std/compiler/parser.tri` with:
- All NK_*, STATE_*, OP_* constants
- Token accessors, AST emit_node, backpatch
- Stack push/pop/peek
- Main loop skeleton (dispatch → STATE_DONE)
- Stub `parse()` that just emits one NK_FILE node

Make lexer TK_* functions pub.
Add compilation test. Verify: `trident build`, `cargo test`.

### Phase 2: Top-Level Parsing (1 session)

- STATE_FILE: parse `program`/`module` keyword, name
- STATE_USES: parse `use` paths
- STATE_ITEMS: dispatch on fn/struct/event/const/attributes
- STATE_FN_PARAMS, STATE_FN_RETURN: function signatures
- STATE_CONST_VALUE, STATE_STRUCT_FIELDS, STATE_EVENT_FIELDS

### Phase 3: Types (1 session)

- STATE_PARSE_TYPE: dispatch on Field/XField/Bool/U32/Digest/[/(/Ident
- STATE_ARRAY_TYPE: parse `[T; N]`
- STATE_TUPLE_TYPE: parse `(T, U)`
- Named types via module path

### Phase 4: Blocks + Statements (1 session)

- STATE_BLOCK, STATE_BLOCK_STMTS: brace-delimited statement lists
- STATE_PARSE_STMT: dispatch on let/if/for/return/reveal/seal/match/asm/expr
- STATE_LET_TYPE, STATE_LET_INIT
- STATE_IF_COND, STATE_IF_THEN, STATE_IF_ELSE (including else-if chains)
- STATE_FOR_RANGE, STATE_FOR_BODY
- STATE_RETURN_VALUE
- STATE_MATCH_EXPR, STATE_MATCH_ARMS, STATE_MATCH_ARM_BODY

### Phase 5: Expressions (1 session)

- STATE_PARSE_EXPR, STATE_EXPR_INFIX, STATE_EXPR_INFIX_RHS (Pratt)
- STATE_PARSE_PRIMARY: Integer, Bool, Var, Call, StructInit, ArrayInit, Tuple, Paren
- STATE_CALL_ARGS, STATE_STRUCT_INIT_FIELDS, STATE_ARRAY_INIT_ELEMS, STATE_TUPLE_ELEMS
- STATE_POSTFIX: .field and [index] chains
- Binding power dispatch for all 8 operators

### Phase 6: Bench + Prove (1 session)

- `parser.reference.rs` — Rust ground truth
- `parser_bench.tri` — benchmark program
- `parser.inputs` — test data
- End-to-end: execute, prove, verify
- Register example in Cargo.toml

### Phase 7: Integration (1 session)

- Chain lexer → parser in a single bench: source bytes → tokens → AST
- Verify against Rust pipeline for multiple test programs
- Update .cortex plan, commit

## Verification

- `cargo check` — zero warnings
- `cargo test` — all pass (incl. new compilation test)
- `trident build std/compiler/parser.tri` — compiles to TASM
- `cargo run --example ref_std_compiler_parser` — Rust ground truth
- `trident bench benches/std/compiler` — includes parser metrics
- `trident bench benches/std/compiler --full` — parser exec, prove, verify PASS
- AST output matches Rust parser for test programs

## Critical Files

- `src/syntax/parser/mod.rs` (210 LOC) — Parser struct, peek/advance/eat/expect
- `src/syntax/parser/items.rs` (456 LOC) — file/fn/struct/event/const parsing
- `src/syntax/parser/stmts.rs` (364 LOC) — block/let/if/for/match/return
- `src/syntax/parser/expr.rs` (291 LOC) — Pratt precedence climbing, primary, postfix
- `src/syntax/parser/types.rs` (109 LOC) — type parsing
- `src/ast/mod.rs` (387 LOC) — AST type definitions (25 types, 57 variants)
- `std/compiler/lexer.tri` (824 LOC) — Token kind constants + lexer
- `vm/core/field.tri` — field.neg for subtraction
- `vm/core/convert.tri` — as_u32, as_field
