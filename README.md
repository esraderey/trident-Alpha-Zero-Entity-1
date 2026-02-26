# Trident

> The weapon is their language. They gave it all to us. If you learn it, when you really learn it, you begin to perceive time the way that they do. So you can see what's to come

<p align="center">
  <img src="media/tri.gif" width="100%" alt="Trident" />
</p>

Trident is a provable programming language. Its power lies in collective, private, intelligent,
quantum-native computation. Every variable, every operation, every function compiles to
arithmetic over the Goldilocks prime field (p = 2^64 - 2^32 + 1). Programs
produce STARK proofs — hash-based, post-quantum secure, no trusted setup.

## One Field. Three Revolutions.

Three computational revolutions — quantum computing, privacy, and
artificial intelligence — are advancing in isolation. They share a
common algebraic root: the prime field.

**Quantum** — prime-dimensional Hilbert spaces have no invariant
subspaces. Every gate touches the full state space. A single
prime-dimensional qudit replaces 64 entangled qubits — four orders of
magnitude gate count reduction.

**Privacy** — zero-knowledge proofs (STARKs), fully homomorphic
encryption (TFHE), and multi-party computation (Shamir sharing) all
demand a field where every nonzero element has a multiplicative inverse
and no information is destroyed. All three operate natively over the
same prime field.

**AI** — neural networks expressed in field arithmetic produce STARK
proofs alongside their outputs. Weights, activations, and gradients are
field elements from the start — no float-to-field quantization.

> Reversible computation with complete arithmetic lives in prime fields.
> Both classical provability and quantum mechanics require reversible
> computation with complete arithmetic. Therefore both require prime
> fields. The convergence is
> [structural](docs/explanation/quantum.md).

### The Rosetta Stone

A single lookup table over the Goldilocks field simultaneously
functions as four mechanisms:

| Reading | Role | What it provides |
|---------|------|------------------|
| Cryptographic S-box | Hash nonlinearity | Security |
| Neural activation | Network expressiveness | Intelligence |
| FHE bootstrap | Encrypted evaluation | Privacy |
| STARK lookup | Proof authentication | Verifiability |

One table. One field. Four purposes. When all systems operate over the
same prime field, four separate mechanisms collapse into one data
structure read four ways.

### Trinity: All Three in One Proof

[Trinity](docs/explanation/trinity-bench.md) demonstrates the
unification: a single Trident program encrypts input with LWE, runs a
dense neural layer, decrypts, hashes with a LUT sponge, applies
Poseidon2, performs programmable bootstrapping, and commits via a
quantum circuit — all inside one STARK trace.

```
Encrypted Input → FHE Linear → Decrypt → Dense Layer (Reader 1)
→ LUT Sponge Hash (Reader 2) → Poseidon2 → PBS Demo (Reader 3) → Quantum Commit
```

All four readers share the same RAM-based ReLU lookup table. To our
knowledge, no existing system composes FHE, neural inference,
LUT-based hashing, and quantum circuits in a single proof.

See [Quantum](docs/explanation/quantum.md),
[Privacy](docs/explanation/privacy.md), and
[Verifiable AI](docs/explanation/ai.md) for the full treatment of each
pillar.

---

## Write Once, Prove Anywhere.

Provable VMs need a language designed for how they work. You write
Trident once; it compiles to any provable target.

```trident
program hello

fn main() {
    let a: Field = pub_read()
    let b: Field = pub_read()
    pub_write(a + b)
}
```

```nu
trident build hello.tri           # compile to TASM (Triton VM)
```

Feed it to the prover and you get a
[STARK proof](docs/explanation/stark-proofs.md) that `a + b = sum` for
secret values of `a` and `b`. Quantum-safe. Zero-knowledge. No trusted
setup. No elliptic curves.

Four structural facts drive every design decision:

**Arithmetic circuits are not programs.** The machine word is a field
element, not a byte. A language that treats `Field`, `Digest`, and
extension fields as first-class types generates native circuits. One
that wraps byte-oriented code in ZK proofs fights the machine at every
step.

**Proofs compose, calls don't.** Programs produce independent proofs
that a verifier checks together. Composition is recursive — a proof
can verify another proof inside it, so any chain of proofs collapses
into a single proof. Trident is designed for recursive proof
composition — not invocation.

**Bounded execution is a feature.** Circuits must terminate. Loops must
be bounded. The compiler computes exact proving cost from source,
before execution. The same bound that makes programs provable makes
them quantum-native: bounded loops map directly to fixed-depth quantum
circuits.

**The field is the type system.** Goldilocks prime, cubic extension
fields, 5-element digests — these are the native machine words. The
same algebraic structure required for STARK proofs is optimal for
[quantum computation](docs/explanation/quantum.md),
[private computation](docs/explanation/vision.md), and neural network
inference. One design choice, three futures.

Today Trident compiles to [Triton VM](https://triton-vm.org/) — the
first target — powering [Neptune Cash](https://neptune.cash/), the
only programmable, private, mineable, quantum-safe blockchain. The
[multi-target architecture](docs/explanation/multi-target.md) supports
quantum, ML, ZK, and classical backends as they ship.

### What follows

Source compiles through a 54-operation
[intermediate representation](reference/ir.md) that maps nearly 1:1 to
target instructions. What you see is what you prove.

Triton VM executes [Tip5](https://eprint.iacr.org/2023/107) in 1 clock
cycle. SP1 needs ~3,000 cycles for SHA-256. RISC Zero needs ~1,000.
For hash-heavy applications — Merkle trees, content addressing, token
transfers — this is decisive.
See [Comparative Analysis](docs/explanation/provable-computing.md).

Annotate with `#[requires]` and `#[ensures]`, run `trident audit`, get
a proof of correctness for all inputs — or a concrete counterexample.
See [Formal Verification](docs/explanation/formal-verification.md).

Every function has a unique cryptographic identity derived from its
normalized AST. Audit certificates travel with the code. See
[Content-Addressed Code](docs/explanation/content-addressing.md).

---

## Trusting, Not Trust.

You download a compiler binary. Someone compiled it — you trust them.
They used a compiler too — you trust that one as well. The trust chain
stretches back to the first hand-assembled binary, and every link is
opaque. Ken Thompson showed in 1984 that a compiler can inject
backdoors invisible in the source. Forty years later, every software
supply chain still rests on the same blind faith.

Trident breaks the chain. The compiler self-hosts on the [cyber/core](https://cyber.page/cyber-core/):
Trident source compiles Trident source, and the execution produces a
STARK proof that the compilation was faithful. Not "we audited the
binary." Not "we reproduced the build." A cryptographic proof, from
the mathematics itself, that the output corresponds to the input.

Seven compiler stages — lexer, parser, typechecker, codegen, optimizer,
lowering, pipeline — are already written in Trident. 9,195 lines of
self-hosted compiler:

```nu
trident bench baselines/triton/std/compiler  # instruction count scoreboard
```

```
Module                       Tri   Hand Neural   Ratio
-------------------------------------------------------
std::compiler::lexer         288      8      -  36.00x
std::compiler::parser        358      8      -  44.75x
std::compiler::pipeline        0      1      -   0.00x
```

The ratios are the optimization target — hand baselines set the floor,
the compiler races toward it. `trident bench --full` adds execution,
proving, and verification via STARK proof.

`src/` is the Rust bootstrap — it shrinks. `std/compiler/` is the
self-hosted replacement — it grows. When the last compiler stage moves
to Trident, every `trident build` produces a proof certificate
alongside the assembly. No trusted compiler. No trusted build server.
No trusted anything. You verify.

---

## Apps

Production programs that compile to TASM with `trident build` today.

[Coin](os/neptune/standards/coin.tri) — Fungible token (TSP-1).
Five operations (Pay, Lock, Update, Mint, Burn), time-locks, nullifiers,
configurable authorities, composable hooks.

[Card](os/neptune/standards/card.tri) — Non-fungible token (TSP-2).
Per-asset metadata, royalties, creator immutability, flag-gated
operations. Same PLUMB framework as Coin.

[Lock scripts](os/neptune/locks/) — Generation, symmetric, timelock,
multisig spending authorization.

[Type scripts](os/neptune/types/) — Native currency and custom token
conservation laws.

[Programs](os/neptune/programs/) — Transaction validation, recursive
verification, proof aggregation and relay.

See the [Gold Standard](docs/explanation/gold-standard.md) for the full
PLUMB specification and the [Skill Library](docs/explanation/skill-library.md)
for designed token capabilities.

---

## Quick Start

```nu
cargo build --release            # build from source
trident build main.tri           # compile to TASM
trident check main.tri           # type-check without emitting
trident fmt main.tri             # format source
trident test main.tri            # run #[test] functions
trident audit main.tri           # formal verification
trident package main.tri         # produce .deploy/ artifact
trident deploy main.tri          # package + deploy to registry
```

---

## Source Tree

```text
src/          Compiler in Rust            ~36K lines, 5 runtime dependencies
vm/           VM intrinsics in Trident    Compiler primitives (hash, I/O, field ops)
std/          Standard library in Trident Crypto, math, neural networks, compiler components
os/           OS bindings in Trident      Per-OS config, programs, and extensions
```

The four-tier namespace:

```
vm.*              Compiler intrinsics       TIR ops (hash, sponge, pub_read, assert)
std.*             Real libraries            Implemented in Trident (sha256, bigint, ecdsa)
os.*              Portable runtime          os.signal, os.neuron, os.state, os.time
os.<os>.*         OS-specific APIs          os.neptune.xfield, os.solana.pda
```

---

## Standard Library Vision

The `std.*` architecture reflects the three-pillar thesis:

**Foundation** — `std.field` (Goldilocks arithmetic, NTT, extensions),
`std.crypto` (Poseidon2, Tip5, signatures, FRI), `std.math` (exact field
arithmetic, linear algebra), `std.data` (Merkle trees, tensors,
authenticated structures), `std.io` (witness injection, storage),
`std.compiler` (self-hosted compiler components: lexer, parser, codegen).

**Three Pillars** — `std.quantum` (state management, gates, Grover's, QFT,
VQE, error correction), `std.private` (ZK + FHE + MPC: credentials,
auctions, voting, compliance-compatible privacy), `std.nn` (field-native
neural networks: matrix multiply, attention, convolutions, lookup-table
activations).

**Intersections** — `std.nn_quantum` (hybrid quantum-classical networks,
variational circuits), `std.nn_private` (private inference on encrypted
data), `std.quantum_priv` (quantum-secure MPC, threshold schemes).

**Applications** — `std.agent` (autonomous agents with proofs), `std.defi`
(financial instruments), `std.science` (verifiable computation for research).

---

## Documentation

Organized following the [Diataxis](https://diataxis.fr/) framework.
Full index: [docs/README.md](docs/README.md)

| Category | Start Here |
|----------|-----------|
| Tutorials | [The Builder's Journey](docs/tutorials/README.md) — six chapters, from hello-proof to a DAO |
| Guides | [Compiling a Program](docs/guides/compiling-a-program.md) — build, test, deploy, prove, verify |
| Reference | [Language Reference](reference/language.md) — types, operators, builtins, grammar |
| Explanation | [Vision](docs/explanation/vision.md) — why Trident exists and what it's building toward |

---

## Design Principles

1. Field elements all the way down. The core numeric type is a finite field element.
2. Bounded execution. All loops require explicit bounds. No recursion. No halting problem.
3. Compile-time everything. All type widths, array sizes, and costs known statically.
4. Constraints are features. No heap, no dynamic dispatch, no callbacks — safety guarantees.
5. Provable-first. Designed for ZK. These constraints make great conventional programs too.
6. Field-native intelligence. Neural networks in field arithmetic, not floats.
7. Quantum-native by construction. The same field structure optimizes for quantum execution.
8. Minimal dependencies. 5 runtime crates: clap, ariadne, blake3, tower-lsp, tokio.

---

## Editor Support

| Editor | Setup |
|--------|-------|
| [Zed](https://zed.dev/) | Extension in `editor/zed/` |
| [Helix](https://helix-editor.com/) | Config in `editor/helix/languages.toml` |
| Any LSP client | `trident lsp` — diagnostics, completions, hover, go-to-definition |

---

## License

[Cyber License](docs/explanation/cyber-license.md): Don't trust. Don't fear. Don't beg.
