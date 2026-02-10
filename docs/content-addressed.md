# Content-Addressed Trident

**Design Document — v0.1 Draft**
**February 2026**

*When code is identified by what it computes, not what it's called, everything changes.*

---

## 1. The Core Idea

Every Trident function compiles to a deterministic constraint system. That constraint system has a unique mathematical identity — change one instruction and the identity changes. This is already true at the proving layer: zkVM verifiers check proofs against specific program hashes.

Content-Addressed Trident pushes this identity all the way up to the source level. Every function, every module, every contract is identified by a cryptographic hash of its normalized abstract syntax tree. Names are metadata. The hash is the identity.

```
// Alice writes:
pub fn verify_merkle(root: Digest, leaf: Digest, index: U32, depth: U32) { ... }
// Source hash: #a7f3b2c1

// Bob writes the same function with different names, on a different continent:
pub fn check_proof(tree_root: Digest, value: Digest, pos: U32, height: U32) { ... }
// Source hash: #a7f3b2c1  ← SAME HASH

// The codebase stores one function. Alice's verification certificate covers Bob's code.
// Bob's proving cost analysis applies to Alice's deployment.
```

This is inspired by Unison's content-addressed code, but applied to a domain where it becomes dramatically more powerful: **provable computation**, where the identity of code is not just convenient but cryptographically essential.

### 1.1 Why Content Addressing Is Natural for ZK

In conventional programming, content-addressed code is a clever optimization. In provable computation, it's fundamental:

- A zkVM verifier already checks proofs against a program hash — the program IS its hash at the verification layer
- A verification certificate proves properties of a specific computation — identified by hash
- Cross-chain equivalence requires proving "chain A and chain B run the same program" — hash comparison
- Audit results must be pinned to exact code, not a name that might point to different versions — hash pinning
- Proving cost is deterministic for a given computation — hash-indexed cost cache

Content addressing isn't bolted on. It's the natural representation for code whose purpose is to be proven.

### 1.2 What Changes

| Aspect | Text-file Trident (today) | Content-Addressed Trident |
|--------|--------------------------|---------------------------|
| Function identity | Name + file path | Cryptographic hash of normalized AST |
| Dependencies | Module path (`use std.crypto.hash`) | Hash references (names are metadata) |
| Builds | Recompile changed files | Never compile the same function twice |
| Verification | Re-verify on every build | Cached by hash — verified once, valid forever |
| Renaming | Find-and-replace, may break things | Instant, non-breaking (names are pointers to hashes) |
| Version conflicts | Possible (two deps want different versions) | Impossible (same hash = same code, different hash = different code) |
| Proving cost | Compute on every build | Cached by hash per target, instantly available |
| Audit certificates | Attached to a repo + commit | Attached to a hash — immutable, unforgeable |
| Cross-chain equivalence | Trust that "same source" was deployed | Verify that same source hash produced both target binaries |
| Code sharing | Copy files, manage versions | Share hashes, pull from global registry |

---

## 2. Hash Function Design

The hash function is the foundation of the entire system. It must be deterministic, collision-resistant, semantic-aware, and fast. Trident uses a two-layer hashing architecture.

### 2.1 Two-Layer Architecture

```
Trident Source
    → Parse → Typecheck → Normalized AST
        → LAYER 1: Canonical Source Hash (BLAKE3)
            Universal identity. Target-independent.
            What developers, registries, and auditors reference.
            
    → Compile to target
        → LAYER 2: Target Artifact Hash (target-native)
            Per-target identity. What on-chain verifiers check.
            Triton: Tip5 hash of TASM bytecode
            Miden: RPO hash of Miden Assembly
            Cairo: Poseidon hash of Sierra IR
            OpenVM: SHA3 hash of ELF binary
```

**Layer 1 (Source Hash)** is the canonical identity used throughout the development workflow, registry, verification certificates, and cross-target equivalence proofs.

**Layer 2 (Target Hash)** is derived from Layer 1 by compilation. The registry maps source hashes to target hashes. On-chain verifiers use target hashes. The source hash proves that two target hashes came from the same computation.

### 2.2 Layer 1: Canonical Source Hash

#### 2.2.1 Hash Algorithm: BLAKE3

BLAKE3 is the right choice for source hashing:

| Property | BLAKE3 | SHA3-256 | Tip5 |
|----------|--------|---------|------|
| Speed | ~4 GB/s (single-threaded) | ~500 MB/s | ~100 MB/s (field ops) |
| Output size | 256 bits (configurable) | 256 bits | 320 bits (5 × 64-bit) |
| Standardized | Yes (IETF draft) | Yes (NIST) | No (Triton-specific) |
| Deterministic | Yes | Yes | Yes |
| Available everywhere | Yes (pure Rust, C, WASM) | Yes | No (requires field arithmetic) |
| ZK-provable | Expensive (bitwise ops) | Expensive | Native on Triton only |
| Incremental | Yes (built-in tree hashing) | No | No |

BLAKE3's incremental tree-hashing mode is particularly valuable: when a function's dependency changes, we can rehash only the changed subtree rather than the entire AST.

**Why not use the target VM's native hash (Tip5, RPO, Poseidon)?**

Target-native hashes are optimized for ZK circuits, not for general-purpose hashing of AST structures. They operate over field elements, not bytes. Using Tip5 would make the source hash Triton-specific, defeating the purpose of a universal identity. BLAKE3 is the source identity; target-native hashes are the compiled identity.

**Why not SHA3-256?**

SHA3 is fine but BLAKE3 is 5-8x faster, supports incremental hashing natively, and is equally collision-resistant. For a development tool that hashes on every keystroke, speed matters.

#### 2.2.2 What Gets Hashed: The Normalized AST

The hash must be **semantic** — two functions with the same behavior should have the same hash, even if they use different variable names or formatting. This requires normalizing the AST before hashing.

**Normalization steps:**

**Step 1: Strip names, replace with de Bruijn indices.**

```
// Before normalization:
fn transfer(sender_balance: Field, amount: Field) -> Field {
    let new_balance = sender_balance - amount
    new_balance
}

// After normalization (de Bruijn indices):
fn (#0: Field, #1: Field) -> Field {
    let #2 = #0 - #1
    #2
}
```

Variable names are metadata, not identity. The function `transfer(a, b)` and `transfer(x, y)` with identical bodies produce identical hashes.

**Step 2: Replace dependency references with their hashes.**

```
// Before:
use std.crypto.hash
let d = hash(input)

// After:
let d = #f8a2b1c3(input)    // #f8a2b1c3 is the hash of std.crypto.hash.hash
```

Dependencies are pinned by hash, not by name or path. If `std.crypto.hash.hash` changes, its hash changes, and all dependents get new hashes too.

**Step 3: Canonicalize type annotations.**

```
// Both produce the same normalized form:
fn foo(x: Field) -> Field { x }
fn foo(x: Field) -> Field { return x }    // (if return syntax existed)
```

Type inference results are made explicit. Implicit conversions are made explicit. The AST is fully elaborated before hashing.

**Step 4: Normalize struct field ordering and expression structure.**

```
// These produce the same hash:
let p = Point { x: 1, y: 2 }
let p = Point { y: 2, x: 1 }     // field order doesn't matter (alphabetized)
```

Struct fields are sorted alphabetically. Commutative operations are sorted by operand hash. Associative chains are flattened and sorted.

**Step 5: Strip metadata.**

Comments, documentation, source location, formatting — all stripped. Only the computational content contributes to the hash.

#### 2.2.3 Hash Serialization Format

The normalized AST is serialized to bytes using a deterministic binary encoding before hashing. The encoding must be:

- **Canonical**: one and only one byte sequence per normalized AST
- **Compact**: minimize hash input size for speed
- **Self-describing**: each node prefixed with a type tag
- **Stable**: the encoding format is versioned and frozen per version

```
Encoding schema (simplified):

Node types (1-byte tag):
  0x01 = FnDef { param_count: u16, body: Node }
  0x02 = Let { binding: u16, value: Node, body: Node }
  0x03 = Var { index: u16 }                              // de Bruijn index
  0x04 = FieldLit { value: u64 }
  0x05 = U32Lit { value: u32 }
  0x06 = BoolLit { value: u8 }
  0x07 = Add { lhs: Node, rhs: Node }
  0x08 = Mul { lhs: Node, rhs: Node }
  0x09 = Sub { lhs: Node, rhs: Node }
  0x0A = Inv { operand: Node }
  0x0B = Eq { lhs: Node, rhs: Node }
  0x0C = Lt { lhs: Node, rhs: Node }
  0x0D = And { lhs: Node, rhs: Node }
  0x0E = Xor { lhs: Node, rhs: Node }
  0x0F = If { cond: Node, then: Node, else: Node }
  0x10 = For { bound: u32, max: u32, body: Node }
  0x11 = Assert { cond: Node }
  0x12 = Call { target_hash: [u8; 32], args: [Node] }    // dependency by hash
  0x13 = PubRead { count: u8 }
  0x14 = PubWrite { value: Node }
  0x15 = Divine { count: u8 }
  0x16 = Hash { inputs: [Node] }                          // abstract hash
  0x17 = SpongeInit
  0x18 = SpongeAbsorb { inputs: [Node] }
  0x19 = SpongeSqueeze
  0x1A = ArrayInit { elements: [Node] }
  0x1B = ArrayIndex { array: Node, index: Node }
  0x1C = StructInit { fields: [(u16, Node)] }             // sorted by field id
  0x1D = FieldAccess { base: Node, field: u16 }
  0x1E = MerkleVerify { root: Node, leaf: Node, index: Node, depth: Node }
  
  // Backend extension intrinsics
  0xF0 = ExtensionIntrinsic { target: u8, id: u16, args: [Node] }
  
  // Specification annotations (don't affect computational hash, 
  // but contribute to verification hash — see §2.2.5)
  0xE0 = Requires { cond: Node }
  0xE1 = Ensures { cond: Node }
  0xE2 = Invariant { cond: Node }

Type tags (1-byte):
  0x80 = Field
  0x81 = Bool
  0x82 = U32
  0x83 = Array { elem: Type, length: u32 }
  0x84 = Tuple { elements: [Type] }
  0x85 = Struct { fields: [(u16, Type)] }
  0x86 = Digest                                           // abstract, width from target
```

#### 2.2.4 Hash Composition

Functions hash their own AST plus the hashes of all dependencies. This creates a Merkle-like structure:

```
Hash(transfer) = BLAKE3(
    serialize(transfer_normalized_ast)
)

// where transfer_normalized_ast contains:
//   Call { target_hash: Hash(verify_merkle), args: [...] }
//   Call { target_hash: Hash(check_balance), args: [...] }
```

If `verify_merkle` changes, its hash changes, which changes `transfer`'s hash (because the `Call` node contains the dependency hash). This propagation is automatic and exact — only truly affected functions get new hashes.

**Circular dependencies are impossible** by Trident's module DAG enforcement. The hash computation always terminates.

#### 2.2.5 Computational Hash vs. Verification Hash

The system maintains two related source-level hashes:

**Computational Hash**: Hashes only the computational content — the code that actually executes. Specification annotations (`#[requires]`, `#[ensures]`, `#[invariant]`) are excluded. Two functions with the same logic but different specs have the same computational hash.

**Verification Hash**: Hashes the computational content PLUS all specification annotations. Two functions with the same logic but different specs have different verification hashes. Verification certificates are keyed by verification hash.

```
fn foo(x: Field) -> Field { x * x }
// Computational hash: #a1b2c3d4
// Verification hash:  #a1b2c3d4 (no specs)

#[ensures(result == x * x)]
fn foo(x: Field) -> Field { x * x }
// Computational hash: #a1b2c3d4 (same — computation unchanged)
// Verification hash:  #e5f6g7h8 (different — spec added)
```

This separation means:
- Compilation caches use the computational hash (specs don't affect output)
- Verification caches use the verification hash (specs affect what's proven)
- Two developers with the same logic but different specs share compilation work but have independent verification results

#### 2.2.6 Hash Display Format

Hashes are displayed as truncated base-32 strings for human readability:

```
Full hash (256 bits):  a7f3b2c1d4e5f6a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8d9e0f1a2
Display (40 bits):     #a7f3b2c1

// Collision probability at 40-bit truncation:
// With 1 million definitions: ~0.00005% chance of display collision
// Full hash always used internally; truncated display is for humans only
```

### 2.3 Layer 2: Target Artifact Hash

Each compilation target produces a target-specific hash of the compiled output:

| Target | Hash Function | Input | Output Size |
|--------|--------------|-------|:-----------:|
| Triton VM | Tip5 | TASM bytecode (as field elements) | 5 × 64-bit |
| Miden VM | RPO | Miden Assembly (as field elements) | 4 × 64-bit |
| Cairo | Poseidon | Sierra IR (as felts) | 1 × 252-bit |
| OpenVM | BLAKE3 | ELF binary (as bytes) | 256-bit |

Target hashes serve a different purpose than source hashes:
- Source hash = developer identity, registry key, cross-target equivalence
- Target hash = on-chain verifier identity, proof binding, deployment address

The registry maintains the mapping:

```
Source #a7f3b2c1 →
    Triton: Tip5(#t_8f2a3b4c5d...)
    Miden:  RPO(#m_3c7b9a1d2e...)
    Cairo:  Poseidon(#c_1d4e5f6a7b...)
    OpenVM: BLAKE3(#o_9a5f2b3c4d...)
```

### 2.4 Hash Stability and Versioning

The normalization and serialization format is versioned. Each version is frozen — once released, it never changes.

```
Hash prefix: version byte + hash bytes
  v1: 0x01 || BLAKE3(serialize_v1(normalize_v1(ast)))
  v2: 0x02 || BLAKE3(serialize_v2(normalize_v2(ast)))
```

A function hashed with v1 normalization has a different hash than the same function hashed with v2, even if the computation is identical. This is intentional — hash stability within a version is absolute, and version migration is explicit.

The codebase stores which version was used for each hash. Migration between versions is a batch operation: re-normalize and re-hash all functions, update all references.

---

## 3. The Codebase

### 3.1 Codebase Structure

Trident's codebase is not a directory of text files. It's a content-addressed database of normalized, type-checked AST nodes. Source text is a view into this database — a way to display and edit code — not the source of truth.

```
Codebase Database:
├── definitions/                    # Normalized ASTs keyed by hash
│   ├── #a7f3b2c1 → { ast: ..., type: (Digest, Digest, U32, U32) -> (), deps: [...] }
│   ├── #c4e9d1a8 → { ast: ..., type: (...) -> TransferResult, deps: [#a7f3b2c1, ...] }
│   └── ...
│
├── names/                          # Name → hash mappings (mutable pointers)
│   ├── verify_merkle → #a7f3b2c1
│   ├── transfer_token → #c4e9d1a8
│   └── ...
│
├── verification/                   # Verification results keyed by verification hash
│   ├── #e5f6g7h8 → { status: verified, properties: [...], certificate: ... }
│   └── ...
│
├── compilation/                    # Compiled artifacts keyed by (source hash, target)
│   ├── (#a7f3b2c1, triton) → { tasm: ..., cost: 847cc, target_hash: #t_8f2a... }
│   ├── (#a7f3b2c1, miden)  → { masm: ..., cost: 623cc, target_hash: #m_3c7b... }
│   └── ...
│
├── types/                          # Type definitions keyed by hash
│   ├── #b3d5e7f9 → struct TransferResult { sender: Field, receiver: Field }
│   └── ...
│
└── metadata/                       # Human-readable metadata (names, docs, source locations)
    ├── #a7f3b2c1 → { name: "verify_merkle", doc: "Verify a Merkle proof...", author: "alice" }
    └── ...
```

### 3.2 Append-Only Semantics

The definitions store is append-only. Once a hash is written, it is never modified or deleted. This guarantees:

- **Reproducibility**: any historical state of the codebase can be reconstructed
- **Auditability**: you can trace the evolution of any function through its hash history
- **Cacheability**: compilation and verification results are valid forever for a given hash
- **Concurrency**: multiple developers can add definitions simultaneously without conflicts

Names (the mutable pointers) can be updated to point to new hashes. This is how "editing" works — you create a new definition with a new hash, and update the name pointer.

### 3.3 Editing Workflow

```
1. Developer asks to edit `verify_merkle`
2. Codebase pretty-prints #a7f3b2c1 to the developer's editor
3. Developer modifies the code and saves
4. Codebase:
   a. Parses the new code
   b. Type-checks it
   c. Normalizes the AST
   d. Computes the new hash: #b8c4d2e5
   e. Stores the new definition at #b8c4d2e5
   f. Updates the name: verify_merkle → #b8c4d2e5
   g. Identifies dependents: transfer_token depends on verify_merkle
   h. Automatically produces new versions of dependents with updated hash references
   i. If dependent type signatures still match: automatic propagation
   j. If type signatures changed: adds to developer's "todo list"
   k. Triggers re-verification of changed definitions
5. Old definition #a7f3b2c1 remains in the database (immutable)
```

### 3.4 Dependency Resolution

Dependencies are resolved by hash, not by name or version number.

```
// In the codebase, this function:
pub fn transfer(balance: Field, amount: Field) -> Field {
    let verified = verify_merkle(root, leaf, index, depth)
    assert(verified)
    balance - amount
}

// Is stored as:
FnDef {
    params: [(Field), (Field)],
    body: [
        Let(#0, Call(#a7f3b2c1, [Var(root), Var(leaf), Var(index), Var(depth)])),
        Assert(Var(#0)),
        Sub(Var(balance), Var(amount))
    ]
}
// Hash: #c4e9d1a8
```

The `Call(#a7f3b2c1, ...)` node pins the dependency to an exact implementation. There is no ambiguity about which `verify_merkle` is being called — it's the one with hash `#a7f3b2c1`, forever.

**Diamond dependency problem: eliminated.**

If library A and library B both depend on `verify_merkle`, there are exactly two cases:
1. They depend on the same hash → one copy, no conflict
2. They depend on different hashes → different functions, both coexist

There is no situation where "library A wants version 1.2 and library B wants version 1.3" because versions don't exist. Hashes exist.

### 3.5 Source Text as a View

Source text files (`.tri` files) are a **view** into the codebase, not the source of truth. The codebase can render any function as text on demand:

```bash
# Render a function by name
trident view verify_merkle

# Render a function by hash
trident view #a7f3b2c1

# Render with all dependencies inlined
trident view verify_merkle --inline-deps

# Render with proving cost annotations
trident view verify_merkle --costs --target triton

# Render in a specific style (formatting is metadata, not identity)
trident view verify_merkle --style compact
```

Text files can still be used as the editing interface (just like Unison). The developer writes `.tri` files in their editor, and the codebase manager watches for changes, parses them, and integrates them into the database.

**Formatting is not identity.** The codebase stores the AST, not the text. When rendering to text, any formatting style can be applied. There is no "tabs vs spaces" debate — the canonical form is the AST.

---

## 4. Verification Cache

### 4.1 Verified Once, Valid Forever

When a function is verified (all assertions proven, all specifications satisfied), the result is cached by verification hash. Since the hash uniquely identifies the exact computation plus its specifications, and the definition is immutable, the verification result is valid forever.

```
Verification Cache:
  #e5f6g7h8 (verify_merkle + specs) →
    status: VERIFIED
    properties:
      - "all assertions hold for all valid inputs"
      - "witness existence proven for divine_digest()"
      - "loop invariant verified (bounded model checking, 64 iterations)"
    solver: z3-4.13.0
    time: 2.1s
    certificate: #cert_8b2f...
    timestamp: 2026-02-10T12:00:00Z
```

**When a developer writes a function that happens to match an existing verified hash:**

```
$ trident add my_merkle_checker

  ✓ Typechecks
  ✓ Hash: #a7f3b2c1
  ✓ Matches existing verified definition
  ✓ Verification: CACHED (verified by alice, 2026-01-15)
  ✓ Proving cost: triton=847cc, miden=623cc (cached)
  
  Skipped verification (0.0s instead of 2.1s)
```

### 4.2 Incremental Re-Verification

When a dependency changes, only the affected functions need re-verification. The codebase tracks the dependency DAG:

```
#a7f3b2c1 (verify_merkle) — verified ✓
    ↑ used by
#c4e9d1a8 (transfer_token) — verified ✓
    ↑ used by
#f7a2b1c3 (main) — verified ✓
```

If `verify_merkle` is edited (new hash `#b8c4d2e5`):
- `transfer_token` gets a new hash (because its dependency hash changed)
- `main` gets a new hash (because `transfer_token`'s hash changed)
- But `verify_merkle`'s OTHER dependents (if any) that weren't affected by the change: untouched

The re-verification propagates up the DAG, but only re-verifies what actually changed. If the new `verify_merkle` has the same type signature and strengthened postconditions, downstream verification may succeed instantly (proven by the stronger upstream guarantee).

### 4.3 Compilation Cache

Compilation results are also cached by (source hash, target):

```
Compilation Cache:
  (#a7f3b2c1, triton) →
    tasm_bytecode: [...]
    target_hash: Tip5(#t_8f2a3b4c5d...)
    proving_cost: { processor: 847, hash: 12, u32: 34, ... }
    padded_height: 1024
    compiled_at: 2026-02-10T12:00:00Z
    
  (#a7f3b2c1, miden) →
    masm_bytecode: [...]
    target_hash: RPO(#m_3c7b9a1d2e...)
    proving_cost: { chiplets: 623, ... }
    padded_height: 1024
    compiled_at: 2026-02-10T12:01:00Z
```

First compile to a new target takes full compilation time. Every subsequent request for the same (hash, target) pair is instant — return the cached artifact.

### 4.4 Global Sharing

Caches can be shared across developers and teams. A global cache server means:

- Developer A compiles `verify_merkle` for Triton. Result cached.
- Developer B (different team, different project) uses the same function. Compilation is instant — pulled from global cache.
- Developer C verifies the function with a specification. Certificate cached.
- Developer D writes the same specification independently. Verification result: "already verified, certificate available."

This creates network effects: the more developers use content-addressed Trident, the more the global cache covers, and the faster everyone's development cycle becomes.

---

## 5. The Registry

### 5.1 Architecture

The Trident Registry is a global, decentralized database of content-addressed functions, their verification status, compilation artifacts, and metadata.

```
┌─────────────────────────────────────────────────────┐
│                  TRIDENT REGISTRY                   │
│                                                     │
│  Functions (keyed by source hash):                  │
│    #a7f3b2c1 → {                                    │
│      type: (Digest, Digest, U32, U32) -> (),        │
│      deps: [#f8a2b1c3, #e2f1b3a9],                 │
│      verification: VERIFIED (cert: #cert_8b2f),     │
│      targets: {                                     │
│        triton: { cost: 847cc, hash: #t_8f2a... },   │
│        miden:  { cost: 623cc, hash: #m_3c7b... },   │
│        cairo:  { cost: 1204,  hash: #c_1d4e... },   │
│      },                                             │
│      metadata: {                                    │
│        names: ["verify_merkle", "check_proof"],     │
│        authors: ["alice", "bob"],                   │
│        description: "Merkle proof verification",    │
│        tags: ["crypto", "merkle", "verification"],  │
│        usage_count: 47,                             │
│      }                                              │
│    }                                                │
│                                                     │
│  Search indices:                                    │
│    by type signature                                │
│    by tags and description                          │
│    by verification status                           │
│    by proving cost per target                       │
│    by dependency graph                              │
└─────────────────────────────────────────────────────┘
```

### 5.2 Registry Operations

```bash
# Publish a verified function to the registry
trident publish verify_merkle
  Published #a7f3b2c1 (verified, 3 targets)

# Search by type signature
trident search --type "(Digest, Digest, U32, U32) -> ()"
  #a7f3b2c1  verify_merkle      verified  triton=847cc  miden=623cc
  #d4e5f6a7  merkle_check_v2    verified  triton=792cc  miden=601cc
  #b2c3d4e5  old_merkle_verify  unverified

# Search by property
trident search --property "conservation" --verified
  #c4e9d1a8  transfer_token     verified  "sum(outputs) == sum(inputs)"
  #f1a2b3c4  atomic_swap        verified  "total_value preserved"

# Pull a function by hash
trident pull #a7f3b2c1
  Pulled verify_merkle (#a7f3b2c1)
  ✓ Verification certificate: valid
  ✓ Available targets: triton, miden, cairo

# Find the cheapest implementation for a given target
trident search --type "(Digest, Digest, U32, U32) -> ()" --target miden --sort cost
  #d4e5f6a7  merkle_check_v2    601cc
  #a7f3b2c1  verify_merkle      623cc
```

### 5.3 On-Chain Registry

The registry itself can be stored on-chain as a Merkle tree of function hashes, verification certificates, and metadata hashes. This creates a trustless, censorship-resistant code repository.

```
Registry Root (on-chain):
  Merkle tree of:
    leaf[0] = Hash(#a7f3b2c1 || type_hash || deps_hash || cert_hash || metadata_hash)
    leaf[1] = Hash(#c4e9d1a8 || type_hash || deps_hash || cert_hash || metadata_hash)
    ...
```

A smart contract can verify: "this computation (identified by source hash #a7f3b2c1) is registered and has a valid verification certificate" — all on-chain, trustlessly.

### 5.4 Cross-Chain Equivalence

The registry provides the bridge for cross-chain equivalence proofs:

```
Claim: "The program on Triton (#t_8f2a...) computes the same function as the program on Miden (#m_3c7b...)"

Proof:
  1. Registry entry for source hash #a7f3b2c1 maps to:
     - Triton target hash: #t_8f2a...  ✓
     - Miden target hash: #m_3c7b...   ✓
  2. Both target hashes derive from the same source hash
  3. The compiler is deterministic (verifiable)
  4. Therefore: same computation on both chains  QED
```

This is a **construction-based equivalence proof** — not a testing-based claim. It's as strong as the compiler's correctness.

---

## 6. Semantic Equivalence

### 6.1 Beyond Syntactic Hashing

Content addressing based on AST hashing catches syntactic equivalence — same code structure, different names. But Trident can go further: **semantic equivalence**.

Two functions with different ASTs but identical computational behavior should ideally share a hash. This is decidable for Trident programs because they're bounded, first-order, and operate over finite fields.

```
// Syntactically different:
fn double_a(x: Field) -> Field { x + x }
fn double_b(x: Field) -> Field { x * 2 }

// Semantically equivalent (for all x in F_p, x + x == x * 2)
// Should they share a hash?
```

### 6.2 Equivalence Detection Strategy

Full semantic hashing (hashing the mathematical function rather than the AST) is expensive. Instead, use a layered approach:

**Level 1: Syntactic hash (always computed)**
- Hash the normalized AST as described above
- Fast, deterministic, catches exact structural matches

**Level 2: Equivalence queries (on demand)**
- When two functions have different syntactic hashes but the same type signature, offer to check semantic equivalence
- Use the verification engine: prove `f(x) == g(x)` for all x
- If proven equivalent, link the hashes in the registry as aliases

```
$ trident add double_b

  ✓ Typechecks
  ✓ Hash: #d5e6f7a8 (new)
  ℹ Similar function found: double_a (#c4d5e6f7)
    Same type signature: (Field) -> Field
    Checking equivalence... EQUIVALENT (algebraic: 0.05ms)
    
  Link as alias? (y/n) y
  
  Linked: #d5e6f7a8 ≡ #c4d5e6f7
  Verification and compilation results shared.
```

**Level 3: Canonical forms (research)**
- For a subset of Trident (pure field arithmetic, no loops), canonical polynomial normal forms exist
- `x + x` and `x * 2` both normalize to the polynomial `2x`
- This is a future optimization, not required for the initial system

### 6.3 Equivalence Classes in the Registry

The registry groups semantically equivalent functions into equivalence classes:

```
Equivalence class [merkle_verify]:
  #a7f3b2c1 — verify_merkle (by alice)
  #b8c4d2e5 — check_merkle_proof (by bob)
  #c9d5e3f6 — merkle_auth (by carol)
  
  All three are semantically equivalent (proven by SMT solver).
  Any verification certificate for one applies to all.
  Proving costs may differ (different AST structure → different instruction count).
  The cheapest per target is recommended as the canonical implementation.
```

---

## 7. LLM Integration

### 7.1 Content Addressing Supercharges LLM Code Generation

The registry + verification cache transforms how LLMs generate ZK code:

**Before content addressing:**
```
LLM generates code → compiler verifies (2-30 seconds) → iterate
```

**With content addressing:**
```
LLM generates code → compiler hashes → registry lookup:
  CASE 1: Hash matches existing verified function → DONE (0 seconds)
  CASE 2: Hash matches unverified function → verify once, share result
  CASE 3: New hash → verify, publish to registry for future reuse
```

Over time, the registry covers most common ZK computation patterns. The LLM increasingly hits Case 1 — instant verified code by recognizing that the requested computation already exists in the registry.

### 7.2 Search-and-Compose Pattern

Instead of generating code from scratch, the LLM can search the registry and compose from verified building blocks:

```
Human: "Create a confidential token transfer"

LLM reasoning:
  1. This requires: Merkle verification, balance checking, conservation proof
  2. Search registry:
     - verify_merkle: #a7f3b2c1 (verified, 847cc on Triton)
     - check_balance: #d4e5f6a7 (verified, 120cc on Triton)
  3. Compose these with new glue code
  4. Only the glue code needs verification — components are pre-verified

LLM output:
  pub fn confidential_transfer(...) {
      verify_merkle(...)     // #a7f3b2c1 — pre-verified
      check_balance(...)     // #d4e5f6a7 — pre-verified
      // New: 5 lines of glue code — needs verification
      let sender_new = sender_balance - amount
      let receiver_new = receiver_balance + amount
      assert(sender_new + receiver_new == sender_balance + receiver_balance)
  }

Compiler: only the composition needs verification (the components are cached)
  → Verified in 0.3s instead of 3.7s
```

### 7.3 The Accumulating Intelligence

Every interaction adds to the registry:
- LLM generates a new verified function → published to registry
- Next time any LLM (or human) needs similar functionality → found in registry
- The registry becomes a growing library of verified ZK computation primitives
- Eventually, most common ZK patterns are covered, and generation becomes assembly from verified parts

This is a flywheel that gets faster over time.

---

## 8. The Codebase Manager (UCM)

### 8.1 Interactive Development

The Trident Codebase Manager (`trident ucm`) is the interactive interface to the content-addressed codebase:

```
$ trident ucm

Welcome to Trident UCM (Content-Addressed Provable Computation)
Codebase: ./my_project (142 definitions, 128 verified)
Connected to registry: registry.trident.dev (1.2M definitions)

trident> ls
  verify_merkle      #a7f3b2c1  verified  ✓
  transfer_token     #c4e9d1a8  verified  ✓
  batch_transfer     #f7a2b1c3  verified  ✓
  experimental_swap  #e1d3c5a7  UNVERIFIED ✗
  main               #b4c5d6e7  verified  ✓

trident> info verify_merkle
  Hash:           #a7f3b2c1
  Type:           (Digest, Digest, U32, U32) -> ()
  Dependencies:   hash (#f8a2b1c3), divine_digest (#e2f1b3a9)
  Dependents:     transfer_token, batch_transfer, main
  Verified:       ✓ (8 properties, cert #cert_8b2f)
  Proving cost:   triton=847cc  miden=623cc  cairo=1204 steps
  Registry:       published ✓ (47 users)
  Equivalents:    check_proof (#b8c4d2e5 by bob), merkle_auth (#c9d5e3f6 by carol)
  Last modified:  2026-01-15 by alice

trident> edit transfer_token
  [opens in editor]
  
trident> update
  Parsing transfer_token... ✓
  Type checking... ✓
  New hash: #c4e9d1a9 (changed)
  Verifying... ✓ (5/5 properties, 1.3s)
  Propagating to dependents:
    batch_transfer: new hash #f7a2b1c4, re-verifying... ✓ (0.8s)
    main: new hash #b4c5d6e8, re-verifying... ✓ (0.4s)
  
  All dependents updated and verified. ✓

trident> cost transfer_token --all-targets
  triton:  312cc (processor=280, hash=12, u32=20) — padded height: 512
  miden:   289cc (chiplets=210, stack=79) — padded height: 512
  cairo:   445 steps — padded height: 512
  openvm:  ~2800 cycles (estimated)

trident> publish transfer_token
  Publishing #c4e9d1a9 to registry...
  ✓ Published (verified, 3 targets)

trident> search --type "(Field, Field) -> Field" --verified --target triton --sort cost
  #d1e2f3a4  field_add_checked     23cc    "overflow-safe addition"
  #e2f3a4b5  field_mul_checked     31cc    "overflow-safe multiplication"
  #f3a4b5c6  safe_divide           89cc    "division with zero-check"
  #a4b5c6d7  sqrt_verify           156cc   "verified square root"

trident> generate "verify that a batch of 10 transfers all preserve total supply"
  Searching registry for components...
    ✓ Found: transfer_token (#c4e9d1a9) — verified
    ✓ Found: sum_array (#g1h2i3j4) — verified
  Generating composition... (LLM: attempt 1)
  Verifying... ✓ (3 properties, 0.9s)
  
  Added: batch_verify_conservation (#k5l6m7n8)
  ✓ Verified, published to registry
```

### 8.2 Project Configuration

```toml
# trident.toml
[project]
name = "neptune-consensus"
edition = "2026"

[codebase]
path = ".trident/codebase.db"    # Local codebase database
registry = "registry.trident.dev" # Remote registry

[targets]
default = "triton"
supported = ["triton", "miden"]

[verification]
mode = "standard"                 # quick | standard | exhaustive
timeout = 30                      # seconds per assertion
auto_verify = true                # verify on every update

[sharing]
auto_publish = false              # require explicit `publish` command
share_compilation = true          # share compilation cache with registry
share_verification = true         # share verification results with registry
```

---

## 9. Implementation Plan

### 9.1 Phase 1: Local Content Addressing (4-6 weeks)

**Goal:** Functions are hashed and stored in a local database. Compilation and verification caching works.

| Task | Effort |
|------|--------|
| AST normalization (de Bruijn indices, dependency hash substitution, canonicalization) | 2 weeks |
| BLAKE3 hashing of serialized normalized AST | 3 days |
| Local codebase database (SQLite or similar) | 1 week |
| Compilation cache by (hash, target) | 3 days |
| Verification cache by verification hash | 3 days |
| `trident hash` command (show hash of any function) | 1 day |
| Integration with existing `trident build` (cache lookups) | 3 days |
| Tests: hash stability, normalization correctness, cache hit/miss | 1 week |

**Deliverable:** `trident build` uses content-addressed caching. Unchanged functions are never recompiled or re-verified. Renaming a function doesn't trigger recompilation.

### 9.2 Phase 2: Codebase Manager (6-8 weeks)

**Goal:** Interactive development workflow with the content-addressed codebase.

| Task | Effort |
|------|--------|
| `trident ucm` interactive CLI (REPL-like interface) | 2 weeks |
| Edit/update workflow (watch files, parse, hash, store) | 1 week |
| Dependency tracking and automatic propagation | 1 week |
| "Todo list" for broken dependents (type-change propagation) | 3 days |
| Pretty-printing from stored AST (view command) | 3 days |
| Name management (rename, alias, deprecate) | 3 days |
| History (show all versions of a name, diff between hashes) | 1 week |
| Cost analysis per target (info command) | 3 days |

**Deliverable:** Developers interact with the codebase through UCM. Editing feels like normal development but with instant verification feedback, automatic dependency propagation, and perfect caching.

### 9.3 Phase 3: Global Registry (6-8 weeks)

**Goal:** Shared registry for cross-developer, cross-project code sharing.

| Task | Effort |
|------|--------|
| Registry server (simple HTTP API over a hash-keyed database) | 2 weeks |
| `trident publish` / `trident pull` commands | 1 week |
| Type-signature search index | 1 week |
| Property and tag search | 3 days |
| Verification certificate publishing and validation | 1 week |
| Cross-target compilation artifact sharing | 3 days |
| Registry integration in UCM (search, pull, publish from REPL) | 1 week |

**Deliverable:** Developers can publish verified functions and pull from a global registry. Compilation and verification results are shared across the community.

### 9.4 Phase 4: Semantic Equivalence (4-6 weeks)

**Goal:** Detect and link semantically equivalent functions.

| Task | Effort |
|------|--------|
| Equivalence checking via verification engine (prove f(x) == g(x) ∀x) | 2 weeks |
| Equivalence class management in registry | 1 week |
| Automatic equivalence suggestion (same type, different hash → offer check) | 1 week |
| Canonical forms for pure field arithmetic (polynomial normalization) | 2 weeks (research) |

### 9.5 Phase 5: On-Chain Registry (4-6 weeks)

**Goal:** Trustless, decentralized registry stored on-chain.

| Task | Effort |
|------|--------|
| Merkle tree registry contract (in Trident, naturally) | 2 weeks |
| On-chain verification certificate validation | 1 week |
| Cross-chain equivalence proof generation | 1 week |
| Registry synchronization (local ↔ on-chain) | 1 week |

### 9.6 Timeline

```
Month 1-2:     Phase 1 — Local content addressing
Month 2-4:     Phase 2 — Codebase Manager
Month 4-6:     Phase 3 — Global Registry
Month 6-8:     Phase 4 — Semantic Equivalence
Month 8-10:    Phase 5 — On-Chain Registry
```

---

## 10. How It All Fits Together

Content-addressed Trident is not a standalone feature. It's the connective tissue that makes every other piece of the Trident ecosystem work better.

```
┌─────────────────────────────────────────────────────────┐
│                    DEVELOPER                            │
│  "I need a confidential token transfer"                 │
└────────────────────────┬────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────┐
│              LLM CODE GENERATION                        │
│  Search registry → compose from verified parts          │
│  Generate new glue code → verify                        │
│  Result: verified function with hash #new_hash          │
└────────────────────────┬────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────┐
│            CONTENT-ADDRESSED CODEBASE                   │
│  Store by hash. Cache compilation. Cache verification.  │
│  Track dependencies. Propagate changes.                 │
│  Names are metadata. Hashes are identity.               │
└────────────────────────┬────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────┐
│            EMBEDDED FORMAL VERIFICATION                 │
│  Prove assertions for all inputs. Cache by hash.        │
│  Generate certificates. Attach to hash permanently.     │
│  Verification result valid forever (immutable code).    │
└────────────────────────┬────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────┐
│           MULTI-TARGET COMPILATION                      │
│  Compile to Triton, Miden, Cairo, OpenVM.               │
│  Each target artifact has its own hash.                 │
│  Source hash → target hash mapping in registry.         │
│  Cross-chain equivalence by shared source hash.         │
└────────────────────────┬────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────┐
│              GLOBAL REGISTRY                            │
│  Publish verified, compiled functions.                  │
│  Search by type, property, cost, target.                │
│  Semantic equivalence detection.                        │
│  On-chain registry for trustless code sharing.          │
│  Network effects: more users → better cache → faster.   │
└────────────────────────┬────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────┐
│                  DEPLOYMENT                             │
│  Deploy to any zkVM. Hash is the address.               │
│  Verification certificate travels with the code.        │
│  Auditors verify the hash, not the name.                │
│  Cross-chain references by source hash.                 │
│  Upgrade = new hash + pointer update.                   │
└─────────────────────────────────────────────────────────┘
```

Each layer reinforces the others:
- Content addressing makes verification caching possible
- Verification caching makes LLM generation practical (fast feedback)
- LLM generation populates the registry with verified functions
- The registry makes future LLM generation faster (search before generate)
- Multi-target compilation shares the source hash across chains
- The on-chain registry makes cross-chain equivalence trustless

The result: a self-reinforcing ecosystem where provable computation becomes easier to write, verify, share, and deploy over time.

---

*Content-Addressed Trident — Code is what it computes, forever.*