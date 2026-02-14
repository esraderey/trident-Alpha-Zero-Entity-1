# Benchmarking Provable Languages

A framework for measuring and comparing provable computation languages.

## Why This Matters

Every provable language makes the same implicit claim: "write a program,
get a proof." But the cost of that proof varies by orders of magnitude
depending on the language, the target VM, the proof system, and the
compiler. There is no standard way to compare them.

This suite establishes a methodology.

## The Languages

| Language | VM / Backend | Proof System | Field | Architecture |
|----------|-------------|--------------|-------|-------------|
| **Trident** | Triton VM | STARK | Goldilocks (64-bit) | Stack |
| **Miden Assembly** | Miden VM | STARK | Goldilocks (64-bit) | Stack |
| **Cairo** | Cairo VM / Stone | STARK | Mersenne-31 / Stark-252 | Register |
| **Noir** | ACIR / Barretenberg | UltraPlonk / Honk | BN254 | Circuit (ACIR) |
| **Leo** | Aleo / snarkVM | Marlin | BLS12-377 | Circuit (R1CS) |
| **Circom** | Groth16 / Plonk | SNARK | BN254 | Circuit (R1CS) |
| **RISC Zero** | RISC Zero VM | STARK | Baby Bear (31-bit) | Register (RISC-V) |
| **SP1** | SP1 VM | STARK + recursion | Baby Bear (31-bit) | Register (RISC-V) |
| **Lurk** | Lurk VM | Nova (IVC) | Pallas/Vesta | Tree (Lisp) |
| **Valida** | Valida VM | STARK | Mersenne-31 | Register |
| **o1js** | Kimchi | Plonk + recursion | Pasta curves | Circuit |

Apples-to-oranges is unavoidable when fields differ. The benchmarks
account for this by reporting raw metrics per-system and normalizing
only where meaningful (e.g., security bits per prover second).

## Metrics

### Primary: Prover Cost

The single number that matters most is **how expensive it is to prove
the program executed correctly**.

| Metric | Unit | What it measures |
|--------|------|-----------------|
| **Cycle count** | cycles | VM execution length (stack/register VMs) |
| **Constraint count** | constraints | Circuit size (R1CS/Plonk systems) |
| **Trace height** | rows | Padded execution trace for STARK systems |
| **Proof generation time** | seconds | Wall-clock proving time on reference hardware |
| **Peak prover memory** | MB | RAM required by the prover |

Cycle count and constraint count are the VM-native cost units.
Trace height is the STARK-specific cost (next power of two from cycle count).
Proof gen time and memory are the end-user costs.

### Secondary: Proof Quality

| Metric | Unit | What it measures |
|--------|------|-----------------|
| **Proof size** | bytes | Bandwidth cost of transmitting the proof |
| **Verification time** | ms | How fast a verifier checks the proof |
| **Security level** | bits | Conjectured security (80 / 100 / 128) |

### Tertiary: Developer Experience

| Metric | Unit | What it measures |
|--------|------|-----------------|
| **Source lines** | LOC | How much code the programmer writes |
| **Compilation time** | ms | Source to executable/circuit |
| **Compiler overhead** | ratio | Compiled output size vs hand-written baseline |

## Benchmark Programs

Eight tiers, from micro-operations to full applications.

### Tier 0: Micro-ops

Measure individual instruction cost. Not meaningful for language
comparison, but essential for compiler overhead analysis.

| Program | Description | Key operation |
|---------|-------------|---------------|
| `field_arithmetic` | a+b, a*b, sum+prod | Modular arithmetic |
| `loop_sum` | Sum 1..N in a loop | Loop + accumulator |

### Tier 1: Cryptographic Primitives

The atomic operations of provable computation.

| Program | Description | Key operation |
|---------|-------------|---------------|
| `hash_preimage` | Hash a single field element | Hash function |
| `hash_chain` | Triple hash chain | Repeated hashing |
| `sponge_absorb` | Sponge init + absorb 10 + squeeze | Sponge API |
| `merkle_verify3` | Depth-3 Merkle path verification | Merkle authentication |

### Tier 2: Signature Verification

The universal bottleneck of provable programs.

| Program | Description | Key operation |
|---------|-------------|---------------|
| `ecdsa_verify` | ECDSA signature verification (secp256k1) | Elliptic curve arithmetic |
| `ed25519_verify` | EdDSA signature verification | Twisted Edwards curve |
| `schnorr_verify` | Schnorr signature verification | Scalar multiplication |

### Tier 3: Protocol Operations

Real operations from blockchain and authentication protocols.

| Program | Description | Key operation |
|---------|-------------|---------------|
| `token_transfer` | Balance verification + range check | Arithmetic + U32 cast |
| `conditional_auth` | Hash-preimage authentication with branching | Conditional + hash |
| `merkle_update` | Verify old leaf, compute new leaf, verify new root | Double Merkle proof |
| `nullifier` | Compute nullifier from secret + nonce | Privacy primitive |

### Tier 4: Recursive Verification

Proofs verifying other proofs. The frontier capability.

| Program | Description | Key operation |
|---------|-------------|---------------|
| `recursive_verify` | Verify a single STARK/SNARK proof | Full verifier circuit |
| `proof_aggregation` | Aggregate N proofs into one | Batch recursion |

### Tier 5: Hash Functions (Apples-to-Apples)

Same algorithm, different implementations. Measures language expressiveness
for bit-manipulation-heavy code.

| Program | Description | Key operation |
|---------|-------------|---------------|
| `sha256_compress` | SHA-256 single-block compression | 64 rounds, 32-bit ops |
| `keccak_f1600` | Keccak-f[1600] permutation | 24 rounds, 64-bit ops |
| `poseidon2_perm` | Poseidon2 permutation over native field | Algebraic hash |
| `rescue_prime` | Rescue-Prime permutation | Algebraic hash (alt) |

### Tier 6: Applications

Full programs that represent real use cases.

| Program | Description | Key operation |
|---------|-------------|---------------|
| `fungible_token` | TSP-1 Coin: pay operation | Full token transfer |
| `nft_mint` | TSP-2 Card: mint operation | NFT creation |
| `multisig_spend` | 2-of-3 multisig authorization | Multiple hash preimages |
| `timelock_release` | Time-locked spending | Timestamp comparison |

### Tier 7: Stress Tests

Extreme programs that expose compiler and prover limits.

| Program | Description | Key operation |
|---------|-------------|---------------|
| `deep_merkle` | Depth-20 Merkle verification | 20 hash iterations |
| `wide_sponge` | Absorb 1000 field elements | Memory pressure |
| `nested_conditionals` | 10-deep nested if/else | Control flow |

## Methodology

### Reference Hardware

All timed benchmarks run on:
- Apple M2 Pro, 16 GB RAM (consumer baseline)
- AMD EPYC 7763, 128 GB RAM (server baseline)

Report both. The ratio between them reveals parallelization opportunity.

### Compiler Overhead Ratio

For stack-machine VMs (Triton, Miden), we compare compiler output against
hand-written assembly:

```
overhead = trident_instructions / baseline_instructions
```

An overhead of 1.0x means the compiler is as good as a human.
Current Trident overhead on Tier 0-1: 1.0x - 1.5x.

Hand-written baselines live alongside each benchmark as `.baseline.tasm` files.

### Cross-Language Protocol

1. Implement each benchmark in each language
2. Use the language's standard library where available (don't hand-roll SHA-256 if the language provides it)
3. Use the language's recommended compiler flags (release mode, optimizations on)
4. Measure cycle/constraint count from the compiler or VM
5. Measure proof generation time from the prover binary
6. Report all raw numbers; normalize only in the analysis

### What We Do NOT Compare

- **Different algorithms**: comparing Poseidon-in-Trident vs SHA-256-in-Noir is meaningless. Compare Poseidon-in-Trident vs Poseidon-in-Cairo.
- **Different security levels**: a 80-bit STARK proof is cheaper than a 128-bit Groth16 proof. Report security level alongside every measurement.
- **Prover hardware optimizations**: some provers use GPU acceleration, some don't. Report the configuration.

## Running

```bash
# Trident benchmarks (compiler overhead analysis)
trident bench benches/

# Full benchmark with proof generation (requires Triton VM prover)
trident bench benches/ --prove --target triton
```

## Current Results (Trident → Triton VM)

| Benchmark | Trident | Baseline | Overhead | Trace Height |
|-----------|---------|----------|----------|-------------|
| field_arithmetic | — | 10 | — | — |
| loop_sum | — | — | — | — |
| hash_preimage | — | 8 | — | — |
| hash_chain | — | 16 | — | — |
| sponge_absorb | — | 7 | — | — |
| merkle_verify3 | — | 17 | — | — |
| conditional_auth | — | 22 | — | — |
| token_transfer | — | 18 | — | — |

Run `trident bench benches/` to fill in the Trident column.

## Contributing a Language

To add benchmarks for a new language:

1. Create a directory: `benches/<language>/`
2. Implement each tier's programs in that language
3. Add a `run.sh` that outputs CSV: `benchmark,cycles,constraints,proof_time_ms,proof_size_bytes`
4. Add the language to the table above
5. Submit a PR with raw results and the hardware used

## Prior Art

- [zk-bench](https://zkbench.dev) — Web-based ZK prover benchmarks
- [Polygon Miden benchmarks](https://github.com/0xPolygonMiden/miden-vm) — Miden VM native benchmarks
- [RISC Zero benchmarks](https://dev.risczero.com/benchmarks) — RISC-V guest program benchmarks  
- [Lurk benchmarks](https://github.com/lurk-lab/lurk-rs) — Nova IVC benchmarks
- [Cairo benchmark suite](https://github.com/starkware-libs/cairo) — Cairo VM benchmarks
