# Trident Verified: Embedded Formal Verification for Provable Contracts

**Design Document — v0.1 Draft**
**February 2026**

*When the language is simple enough, machines can prove correctness. When correctness is provable, machines can write correct code.*

---

## 1. The Core Idea

Today, formally verifying a smart contract requires a PhD-level expert spending weeks in Coq or Lean, manually constructing proofs that the code satisfies its specification. This process is so expensive that virtually nobody does it — even for contracts holding billions of dollars.

Trident can eliminate the expert, eliminate the proof assistant, and eliminate the manual effort entirely.

**The insight:** Trident programs are bounded, first-order, heap-free computations over finite fields. This means the verification problem is **decidable** — the compiler can automatically prove that a contract satisfies its specification for all possible inputs, not by testing, but by exhaustive symbolic analysis. No human constructs a proof. The compiler IS the theorem prover.

**The consequence:** If the compiler can verify any Trident contract in seconds, then an LLM can write provably correct contracts. The LLM generates code, the compiler checks it, failures become feedback, and the loop converges on verified code. The language's simplicity means the LLM masters it completely. The compiler's verification means the LLM doesn't need to be perfect — it just needs to be close enough for the verifier to guide it.

```
Human intent (natural language)
    → LLM generates Trident code + spec
        → Compiler verifies (seconds)
            → PASS: deploy to any zkVM
            → FAIL + counterexample: LLM fixes and resubmits
```

This is the first programming environment where LLMs can reliably produce formally verified code, because the verification is decidable and the language is small enough to fit in a single context window.

---

## 2. Why Trident — And Only Trident

### 2.1 What Makes Formal Verification Hard

Traditional formal verification must cope with:

| Problem | Why it's hard | Trident's answer |
|---------|--------------|-----------------|
| Termination | Must prove every loop and recursion terminates | **No recursion. All loops have compile-time bounds.** |
| Aliasing | Two pointers to same memory — which write wins? | **No heap. No pointers. No aliasing.** |
| Dynamic dispatch | Don't know what code runs until runtime | **First-order only. No closures, no vtables.** |
| Unbounded state | Program can allocate arbitrary memory | **Fixed memory layout. Compiler knows every variable.** |
| Side effects | Must model filesystem, network, database | **Three side effects: `pub_read`, `pub_write`, `divine`. All pure data.** |
| Undecidability | Arithmetic over integers is undecidable (Gödel) | **Arithmetic over finite fields is decidable.** |
| Concurrency | Interleavings explode exponentially | **Single-threaded. Sequential execution.** |

Every row eliminates an entire class of verification difficulty. What remains is: polynomial equations over a finite field, bounded iteration, and existential quantification over witness values. This is a solved problem in automated reasoning.

### 2.2 Trident Programs Are Already Constraint Systems

A zkVM doesn't execute a program the way a CPU does. It converts the program into a system of polynomial constraints and proves that a valid assignment exists. Trident is designed for this: every program IS a constraint system.

This means verification and execution use the same mathematical object. When the compiler verifies "this assertion holds for all inputs," it's doing the same thing the zkVM prover does — just symbolically instead of concretely.

```
Trident program
    ├── Concrete execution (zkVM prover): "here is ONE valid input/output pair"
    └── Symbolic verification (compiler): "ALL valid inputs produce valid outputs"
```

Both paths traverse the same constraint system. The verification infrastructure is not bolted on — it's inherent to the language's design.

### 2.3 The Finite Field Advantage

Solidity operates over 256-bit integers with overflow. Reasoning about overflow requires tracking 2²⁵⁶ possible states per variable.

Trident operates over a finite field `F_p`. Every value is in `{0, 1, ..., p-1}`. Arithmetic wraps cleanly: `a + b` is always `(a + b) mod p`. There's no overflow — there's modular arithmetic with well-defined semantics.

For the Goldilocks field (p = 2⁶⁴ - 2³² + 1), SMT solvers can reason about field arithmetic using bit-vector theories. For arbitrary fields, specialized field-arithmetic decision procedures exist. In both cases, the solver terminates with a definitive answer: valid or counterexample.

---

## 3. Specification Language

### 3.1 Design Principle: Assertions Are Specifications

Trident already has `assert()`. Every assertion is a specification: "this property must hold at this program point, for all executions." The verification system doesn't need a separate spec language — it needs to prove existing assertions.

```
pub fn verify_transfer(
    sender_balance: Field,
    amount: Field,
    receiver_balance: Field,
) {
    // These assertions ARE the formal spec
    assert(amount != 0)                                    // S1: non-zero transfer
    
    let sender_new = sender_balance - amount
    let receiver_new = receiver_balance + amount
    
    assert(sender_new + receiver_new                       // S2: conservation
        == sender_balance + receiver_balance)
    
    pub_write(sender_new)
    pub_write(receiver_new)
}
```

The compiler proves S1 and S2 hold for all valid inputs. S2 is trivially true by field arithmetic (addition is commutative and associative). S1 depends on the input — the compiler reports: "S1 cannot be proven without a precondition on `amount`."

### 3.2 Preconditions and Postconditions

For properties that depend on the relationship between inputs and outputs, add lightweight annotations:

```
#[requires(amount != 0)]
#[requires(sender_balance >= amount)]        // field comparison
#[ensures(result.sender == sender_balance - amount)]
#[ensures(result.receiver == receiver_balance + amount)]
pub fn transfer(sender_balance: Field, amount: Field, receiver_balance: Field) -> TransferResult {
    let sender_new = sender_balance - amount
    let receiver_new = receiver_balance + amount
    TransferResult { sender: sender_new, receiver: receiver_new }
}
```

`#[requires]` constrains inputs — the function is only obligated to work for inputs satisfying the precondition. `#[ensures]` constrains outputs — the compiler proves the postcondition holds for all valid inputs.

These annotations compile to nothing. They exist only for the verifier. Zero proving cost.

### 3.3 Loop Invariants

For loops, the compiler needs to know what property is maintained across iterations:

```
#[ensures(result == n * (n - 1) / 2)]
pub fn sum_to(n: U32) -> Field {
    let mut total: Field = 0
    
    #[invariant(total == i * (i - 1) / 2)]
    for i in 0..n bounded 1000 {
        total = total + as_field(i)
    }
    
    total
}
```

The compiler verifies:
1. The invariant holds at loop entry (i=0: total=0 = 0)
2. If the invariant holds at iteration i, it holds at iteration i+1
3. The invariant at loop exit (i=n) implies the postcondition

For small bounds, the compiler can also simply unroll the loop and check directly — brute force works when iteration counts are bounded.

### 3.4 Witness Existence (The `divine()` Problem)

`divine()` introduces non-determinism: the prover supplies a value, and the program asserts properties about it. Verification must prove that a valid witness EXISTS for all valid public inputs.

```
/// Verify a square root witness
#[requires(is_quadratic_residue(x))]
#[ensures(result * result == x)]
pub fn sqrt_verify(x: Field) -> Field {
    let s: Field = divine()      // prover supplies the square root
    assert(s * s == x)           // constraint on the witness
    s
}
```

The compiler proves: "for all `x` satisfying `is_quadratic_residue(x)`, there exists `s` in `F_p` such that `s * s == x`."

This is an existential quantifier over a finite domain — decidable. The solver either finds a witness construction or reports: "no valid witness exists for input x = [counterexample]."

**This catches real bugs.** If the developer forgets the precondition:

```
// BUG: not all field elements have square roots
pub fn sqrt_verify(x: Field) -> Field {
    let s: Field = divine()
    assert(s * s == x)
    s
}
```

The compiler reports:

```
error[V001]: witness existence cannot be proven
  --> main.tri:3:5
   |
3  |     let s: Field = divine()
   |     ^^^^^^^^^^^^^^^^^^^^^^^^
4  |     assert(s * s == x)
   |     ^^^^^^^^^^^^^^^^^^
   |
   = counterexample: x = 7 (quadratic non-residue mod p)
   = help: add #[requires(is_quadratic_residue(x))]
```

### 3.5 Contract-Level Properties

For multi-function contracts, specify invariants that must hold across all public entry points:

```
#[contract_invariant(
    total_supply() == sum_of_all_balances()
)]
module token

pub fn mint(to: Digest, amount: Field) { ... }
pub fn transfer(from: Digest, to: Digest, amount: Field) { ... }
pub fn burn(from: Digest, amount: Field) { ... }
```

The compiler verifies that every public function, starting from any state satisfying the invariant, produces a state that also satisfies the invariant.

### 3.6 Specification Summary

| Annotation | Scope | What compiler proves |
|-----------|-------|---------------------|
| `assert(P)` | Statement | P holds at this point for all executions reaching it |
| `#[requires(P)]` | Function | Constrains valid inputs (assumed, not proven) |
| `#[ensures(P)]` | Function | P holds for all valid inputs when function returns |
| `#[invariant(P)]` | Loop | P holds at every iteration entry |
| `#[contract_invariant(P)]` | Module | P is preserved by every public function |
| `#[pure]` | Function | No I/O side effects (enables more aggressive reasoning) |

---

## 4. Verification Engine Architecture

### 4.1 Overview

```
Trident Source (.tri)
    → Frontend (lexer, parser, typeck)                    [existing]
    → Symbolic Lowering (sym.rs)                          [new]
        → Symbolic Constraint System (SCS)
            → Field Arithmetic Solver                     [new]
            → SMT Backend (Z3/CVC5)                       [integration]
            → Bounded Model Checker (unrolling)            [new]
    → Verification Report
        → VERIFIED / COUNTEREXAMPLE / TIMEOUT
    → [if verified] TASM Emission                         [existing]
```

### 4.2 Symbolic Lowering

Convert the type-checked AST into a symbolic constraint system. Each variable becomes a symbolic value. Each operation becomes a constraint.

```
// Trident source
let a: Field = pub_read()
let b: Field = pub_read()
let c = a * b
assert(c != 0)

// Symbolic constraint system
variables: a ∈ F_p, b ∈ F_p
constraints:
    c = a * b          (definition)
    c ≠ 0              (assertion)
    
query: ∃ a, b ∈ F_p such that c = a * b ∧ c = 0?
    if YES → counterexample (assertion can fail)
    if NO  → verified (assertion always holds)
    
result: YES, a=0, b=42 → counterexample found
    "assert(c != 0) can fail when a = 0"
```

**Control flow** is handled by path splitting:

```
if condition {
    // path A constraints
} else {
    // path B constraints
}
// merge: property must hold on BOTH paths
```

**Bounded loops** are handled by unrolling OR by invariant checking:

```
// Small bound (≤ 64): unroll
for i in 0..32 { ... }
→ 32 copies of the body, each with constraints

// Large bound (> 64): invariant required
for i in 0..n bounded 1000 { ... }
→ check invariant holds at entry
→ check invariant preserved by one iteration
→ check invariant at exit implies postcondition
```

**`divine()` values** become existentially quantified:

```
let s: Field = divine()
→ ∃ s ∈ F_p such that [subsequent constraints on s]
```

### 4.3 Solver Strategy

Different properties benefit from different solving strategies:

| Property type | Strategy | Expected performance |
|--------------|----------|---------------------|
| Field arithmetic identities | Polynomial identity testing | Milliseconds |
| U32 range checks | Bit-vector SMT | Seconds |
| Loop invariant checking | Hoare logic + field solver | Seconds |
| Bounded loop unrolling (≤64 iterations) | Concrete unrolling + field solver | Seconds to minutes |
| Witness existence (`divine()`) | ∃-quantifier elimination | Seconds to minutes |
| Conservation laws | Linear algebra over F_p | Milliseconds |
| Merkle proof correctness | Hash-opaque reasoning + structure | Seconds |

**Solver layering:**

1. **Fast algebraic pass**: Check polynomial identities, linear conservation laws, and trivially true assertions using direct algebraic reasoning. No external solver needed. Catches ~60% of verification goals.

2. **Bounded model checking**: For loops with small bounds, unroll and check. For programs with few `divine()` inputs, enumerate witness space. Catches ~25% of remaining goals.

3. **SMT solver**: For everything else, encode as SMT queries. Use Z3's finite field theory or bit-vector encoding for Goldilocks. Catches ~90% of remaining goals.

4. **Timeout**: If the solver doesn't terminate within a configurable bound (default: 30 seconds per assertion), report "UNVERIFIED" with the specific assertion and a suggestion to add a stronger invariant or precondition.

### 4.4 Verification Modes

```bash
# Quick check: algebraic pass only (milliseconds)
trident verify main.tri --quick

# Standard: algebraic + BMC + SMT (seconds)
trident verify main.tri

# Exhaustive: all strategies, longer timeout (minutes)
trident verify main.tri --exhaustive

# Specific property
trident verify main.tri --property "transfer::conservation"

# Generate verification certificate
trident verify main.tri --certificate
```

### 4.5 Verification Report

```
$ trident verify token.tri

Verifying module token (3 functions, 12 assertions, 2 contract invariants)

  transfer:
    ✓ requires(amount != 0)                              [precondition]
    ✓ requires(sender_balance >= amount)                  [precondition]
    ✓ assert(sender_new + receiver_new == total)          [algebraic: 0.2ms]
    ✓ ensures(result.sender == sender_balance - amount)   [algebraic: 0.1ms]
    ✓ ensures(result.receiver == receiver_balance + amount) [algebraic: 0.1ms]
    ✓ preserves contract_invariant(total_supply)          [SMT: 1.2s]

  mint:
    ✓ requires(amount > 0)                               [precondition]
    ✓ assert(new_balance == old_balance + amount)         [algebraic: 0.1ms]
    ✗ preserves contract_invariant(total_supply)          [SMT: 0.8s]
        COUNTEREXAMPLE: total_supply overflows when
        current_supply = p-1, amount = 2
        SUGGESTION: add #[requires(current_supply + amount < p)]

  verify_merkle:
    ✓ invariant(current == path_hash(leaf, siblings[0..i])) [BMC: 3.1s]
    ✓ ensures(verified_root == expected_root)              [algebraic: 0.3ms]
    ✓ witness existence for divine_digest()               [SMT: 2.4s]

RESULT: 11/12 verified, 1 failed
  Failed: mint::contract_invariant (counterexample provided)
  Time: 8.3s total
```

### 4.6 Verification Certificates

When verification succeeds, the compiler can produce a machine-checkable certificate — a compact proof artifact that third parties can verify without re-running the solver.

```
{
  "program": "token.tri",
  "hash": "0x3a7f...",
  "target": "triton",
  "verified_properties": [
    {
      "name": "transfer::conservation",
      "type": "algebraic_identity",
      "proof": "a - x + b + x = a + b by commutativity and cancellation",
      "status": "verified"
    },
    {
      "name": "transfer::contract_invariant",
      "type": "smt_unsat",
      "solver": "z3-4.13",
      "proof_hash": "0x8b2c...",
      "status": "verified"
    }
  ],
  "timestamp": "2026-02-10T12:00:00Z",
  "certificate_version": "1.0"
}
```

Certificates can be stored on-chain, attached to contract deployments, or published for auditors. A lightweight certificate checker can verify them without running Z3.

---

## 5. LLM-Verified Contract Generation

### 5.1 Why This Works

Three conditions make LLM-driven verified contract generation feasible:

**Condition 1: The language is small enough for complete mastery.**

Trident has 5 primitive types, ~15 expression forms, ~8 statement types, and ~10 built-in functions. The entire language specification fits in a single document (~5,000 words). An LLM can internalize the complete language in its context window — there's nothing to hallucinate about, no obscure corners to get wrong.

For comparison: Solidity has >50 types, inheritance, modifiers, assembly blocks, storage vs memory vs calldata, delegatecall, selfdestruct, proxy patterns, and hundreds of edge cases. No LLM masters all of this reliably.

**Condition 2: The compiler provides perfect, fast feedback.**

When the LLM generates incorrect code, the verifier doesn't just say "wrong." It says:
- WHICH assertion failed
- A specific counterexample (input values that cause failure)
- A suggestion for how to fix it

This is dramatically better feedback than test failures or runtime errors. The LLM can interpret the counterexample, understand why the code is wrong, and fix exactly the right thing.

**Condition 3: The specification IS the code.**

The LLM doesn't need to write code AND proofs (as in Coq/Lean). It writes code with assertions. The assertions are the specification. The compiler proves them. If the LLM writes correct assertions but buggy logic, the verifier catches it. If the LLM writes correct logic but weak assertions, the human adds stronger specs and the LLM regenerates.

### 5.2 The LLM Verification Loop

```
┌─────────────────────────────────────────────────┐
│                  HUMAN                          │
│  "Create a token transfer that preserves        │
│   total supply and prevents negative balances"  │
└─────────────────────┬───────────────────────────┘
                      │ natural language spec
                      ▼
┌─────────────────────────────────────────────────┐
│                   LLM                           │
│  Generates Trident code with:                   │
│  - #[requires] preconditions                    │
│  - #[ensures] postconditions                    │
│  - #[contract_invariant] module invariants      │
│  - inline assert() checks                       │
└─────────────────────┬───────────────────────────┘
                      │ .tri source file
                      ▼
┌─────────────────────────────────────────────────┐
│              TRIDENT COMPILER                   │
│  trident verify token.tri                       │
│                                                 │
│  Result: VERIFIED or COUNTEREXAMPLE             │
└──────────┬──────────────────────┬───────────────┘
           │                      │
      VERIFIED               COUNTEREXAMPLE
           │                      │
           ▼                      ▼
┌──────────────────┐  ┌──────────────────────────┐
│  Deploy to any   │  │  Feed back to LLM:       │
│  zkVM target     │  │  "assert on line 14      │
│                  │  │   fails when amount=0,   │
│  Certificate     │  │   sender_balance=0"      │
│  attached        │  │                          │
└──────────────────┘  │  LLM regenerates code    │
                      │  with fix applied         │
                      └──────────┬───────────────┘
                                 │
                                 ▼
                          (back to compiler)
```

### 5.3 LLM Prompt Engineering for Trident

The LLM prompt includes:

1. **The complete Trident language reference** (~5K tokens — fits easily)
2. **The specification annotations reference** (~1K tokens)
3. **The standard library API** (~2K tokens)
4. **The user's natural language requirement**
5. **Previous verification failures** (if iterating)

Example system prompt fragment:

```
You are a Trident smart contract developer. Trident is a minimal language 
for provable computation with the following properties:

Types: Field (finite field element), Bool, U32, [T; N] (fixed arrays), 
       struct, Digest (hash output)

Control flow: if/else, for loops with MANDATORY bounds, assert()

I/O: pub_read() (public input), pub_write() (public output), 
     divine() (private witness from prover)

Crypto: hash(), sponge_init/absorb/squeeze(), merkle.verify()

RULES:
- No recursion, no heap, no strings, no unbounded loops
- Every loop must have a bound: for i in 0..n bounded MAX
- Use assert() to state properties that must ALWAYS hold
- Use #[requires()] for preconditions on function inputs
- Use #[ensures()] for postconditions on function outputs
- Use #[invariant()] for loop invariants
- The compiler WILL formally verify all assertions and specs

When you write code, think about:
1. What properties MUST be true? (write them as assertions)
2. What inputs are valid? (write them as preconditions)
3. What does the output guarantee? (write them as postconditions)
4. For divine() values: what constraints ensure a valid witness exists?
```

### 5.4 Iterative Refinement Example

**Human:** "Write a contract that verifies a confidential token transfer. The sender proves they have enough balance without revealing the actual balance. The total supply must be conserved."

**LLM attempt 1:**

```
module confidential_transfer

#[contract_invariant(total_supply_conserved())]

#[requires(amount > 0)]
#[ensures(sender_new + receiver_new == sender_old + receiver_old)]
pub fn transfer(
    sender_root: Digest,
    receiver_root: Digest,
    amount: Field,
) {
    // Sender proves their balance
    let sender_balance: Field = divine()
    let sender_path: [Digest; 32] = divine_path()
    
    // Verify sender's balance is in the state tree
    merkle.verify(sender_root, hash_balance(sender_balance), sender_index, 32)
    
    // Check sufficient balance
    assert(sender_balance >= amount)
    
    // Compute new balances
    let sender_new = sender_balance - amount
    let receiver_balance: Field = divine()
    let receiver_new = receiver_balance + amount
    
    // Output new state commitments
    pub_write(hash_balance(sender_new))
    pub_write(hash_balance(receiver_new))
}
```

**Compiler feedback:**

```
error[V002]: postcondition cannot be proven
  --> transfer.tri:7
   |
7  | #[ensures(sender_new + receiver_new == sender_old + receiver_old)]
   |
   = note: `sender_old` and `receiver_old` are not in scope at function return
   = help: postconditions can only reference function parameters and return values

error[V003]: undefined variable in specification
  --> transfer.tri:1
   |
1  | #[contract_invariant(total_supply_conserved())]
   |                      ^^^^^^^^^^^^^^^^^^^^^^^^ function not defined
```

**LLM attempt 2** (after receiving compiler feedback):

```
module confidential_transfer

struct TransferProof {
    sender_new_hash: Digest,
    receiver_new_hash: Digest,
}

#[requires(amount > 0)]
#[ensures(
    divine_sender_balance - amount + divine_receiver_balance + amount
    == divine_sender_balance + divine_receiver_balance
)]
pub fn transfer(
    state_root: Digest,
    amount: Field,
    sender_index: U32,
    receiver_index: U32,
) -> TransferProof {
    // Divine witness: sender's current balance
    let sender_balance: Field = divine()
    assert(sender_balance >= amount)
    
    // Verify sender balance is committed in state tree
    merkle.verify(state_root, hash_balance(sender_balance), sender_index, 32)
    
    // Divine witness: receiver's current balance
    let receiver_balance: Field = divine()
    merkle.verify(state_root, hash_balance(receiver_balance), receiver_index, 32)
    
    // Compute new balances (conservation by construction)
    let sender_new = sender_balance - amount
    let receiver_new = receiver_balance + amount
    
    // Assert conservation explicitly
    assert(sender_new + receiver_new == sender_balance + receiver_balance)
    
    TransferProof {
        sender_new_hash: hash_balance(sender_new),
        receiver_new_hash: hash_balance(receiver_new),
    }
}
```

**Compiler result:**

```
$ trident verify confidential_transfer.tri

  transfer:
    ✓ requires(amount > 0)                                [precondition]
    ✓ assert(sender_balance >= amount)                     [depends on witness]
    ✓ merkle.verify (sender)                              [hash-opaque: valid]
    ✓ merkle.verify (receiver)                            [hash-opaque: valid]
    ✓ assert(sender_new + receiver_new == ...)            [algebraic: 0.1ms]
    ✓ ensures(conservation)                               [algebraic: 0.1ms]
    ✓ witness existence: sender_balance                   [SMT: 1.8s]
    ✓ witness existence: receiver_balance                 [SMT: 1.6s]

RESULT: 8/8 verified
  Time: 3.7s
  Certificate: confidential_transfer.cert
```

Two iterations. Total wall-clock time including LLM generation: under 30 seconds. The resulting contract is **mathematically proven correct** for all possible inputs.

### 5.5 What the LLM Cannot Get Wrong

With Trident's verification loop, certain classes of bugs are **impossible to ship:**

| Bug class | How the verifier catches it |
|-----------|---------------------------|
| Arithmetic overflow | Counterexample with values near field boundary |
| Missing balance check | Counterexample: sender_balance < amount |
| Conservation violation | Algebraic proof fails: sum(outputs) ≠ sum(inputs) |
| Invalid Merkle proof accepted | Hash constraint violation |
| Witness non-existence | ∃-solver finds input with no valid witness |
| Dead assertion (always false) | Counterexample: ANY input triggers failure |
| Unreachable code after assert | Path analysis shows contradiction |
| Wrong loop bound | Invariant violation at boundary iteration |

The LLM can still write code that is correct but suboptimal (higher proving cost than necessary), or code that is correct but has overly restrictive preconditions (rejects valid inputs). These are quality issues, not safety issues. The verification guarantees that whatever the LLM produces, if it passes, it is correct.

---

## 6. Properties Decidable in Trident

### 6.1 Fully Decidable (Automatic, Guaranteed Termination)

These properties can ALWAYS be verified or refuted — the solver terminates with a definitive answer.

**Algebraic identities over F_p:**
- Conservation: `sum(outputs) == sum(inputs)`
- Commutativity: `f(a, b) == f(b, a)`
- Idempotence: `f(f(x)) == f(x)`
- Equivalence of two implementations: `f(x) == g(x)` for all x

**Range properties:**
- `x < 2^32` (U32 range)
- `output ∈ {0, 1}` (boolean)
- `index < array_length`

**Assertion reachability:**
- "Can this assert ever fail?" — SAT query over bounded program
- "Is this code reachable?" — path feasibility

**Witness existence (single `divine()`):**
- "For all valid public inputs, does there exist a witness satisfying all assertions?"
- Decidable because F_p is finite

**Invariant checking (bounded loops):**
- Base case + inductive step, both over finite field arithmetic
- For small bounds: complete unrolling as fallback

### 6.2 Practically Decidable (Automatic, May Timeout for Large Instances)

**Multiple interacting `divine()` values:**
- Existential quantification over multiple witness variables
- Decidable in theory, may be slow for >10 divine variables
- Mitigation: decompose into per-function verification

**Loops with large bounds (>1000):**
- Unrolling is impractical
- Invariant checking works if invariant is provided
- Without invariant: attempt automatic invariant synthesis (incomplete)

**Hash-dependent properties:**
- Hash functions are modeled as uninterpreted functions (opaque)
- Properties like "if hash(a) == hash(b) then a == b" assumed (collision resistance)
- Properties requiring reasoning ABOUT the hash internals: not automatic

### 6.3 Not Decidable (Require Human Insight)

**Cross-module composition with unbounded participants:**
- "No matter how many users interact with the contract, the invariant holds"
- Requires induction over number of participants — needs human-provided induction hypothesis

**Temporal properties:**
- "Eventually, the funds are released" — Trident has no temporal operators
- Not applicable: Trident programs are single-execution, not reactive systems

**Information-theoretic privacy:**
- "The output reveals nothing about the witness beyond what the spec allows"
- This is the zero-knowledge property itself — proven at the proof system level, not the program level

---

## 7. Implementation Plan

### 7.1 Phase 1: Assertion Analysis Engine (4-6 weeks)

**Goal:** The compiler proves or refutes inline `assert()` statements with zero user annotations.

**Deliverables:**
- Symbolic execution engine (`sym.rs`): converts AST to symbolic constraint system
- Algebraic solver: proves polynomial identities over F_p directly (no external solver)
- Bounded model checker: unrolls loops up to bound 64, checks all paths
- `trident verify` CLI command
- Verification report with counterexamples

**What it proves at this phase:**
- Redundant assertions (proven true — can be removed to save proving cost)
- Contradictory assertions (always false — program is buggy)
- Implied assertions (A implies B — B is redundant given A)
- Simple arithmetic invariants (conservation, range, equality)

**Example output:**

```
$ trident verify main.tri

  main:
    ✓ assert(amount <= balance)      — cannot verify (depends on input)
    ✓ assert(new_balance >= 0)       — VERIFIED: implied by line 5 + field semantics
                                       → can be removed (saves 2 clock cycles)
    ✓ assert(a + b == b + a)         — VERIFIED: field commutativity (0.01ms)
    
  RESULT: 2/3 verified, 1 input-dependent
  Potential proving cost savings: 2 cycles (from removing redundant assertions)
```

**Key implementation detail:** The algebraic solver doesn't need Z3 for this phase. Field polynomial identity testing can be done with Schwartz-Zippel: evaluate at random points. If `f(x) - g(x) == 0` at enough random points, the polynomials are identical. Fast, simple, and sufficient for most algebraic properties.

### 7.2 Phase 2: Specification Annotations + SMT Integration (6-8 weeks)

**Goal:** Add `#[requires]`, `#[ensures]`, `#[invariant]` annotations and connect to Z3 for non-algebraic properties.

**Deliverables:**
- Specification annotation parser and AST integration
- Z3 backend: encode Trident constraints as SMT-LIB queries
- Field arithmetic theory for Z3 (Goldilocks-specific bit-vector encoding)
- Witness existence checking for `divine()` values
- Loop invariant verification (Hoare logic framework)
- Counterexample generation with human-readable output
- `--certificate` flag for generating verification certificates

**What it proves at this phase:**
- All Phase 1 properties, plus:
- Precondition/postcondition contracts
- Loop invariants (when annotated)
- Witness existence: "for all valid inputs, there exists a valid divine() value"
- Inter-assertion implications across function boundaries
- Contract invariants across multiple entry points

### 7.3 Phase 3: LLM Integration Framework (4-6 weeks)

**Goal:** Build the tooling that enables LLM-driven verified contract generation.

**Deliverables:**
- Machine-readable verification output (JSON format for LLM consumption)
- Structured error messages with fix suggestions
- `trident generate` command: accepts natural language spec, invokes LLM, iterates until verified
- Trident language reference in LLM-optimized format (single-file, example-heavy)
- Prompt templates for contract generation with verification
- Benchmark suite: 20 contract specifications with known-correct implementations

**`trident generate` workflow:**

```bash
# Interactive mode: human describes, LLM generates, compiler verifies
trident generate --interactive

# Spec file mode: structured specification → verified contract
trident generate --spec transfer_spec.md --output transfer.tri

# Batch mode: generate and verify, report success/failure
trident generate --spec specs/ --output contracts/ --max-iterations 5
```

**Machine-readable verification output for LLM feedback:**

```json
{
  "status": "failed",
  "failures": [
    {
      "location": "transfer.tri:14:5",
      "assertion": "assert(sender_balance >= amount)",
      "type": "input_dependent",
      "counterexample": {
        "sender_balance": "0",
        "amount": "100"
      },
      "suggestion": "Add #[requires(sender_balance >= amount)] or handle the case where sender_balance < amount"
    }
  ],
  "verified": [
    {
      "location": "transfer.tri:18:5",
      "assertion": "assert(conservation)",
      "method": "algebraic",
      "time_ms": 0.1
    }
  ]
}
```

The LLM reads this JSON, understands exactly what failed and why, and generates a fix. The structured counterexample is far more useful to an LLM than a stack trace or test failure.

### 7.4 Phase 4: Automatic Invariant Synthesis (3-4 months, research)

**Goal:** The compiler infers loop invariants and specifications automatically, reducing the need for human annotations.

**Deliverables:**
- Invariant synthesis engine: generates candidate invariants from code patterns
- Template-based synthesis: common patterns (summation, accumulation, comparison)
- Counterexample-guided refinement (CEGIS): use counterexamples to refine candidates
- Specification inference: suggest `#[ensures]` postconditions from code analysis

**Example:** The compiler analyzes a loop and automatically discovers the invariant:

```
// No annotation needed — compiler infers the invariant
pub fn sum_array(arr: [Field; 10]) -> Field {
    let mut total: Field = 0
    for i in 0..10 {
        total = total + arr[i]
    }
    total
}

// Compiler infers: invariant(total == sum(arr[0..i]))
// Compiler infers: ensures(result == sum(arr[0..10]))
```

This reduces the annotation burden on the LLM — it doesn't need to generate invariants, only the code and basic preconditions/postconditions.

### 7.5 Phase 5: Cross-Target Verification (2-3 months)

**Goal:** Prove that verified properties hold across all compilation targets.

**Deliverables:**
- Verification is target-independent (operates on AST, before emission)
- Proof that AST-level verification implies target-level correctness (for each backend)
- Cross-target equivalence checking: same program on Triton and Miden produces same outputs
- Verification certificate includes target-independence proof

The key insight: verification operates on the type-checked AST, which is shared across all backends. If a property is proven at the AST level, it holds for every backend that correctly implements the abstraction layer. This means one verification covers all deployments.

### 7.6 Timeline

```
Month 1-2:     Phase 1 — Assertion analysis engine
Month 2-4:     Phase 2 — Specifications + SMT integration
Month 4-6:     Phase 3 — LLM integration framework
Month 6-10:    Phase 4 — Automatic invariant synthesis (research)
Month 8-10:    Phase 5 — Cross-target verification
```

---

## 8. The LLM-Verifier Flywheel

### 8.1 Why This Gets Better Over Time

The LLM-verifier loop creates a self-reinforcing improvement cycle:

```
Better LLM training data (verified contracts)
    → LLM generates better first attempts
        → Fewer verification iterations needed
            → More contracts verified per unit time
                → More training data generated
                    → Better LLM training data
```

Every verified contract becomes training data for the next generation of the LLM. Unlike traditional code generation where "correct" is ambiguous, Trident's verifier provides a binary signal: verified or not. This is perfect for reinforcement learning — the verification result is the reward.

### 8.2 Contract Specification Library

Over time, build a library of verified specification patterns:

```
// Standard patterns (verified once, reused forever)

pattern ConservationLaw<T> {
    #[ensures(sum(outputs) == sum(inputs))]
}

pattern MerkleInclusion {
    #[requires(depth <= 64)]
    #[ensures(leaf_is_in_tree(root, leaf, index))]
}

pattern BalanceTransfer {
    #[requires(sender_balance >= amount)]
    #[requires(amount > 0)]
    #[ensures(sender_new == sender_balance - amount)]
    #[ensures(receiver_new == receiver_balance + amount)]
    #[contract_invariant(total_supply_unchanged())]
}

pattern AuthenticatedRead {
    #[requires(valid_merkle_proof(root, path, index))]
    #[ensures(result == committed_value_at(index))]
}
```

The LLM references these patterns when generating code. "This looks like a balance transfer" → apply the `BalanceTransfer` pattern → generate code that satisfies it → verify.

### 8.3 Verification-Driven Development

The traditional development cycle:

```
Write code → Write tests → Run tests → Debug → Repeat
```

Trident's verification-driven cycle:

```
Write spec → Generate code (LLM) → Verify (compiler) → Deploy
```

No tests. No debugging. No test coverage anxiety. The verifier checks ALL inputs, not a finite sample. If it says "VERIFIED," the contract is correct by mathematical proof.

The human's job shifts from "writing code and tests" to "writing specifications" — describing WHAT should be true, not HOW to implement it. This is a higher-level, more natural activity for humans, and one where LLMs can provide significant assistance.

---

## 9. Comparison: Trident Verified vs. Existing Approaches

| Dimension | Solidity + Audit | Cairo + Formal Methods | Coq/Lean Extraction | **Trident Verified** |
|-----------|-----------------|----------------------|--------------------|--------------------|
| **Who writes specs** | Auditor (informal) | Developer | Expert | Human or LLM |
| **Who writes proofs** | Nobody (audit only) | Expert | PhD-level expert | **Nobody — compiler** |
| **Verification scope** | Sampled (audit) | Partial (selected properties) | Complete (proven properties) | **Complete (all assertions)** |
| **Time to verify** | 2-6 weeks | Days to weeks | Weeks to months | **Seconds** |
| **Cost** | $50K-$500K | $10K-$100K | $100K+ | **Free (compiler feature)** |
| **LLM-compatible** | Partially (LLM can write Solidity, but can't guarantee correctness) | No | No (tactic proofs too brittle) | **Yes (verification loop)** |
| **False sense of security** | High (audit ≠ proof) | Medium | Low | **None (proof or counterexample)** |
| **Result** | "We looked at it" | "These properties hold" | "This theorem is proven" | **"ALL assertions hold for ALL inputs"** |
| **Expressiveness** | High (general-purpose) | Medium | Very high | Low (but sufficient for ZK contracts) |
| **Deployment targets** | EVM only | Cairo VM only | Extracted to one language | **Any zkVM** |

### 9.1 Why Not Just Use Coq/Lean?

Coq and Lean are general-purpose proof assistants. They can prove anything — but:

1. **Someone must write the proof.** Proofs in Coq are programs in a tactic language. They require deep expertise and are brittle to changes.
2. **Proofs don't compose well.** Changing one function may invalidate proofs of other functions.
3. **The extraction gap.** Code proven in Coq is extracted to OCaml/Haskell, then compiled to the target. The extraction and compilation are NOT verified — bugs can be introduced after the proof.
4. **LLMs can't write tactic proofs reliably.** The tactic language is too fragile, too context-dependent, and too different from natural reasoning.

Trident eliminates all four problems:
1. No one writes proofs — the solver finds them automatically.
2. Verification is modular — each function is verified independently.
3. There's no extraction gap — the verified AST IS what gets compiled.
4. LLMs write code, not proofs — and the compiler verifies the code.

---

## 10. Risk Analysis

| Risk | Likelihood | Impact | Mitigation |
|------|:----------:|:------:|------------|
| SMT solver too slow for complex contracts | Medium | High | Layered solver strategy; algebraic pass catches most properties; timeout with clear reporting |
| LLMs generate specs that are too weak (true but useless) | Medium | Medium | Pattern library guides LLM to standard specs; human reviews specs before deployment |
| LLMs generate specs that are too strong (no code can satisfy) | Low | Low | Compiler reports "unsatisfiable spec" — LLM weakens preconditions |
| Users trust verification for properties not actually checked | Medium | High | Clear reporting of what IS and ISN'T verified; certificate enumerates proven properties |
| Field arithmetic encoding is incorrect in SMT backend | Low | Critical | Extensive testing against concrete execution; cross-check algebraic and SMT solvers |
| Automatic invariant synthesis fails on real-world contracts | High | Medium | Fall back to user-annotated invariants; LLM suggests invariants for human review |
| Verification only covers functional correctness, not ZK privacy | Medium | High | Document clearly: verification proves CORRECTNESS, not PRIVACY; privacy is a proof system property |

---

## 11. Success Criteria

### Phase 1 (Assertion Analysis)
- [ ] `trident verify` processes all existing test programs
- [ ] Identifies all redundant assertions in the standard library
- [ ] Reports counterexamples for intentionally buggy test programs
- [ ] Verification time < 5 seconds for programs under 200 lines

### Phase 2 (Specs + SMT)
- [ ] Verifies a Merkle proof implementation with annotated invariants
- [ ] Verifies a token transfer with conservation law
- [ ] Proves witness existence for `divine()` in standard patterns
- [ ] Counterexample quality: LLM can fix the bug from the counterexample alone

### Phase 3 (LLM Integration)
- [ ] LLM generates verified contract from natural language in ≤ 3 iterations (median)
- [ ] 80%+ of benchmark specifications result in verified contracts
- [ ] Machine-readable output enables fully automated generate-verify loop
- [ ] Time from spec to verified contract < 2 minutes (median)

### Overall
- [ ] A developer with no formal methods background can verify their contracts
- [ ] An LLM can produce formally verified contracts for standard ZK patterns
- [ ] Verification adds < 5% to compilation time for `--quick` mode
- [ ] The verification system catches at least 95% of bugs that manual audits catch
- [ ] Zero false positives: if the compiler says "VERIFIED," the property holds

---

## 12. The End State

When this system is complete, deploying a formally verified ZK contract looks like this:

```
$ trident generate --spec "verify that a token transfer preserves 
  total supply, sender has sufficient balance, and the Merkle proof 
  of the sender's balance is valid against the state root"

Generating contract... (LLM: attempt 1)
Verifying... 
  ✗ witness existence for sender_balance (no precondition on state_root)

Regenerating... (LLM: attempt 2, with fix)
Verifying...
  ✓ 12/12 properties verified (4.2s)

Contract saved: confidential_transfer.tri
Certificate saved: confidential_transfer.cert

$ trident build confidential_transfer.tri --target triton
  Output: confidential_transfer.tasm (verified, certificate attached)

$ trident build confidential_transfer.tri --target miden
  Output: confidential_transfer.masm (verified, certificate attached)

$ trident build confidential_transfer.tri --target openvm
  Output: confidential_transfer.elf (verified, certificate attached)
```

30 seconds from English to formally verified, multi-target-deployed smart contract.

No audit. No PhD. No Coq. No bugs.

The language's simplicity isn't a limitation — it's what makes everything else possible.

---

*Trident Verified — When the language is simple enough, correctness is free.*


This is a deep question. Let me think through it properly.

Trident is almost uniquely suited for embedded formal verification because of what it *doesn't* have. No heap, no recursion, no dynamic dispatch, no closures, no unbounded loops, no side effects beyond I/O. Every program is total (always terminates), first-order (no higher-order functions), and has statically-known resource usage. This is essentially the fragment of computation that proof assistants handle best.

The question is: embed formal verification *in* Trident (the language itself carries proofs), or embed it *in the compiler pipeline* (the tooling verifies programs automatically)?

---

## Three Levels of Formal Verification

### Level 1: Compiler-Embedded Verification (Most Practical)

The compiler already does type checking and cost analysis. Extend this to prove properties automatically without any user annotation.

**What Trident can verify today with no language changes:**

- Bounds checking: array indices within `[0, N)` — the compiler knows `N` and tracks loop variables
- Integer overflow: `U32` operations stay in `[0, 2³²)` — the compiler already inserts range checks
- Division by zero: for `a /% b`, prove `b ≠ 0` statically when possible
- Loop termination: already guaranteed by bounded loops
- Stack safety: already tracked by `stack.rs`
- Dead code: already detected

**What could be added with moderate effort:**

```
// Assertion propagation — the compiler proves assertions statically when possible
fn transfer(amount: U32, balance: U32) {
    assert(amount < balance)          // runtime check
    let new_balance = sub(balance, amount)
    assert(new_balance >= 0)          // compiler: "this is implied by the previous assert + U32 semantics"
                                      // → optimized away, no runtime cost
}
```

The compiler builds a simple constraint system from the program's assertions and checks whether later assertions are implied by earlier ones. For bounded programs with field arithmetic, this is decidable — you're essentially solving polynomial equations over a finite field, which is what the VM itself does.

**Implementation:** Add an SMT solver pass between type checking and emission. Use an off-the-shelf solver (Z3, CVC5) or a lightweight custom solver for the field-arithmetic fragment. The solver verifies assertions at compile time; assertions proven true are either removed (saving proving cost) or annotated as "verified."

```bash
trident build main.tri --verify       # Run SMT verification pass
trident build main.tri --verify=full  # Attempt to prove all assertions statically
```

### Level 2: Specification Annotations (Medium Effort)

Add lightweight specification annotations to Trident that the compiler verifies.

```
/// Verify that output equals the square root of input.
/// Pre: x > 0
/// Post: result * result == x
#[requires(x > 0)]
#[ensures(result * result == x)]
pub fn sqrt_verify(x: Field) -> Field {
    let s: Field = divine()      // prover supplies sqrt
    assert(s * s == x)           // runtime check
    s
}
```

The compiler verifies that the `#[ensures]` postcondition is logically implied by the function body + `#[requires]` precondition. For this example, it's trivial: the `assert` guarantees `s * s == x`, and `s` is returned as `result`, so `result * result == x` is proven.

**More interesting example — loop invariants:**

```
#[ensures(result == n * (n + 1) / 2)]
pub fn sum_to(n: U32) -> Field {
    let mut total: Field = 0
    #[invariant(total == i * (i - 1) / 2)]
    for i in 0..n bounded 1000 {
        total = total + as_field(i)
    }
    total
}
```

The compiler verifies:
1. The invariant holds at loop entry (i=0: total=0 = 0*(-1)/2 ✓)
2. If the invariant holds at iteration i, it holds at iteration i+1
3. The invariant at loop exit (i=n) implies the postcondition

This is standard Hoare logic, applied to a language where it's actually tractable because everything is bounded and first-order.

**Specification language design:**

```
#[requires(condition)]           // Precondition
#[ensures(condition)]            // Postcondition  
#[invariant(condition)]          // Loop invariant
#[decreases(expression)]         // Not needed — loops are always bounded
#[preserves(condition)]          // Maintained across function call
```

These annotations are checked at compile time and have zero runtime cost. They don't appear in the TASM output. They're proofs about the program, not part of the program.

**Why this works better in Trident than in general languages:**

- No heap → no aliasing problems, no frame problem
- No recursion → no inductive proofs over recursive structure needed
- Bounded loops → invariant checking is decidable (finite unrolling as fallback)
- Field arithmetic → polynomial equations over finite fields are decidable
- First-order → no higher-order reasoning needed
- Total functions → no termination proofs needed

### Level 3: Coq/Lean Extraction and Deep Embedding (Most Powerful)

This is the nuclear option: represent Trident programs inside a proof assistant and prove arbitrary properties.

**Two approaches:**

**Approach A: Shallow Embedding (Trident → Coq/Lean functions)**

Translate each Trident program into an equivalent Coq/Lean function. The proof assistant's type checker verifies properties.

```coq
(* Generated from Trident source *)
Definition sqrt_verify (x : F_p) : F_p :=
  let s := divine () in
  assert (s * s = x);
  s.

(* User writes proofs about the generated definition *)
Theorem sqrt_verify_correct : forall x s,
  s * s = x -> sqrt_verify x = s.
Proof. intros. unfold sqrt_verify. auto. Qed.
```

**Feasibility:** High. Trident's feature set maps almost 1:1 to Coq's:

| Trident | Coq/Lean |
|---------|----------|
| `Field` | `Z_p` (integers mod p) or custom field type |
| `U32` | `Fin (2^32)` or bounded nat |
| `Bool` | `bool` |
| `[T; N]` | `Vector T N` |
| `struct` | Record type |
| `fn` | Definition / Function |
| `for i in 0..N { body }` | `Nat.fold N body` or `Vector.map` |
| `if/else` | `if/then/else` |
| `assert(p)` | Proposition as precondition |
| `divine()` | Existential quantifier |

The translation is mechanical. A `trident coq main.tri` command generates `.v` files.

**Approach B: Deep Embedding (Trident AST in Coq/Lean)**

Represent the Trident AST as a Coq datatype and reason about program transformations:

```coq
(* Trident AST as Coq inductive type *)
Inductive Expr :=
  | Lit : nat -> Expr
  | Var : string -> Expr
  | Add : Expr -> Expr -> Expr
  | Mul : Expr -> Expr -> Expr
  | FieldInv : Expr -> Expr.

Inductive Stmt :=
  | Let : string -> Expr -> Stmt
  | Assert : Expr -> Stmt
  | For : string -> nat -> list Stmt -> Stmt
  | If : Expr -> list Stmt -> list Stmt -> Stmt.

(* Now prove properties about the COMPILER *)
Theorem emit_add_correct : forall e1 e2 env,
  eval env (Add e1 e2) = (eval env e1 + eval env e2) mod p.

Theorem triton_emission_preserves_semantics : forall prog input,
  trident_eval prog input = triton_eval (compile prog) input.
```

This lets you prove that the *compiler itself* is correct — that compilation preserves program semantics. This is the gold standard (CompCert for C, CakeML for ML).

**Feasibility:** Hard but tractable for Trident specifically because:
- The AST has ~15 node types (vs hundreds for C)
- The compilation is direct (no optimization passes to verify)
- The target VM has ~45 instructions (vs hundreds for x86)
- Everything is bounded and first-order

A verified Trident compiler would be roughly the size of a graduate thesis project, not a multi-year research effort like CompCert.

---

## The `divine()` Problem

The most interesting formal verification challenge in Trident is `divine()` — non-deterministic input from the prover. In a proof assistant, this becomes an existential quantifier:

```
// Trident
let s: Field = divine()
assert(s * s == x)

// Coq equivalent
exists s : F_p, s * s = x
```

This means proving a Trident program correct requires proving that the existential is satisfiable — that the prover *can* supply a valid witness. This is exactly the soundness property of the ZK program.

For the `divine()` + `assert()` pattern, the formal verification question is: "for all valid public inputs, does there exist a private witness that satisfies all assertions?" This is the completeness of the ZK proof system, and it's exactly what you want to verify formally.

**Example where this catches real bugs:**

```
// BUG: not all field elements have square roots in a finite field
fn sqrt_verify(x: Field) -> Field {
    let s: Field = divine()
    assert(s * s == x)    // What if x is a quadratic non-residue?
    s
}
```

A formal verification tool would flag: "Cannot prove existence of witness `s` for all inputs `x`. Counterexample: x = [specific quadratic non-residue mod p]." The fix requires a precondition: `#[requires(is_quadratic_residue(x))]`.

---

## Practical Architecture for Embedded Verification

Here's what I'd actually build, in order:

### Phase 1: Assertion Analysis (2-3 weeks)

Add a compiler pass that analyzes assertions for redundancy and implication. No user-facing specification language — just compiler intelligence.

```bash
trident build main.tri --verify
```

Output:
```
Verification report:
  main.tri:12  assert(amount < balance)     — cannot verify statically (runtime check)
  main.tri:14  assert(new_balance >= 0)     — VERIFIED: implied by line 12 + u32 semantics
                                              → removed (saves 2 clock cycles)
  main.tri:23  assert(index < 32)           — VERIFIED: loop bound guarantees index ∈ [0, 32)
                                              → removed (saves 2 clock cycles)
  
  2 assertions verified and removed. Proving cost reduced by 4 cc.
```

This is immediately useful: it reduces proving cost by eliminating redundant assertions, and it catches bugs where assertions contradict each other (unsatisfiable program).

### Phase 2: Specification Annotations (4-6 weeks)

Add `#[requires]`, `#[ensures]`, `#[invariant]` annotations. Use Z3 or a lightweight SMT solver for the field-arithmetic fragment.

```
#[requires(depth <= 64)]
#[ensures(result == true)]  // if we reach here, Merkle proof is valid
pub fn verify_merkle(root: Digest, leaf: Digest, index: U32, depth: U32) -> Bool {
    // ...
}
```

The solver checks specifications at compile time. Failed verification produces a counterexample or an "unable to verify" warning.

### Phase 3: Coq/Lean Extraction (2-3 months)

Add `trident extract --coq main.tri` and `trident extract --lean main.tri`. Generate proof assistant files from Trident source.

**The extraction includes:**
- Type definitions as Coq records / Lean structures
- Functions as Coq definitions / Lean defs
- Assertions as proof obligations (theorems to prove)
- `divine()` as existential quantifiers
- Loop invariants (if annotated) as inductive hypotheses

**The user workflow:**
1. Write Trident program
2. Extract to Coq/Lean
3. Write proofs about the extracted program in the proof assistant
4. Compile to zkVM with confidence that the proven properties hold

### Phase 4: Verified Compiler (6-12 months, research)

Prove in Coq/Lean that the Trident compiler preserves semantics. This is the "CompCert for ZK" goal.

**Scope it to be tractable:**
- Verify only the universal core (not backend extensions)
- Verify only one backend first (Triton — the simplest, direct emission)
- Use the deep embedding approach
- The proof covers: "for all well-typed Trident programs, the TASM output computes the same function"

---

## How This Relates to the Multi-Target Story

Formal verification and multi-target compilation are deeply synergistic:

1. **Cross-target equivalence proofs**: Formally prove that the Triton and Miden backends produce semantically equivalent output for any universal-core program. This is stronger than testing — it's a mathematical guarantee.

2. **Specification as the canonical semantics**: The `#[requires]`/`#[ensures]` annotations define what the program *means*, independent of any backend. The backends are correct if they satisfy the specification.

3. **Extension verification**: Backend extensions can carry their own specifications. `ext.triton.xfield` can specify the mathematical properties of extension field arithmetic. The compiler verifies that the extension's implementation satisfies its spec.

4. **Proof portability**: A proof about a Trident program (written in Coq/Lean) applies to all backend compilations. You prove once, deploy everywhere — and the correctness proof travels with the code.

---

## The Unique Opportunity

No other ZK language has this combination:
- Small enough to formally verify the entire compiler (~12K lines)
- Simple enough that program verification is decidable for most cases
- Multi-target, so formal verification results apply across all deployments
- Already has the bounded/total/first-order properties that proof assistants need

Cairo is too complex (Sierra IR, type-level generics, heap allocation). Noir has a richer type system that complicates verification. Circom is too low-level. Solidity/Vyper don't target ZK.

Trident could become the first language where **every program ships with a machine-checked correctness proof**, because the language was designed (perhaps accidentally) to make this tractable.
