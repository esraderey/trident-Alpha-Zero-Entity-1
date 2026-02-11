# Trident Universal Execution: Three Levels of Deployment

**Design document for extending Trident from a ZK-first language to a universal smart contract language with provable computation as a native capability.**

---

## Motivation

No language exists today that lets developers write a smart contract once and deploy it across EVM, SVM, CosmWasm, and ZK virtual machines. The industry forces a choice: pick your chain, learn its language, rewrite for every target. Trident is already designed around a universal core with backend extensions for ZK virtual machines. This document extends that architecture to encompass general-purpose blockchain execution — making Trident the first language that spans both conventional and provable smart contract targets from a single source.

The key insight: Trident's existing design properties — bounded loops, no heap, no dynamic dispatch, fixed-width types, compile-time cost analysis — are not just ZK requirements. They are properties that make a language *ideal* for safe, portable smart contract execution on any VM. The restrictions that make provable computation possible also make universal deployment possible.

---

## Architecture: Three Levels

```
┌─────────────────────────────────────────────────────────┐
│                  Level 1: Execute Anywhere               │
│  Field, U32, Bool, Digest, structs, bounded loops,      │
│  match, modules, hash(), storage, events                │
│  ─────────────────────────────────────────────────────── │
│  Targets: EVM, SVM, CosmWasm, Triton VM, Miden, Cairo   │
├─────────────────────────────────────────────────────────┤
│              Level 2: Prove Anywhere                     │
│  divine(), pub_read/pub_write, seal events,             │
│  Merkle authentication, sponge construction,            │
│  recursive proof verification, cost annotations         │
│  ─────────────────────────────────────────────────────── │
│  Targets: Triton VM, Miden, Cairo, SP1/RISC-V zkVMs     │
├─────────────────────────────────────────────────────────┤
│          Level 3: Platform Superpowers                   │
│  Target-specific extensions under ext/<platform>/       │
│  ─────────────────────────────────────────────────────── │
│  ext/triton:   XField, asm(triton), kernel, utxo        │
│  ext/neptune:  Neptune transaction model, MAST           │
│  ext/evm:      msg.sender, msg.value, address, payable  │
│  ext/cosmwasm: Deps, Env, IBC, submessages              │
│  ext/svm:      accounts, PDAs, CPI                      │
│  ext/miden:    Miden-specific intrinsics                 │
└─────────────────────────────────────────────────────────┘
```

A `.tri` file that uses only Level 1 constructs compiles to **every** target. The moment it imports Level 2 modules, it compiles only to ZK targets. The moment it imports a Level 3 extension, it is locked to that specific platform. The compiler enforces this statically — there is no runtime check, no silent failure.

---

## Level 1: Execute Anywhere

Level 1 is the universal smart contract language. Programs written at this level deploy to any supported blockchain, whether conventional or zero-knowledge.

### Core Types

| Type       | Width    | Description                                        |
|------------|----------|----------------------------------------------------|
| `Field`    | 1        | Goldilocks field element (mod p = 2^64 - 2^32 + 1) |
| `U32`      | 1        | Unsigned 32-bit integer (range-checked)            |
| `Bool`     | 1        | Boolean (0 or 1)                                   |
| `Digest`   | 5        | Hash digest (5 field elements)                     |
| `[T; N]`   | N*w(T)   | Fixed-size array                                   |
| `(T, U)`   | w(T)+w(U)| Tuple                                              |
| struct     | Σ w(fi)  | Named product type                                 |

**`Field` is core, not an extension.** Every target implements Goldilocks arithmetic:

- **EVM**: `addmod`/`mulmod` over `p` as a `uint64` within `uint256` words. Native EVM opcodes, ~8-10 gas per field operation.
- **CosmWasm**: `u64` arithmetic with modular reduction. Near-zero overhead — it's native Rust math.
- **SVM**: Same as CosmWasm — Rust `u64` with `% p`. Native performance.
- **Triton VM**: Native field elements. Zero overhead.

The Goldilocks prime (2^64 - 2^32 + 1) was chosen for Triton VM because it fits in 64 bits with fast reduction. This same property makes it efficient on every target. On EVM, where the word size is 256 bits, a 64-bit prime is cheaper than native 256-bit operations for many use cases.

### Control Flow

```
// All of this is Level 1 — compiles everywhere

if balance > threshold {
    process(balance)
} else {
    reject()
}

for i in 0..10 bounded 10 {
    accumulate(i)
}

match op_code {
    0 => { transfer() }
    1 => { lock() }
    _ => { reject() }
}
```

Bounded loops, pattern matching, and conditionals compile directly to every target. The `bounded` keyword is enforced by the compiler on all targets, not just ZK — it prevents infinite loops universally.

### Functions and Modules

```
// main.tri
program token

use helpers

fn main() {
    let supply: Field = 1000000
    let result: Field = helpers.double(supply)
    storage.write(0, result)
}
```

```
// helpers.tri
module helpers

pub fn double(x: Field) -> Field {
    x + x
}
```

Module system, size-generic functions (`fn sum<N>(arr: [Field; N])`), struct definitions, and `pub` visibility work identically across all targets.

### Abstract Primitives

Level 1 provides abstract interfaces that dispatch to target-native implementations:

**`hash()`** — target-native hash function:
- Triton VM → Tip5 (1 cycle + 6 coprocessor rows)
- EVM → keccak256 (30 gas)
- CosmWasm → SHA-256 (native cosmwasm_std)
- SVM → SHA-256 (Solana syscall)

**`storage.read()` / `storage.write()`** — abstract persistent state:
- Triton VM → RAM read/write with Merkle commitment
- EVM → SSTORE/SLOAD (storage slots)
- CosmWasm → deps.storage get/set
- SVM → account data read/write

**`emit`** — open events:
- Triton VM → public output
- EVM → LOG opcodes (indexed events)
- CosmWasm → cosmwasm_std events
- SVM → Solana program logs

### What Level 1 Programs Look Like

```
program counter

use std.core.field
use std.io.storage

fn increment() {
    let current: Field = storage.read(0)
    let next: Field = current + 1
    storage.write(0, next)
    emit CounterUpdated { value: next }
}

event CounterUpdated {
    value: Field,
}
```

This program compiles to Solidity, CosmWasm Rust, Anchor Rust, and TASM. The developer writes it once. The compiler handles storage layout, event encoding, and entry point generation per target.

### On-Chain Infrastructure

Each non-ZK target requires a small field arithmetic library:

| Target   | Library                | What It Provides                          | Overhead    |
|----------|------------------------|-------------------------------------------|-------------|
| EVM      | `GoldilocksLib.sol`    | `fadd`, `fsub`, `fmul`, `finv`, `Digest`  | ~10 gas/op  |
| CosmWasm | `goldilocks` crate     | `Field` type with `Add/Sub/Mul/Inv` impls | Near-zero   |
| SVM      | `goldilocks` crate     | Same Rust crate, shared with CosmWasm     | Near-zero   |

On CosmWasm and SVM this is trivially efficient — Goldilocks fits in a `u64`, and modular arithmetic over a Mersenne-like prime is a handful of CPU instructions. On EVM it adds ~10 gas per operation via `addmod`/`mulmod`, which is modest compared to typical contract gas costs.

---

## Level 2: Prove Anywhere

Level 2 adds zero-knowledge capabilities. Programs at this level compile only to ZK virtual machines (Triton VM, Miden, Cairo, SP1/RISC-V zkVMs). They gain the ability to produce cryptographic proofs of correct execution.

### What Level 2 Adds

**`divine()` — secret witness input.** The prover supplies data invisible to the verifier. No equivalent exists in conventional smart contracts.

```
let secret: Field = divine()          // verifier never sees this
let preimage: Digest = divine5()      // 5 field elements, secret
```

**`pub_read()` / `pub_write()` — public I/O for proof circuits.** The verifier provides inputs and reads outputs. These define the proof's public claim.

```
let root: Digest = pub_read5()        // verifier provides state root
// ... computation ...
pub_write(result)                     // verifier sees the result
```

**`seal` — privacy-preserving events.** Fields are hashed; only the digest is visible. The verifier knows an event occurred but cannot read its contents.

```
seal Nullifier { account_id: id, nonce: n }
// Verifier sees: hash(id, n) — not the actual values
```

**Merkle authentication.** The divine-and-authenticate pattern: prover secretly inputs data, then proves it belongs to a committed Merkle tree.

```
use std.crypto.merkle

let root: Digest = pub_read5()
let leaf: Digest = divine5()
let index: U32 = as_u32(divine())
merkle.verify(root, leaf, index, TREE_DEPTH)
```

**Sponge construction.** Incremental hashing for variable-length data.

```
use std.crypto.hash

hash.sponge_init()
hash.sponge_absorb(data_chunk_1)
hash.sponge_absorb(data_chunk_2)
let digest: Digest = hash.sponge_squeeze()
```

**Recursive proof verification.** Verify a STARK proof inside another STARK proof — Triton VM's architectural sweet spot.

**Cost annotations.** Compile-time proving cost analysis across all algebraic tables.

```
$ trident build program.tri --target triton --costs
```

### Level 2 Programs

A Level 2 program looks like a Level 1 program with `divine()`, `pub_read()`/`pub_write()`, and `seal` added. The Level 1 business logic (field arithmetic, structs, bounded loops, match) remains identical.

```
program private_transfer

use std.crypto.hash
use std.crypto.merkle
use std.io.io

fn main() {
    let old_root: Digest = pub_read5()
    let new_root: Digest = pub_read5()
    let amount: Field = pub_read()

    // Divine sender state (secret)
    let s_bal: Field = divine()
    let s_auth: Field = divine()
    // ... authenticate against old_root ...

    verify_auth(s_auth)

    let new_s_bal: Field = s_bal - amount
    assert_non_negative(new_s_bal)

    // ... compute new Merkle root ...

    seal Transfer { from_id: s_id, to_id: r_id, amount: amount }
}
```

The business logic (`s_bal - amount`, `assert_non_negative`, struct manipulation) is pure Level 1. The ZK machinery (`divine`, `pub_read`, `seal`, Merkle proofs) is Level 2. Clean separation.

---

## Level 3: Platform Superpowers

Level 3 provides target-specific capabilities that have no portable equivalent. Importing any `ext/<platform>/` module locks the program to that platform. This is explicit and intentional — when you need `msg.sender`, you're writing an EVM program.

### ext/triton — Triton VM Extensions

```
use ext.triton.xfield

let a: XField = xfield.new(x0, x1, x2)
let b: XField = xfield.inv(a)
```

- `XField` type (extension field triples)
- `asm(triton) { ... }` blocks with TASM instructions
- `xx_dot_step`, `xb_dot_step` for FRI verification
- Direct hash coprocessor access

### ext/neptune — Neptune Blockchain Extensions

```
use ext.neptune.kernel
use ext.neptune.utxo

let height: U32 = kernel.tree_height()
utxo.authenticate(leaf, index)
```

- Neptune kernel interface (MAST hash, tree height)
- UTXO authentication primitives
- Neptune transaction model (lock scripts, type scripts)
- Neptune-specific consensus constraints

### ext/evm — EVM Extensions

```
use ext.evm.context
use ext.evm.token

let sender: Address = context.msg_sender()
let value: U256 = context.msg_value()

fn withdraw() {
    require(context.msg_sender() == owner)
    context.transfer(owner, balance)
}
```

- `Address` type (20 bytes), `U256` type (256-bit integer)
- `msg.sender`, `msg.value`, `block.timestamp`, `block.number`
- `transfer()`, `call()`, `delegatecall()`
- ERC-20/ERC-721 standard interfaces
- `payable` function modifier
- Reentrancy guard patterns

### ext/cosmwasm — CosmWasm Extensions

```
use ext.cosmwasm.context
use ext.cosmwasm.ibc

let sender: Addr = context.info_sender()
let funds: Vec<Coin> = context.info_funds()

fn execute() -> Response {
    let msg = ibc.send_packet(channel, data, timeout)
    Response.new().add_message(msg)
}
```

- `Deps`, `Env`, `MessageInfo` access
- `Response` builder with messages and submessages
- IBC channel and packet operations
- CosmWasm-specific storage patterns (`Item`, `Map`)
- Bank module integration

### ext/svm — Solana VM Extensions

```
use ext.svm.accounts
use ext.svm.pda

#[account]
struct TokenAccount {
    pub owner: Pubkey,
    pub amount: U64,
}

fn transfer(ctx: Context) {
    let pda = pda.derive(program_id, &[b"vault", user.key()])
    // ...
}
```

- Account declarations with `#[account]` attribute
- `Pubkey` type
- PDA derivation
- Cross-Program Invocation (CPI)
- Anchor-compatible instruction dispatch

---

## Compiler Architecture

```
Source (.tri)
    │
    ├── Parse ──→ AST
    │
    ├── Level Check ──→ determines minimum level required
    │                    (Level 1 if no ZK or platform imports,
    │                     Level 2 if ZK imports present,
    │                     Level 3 if platform extension imported)
    │
    ├── Type Check ──→ validates types including target-specific types
    │
    └── Backend Emit ──→ selected by --target flag
         │
         ├── triton  → TASM (direct emit, no IR)
         ├── miden   → MASM (direct emit for stack machines)
         ├── cairo   → Sierra (minimal IR for register machines)
         ├── sp1     → RISC-V ELF (minimal IR)
         ├── evm     → Vyper or Yul → EVM bytecode
         ├── cosmwasm → Rust → WASM
         └── svm     → Rust (Anchor) → BPF
```

### Backend Responsibilities

Each backend implements a trait that maps abstract operations to target-specific code:

```
trait Backend {
    // Level 1: universal operations
    fn emit_field_add(a: Operand, b: Operand) -> Code;
    fn emit_field_mul(a: Operand, b: Operand) -> Code;
    fn emit_field_inv(a: Operand) -> Code;
    fn emit_hash(inputs: &[Operand]) -> Code;
    fn emit_storage_read(key: Operand) -> Code;
    fn emit_storage_write(key: Operand, value: Operand) -> Code;
    fn emit_event(name: &str, fields: &[Operand]) -> Code;
    fn emit_bounded_loop(bound: usize, body: Code) -> Code;
    fn emit_match(scrutinee: Operand, arms: &[Arm]) -> Code;

    // Level 2: ZK operations (default: compile error)
    fn emit_divine() -> Code { error!("divine() requires ZK target") }
    fn emit_pub_read() -> Code { error!("pub_read() requires ZK target") }
    fn emit_pub_write(value: Operand) -> Code { ... }
    fn emit_seal_event(name: &str, fields: &[Operand]) -> Code { ... }
    fn emit_merkle_verify(...) -> Code { ... }

    // Level 3: platform-specific (implemented per backend)
    // Not part of the trait — accessed through ext/ modules
}
```

### Target-Specific Code Generation

**EVM backend** generates Vyper (preferred for Trident's philosophy of simplicity) or Yul:

```vyper
# Generated from: let sum: Field = a + b
# where Field = Goldilocks prime

GOLDILOCKS_P: constant(uint256) = 18446744069414584321  # 2^64 - 2^32 + 1

@internal
def field_add(a: uint256, b: uint256) -> uint256:
    return addmod(a, b, GOLDILOCKS_P)
```

**CosmWasm backend** generates Rust with cosmwasm-std:

```rust
// Generated from: storage.write(0, result)
pub fn execute(deps: DepsMut, _env: Env, _info: MessageInfo, 
               msg: ExecuteMsg) -> Result<Response, ContractError> {
    let result = field_add(current, Field(1));
    COUNTER.save(deps.storage, &result)?;
    Ok(Response::new().add_attribute("value", result.0.to_string()))
}
```

**SVM backend** generates Anchor Rust:

```rust
// Generated from: storage.write(0, result)
pub fn increment(ctx: Context<Increment>) -> Result<()> {
    let counter = &mut ctx.accounts.counter;
    counter.value = field_add(counter.value, Field(1));
    Ok(())
}
```

---

## Storage Model Translation

The most challenging aspect of universal deployment is mapping Trident's abstract storage to each target's state model.

### Abstract Storage Interface

```
// Level 1 storage — key-value with Field keys and Field/Digest values
storage.write(key: Field, value: Field)
storage.write_digest(key: Field, value: Digest)
let v: Field = storage.read(key: Field)
let d: Digest = storage.read_digest(key: Field)
```

### Per-Target Mapping

| Target   | Storage Mapping                                                  |
|----------|------------------------------------------------------------------|
| Triton VM | RAM addresses + Merkle commitment over state                    |
| EVM       | `mapping(uint256 => uint256)` at slot derived from key          |
| CosmWasm  | `Map<u64, u64>` via cosmwasm_std storage                        |
| SVM       | Account data with field offset derived from key                 |

### Hash Function Mapping

| Target   | `hash()` implementation  | `Digest` representation        |
|----------|--------------------------|--------------------------------|
| Triton VM | Tip5 permutation        | 5 Goldilocks field elements    |
| EVM       | keccak256               | 5 uint64 packed into uint256[] |
| CosmWasm  | SHA-256                 | 5 u64 from first 40 bytes      |
| SVM       | SHA-256 syscall         | 5 u64 from first 40 bytes      |

The `Digest` type always contains 5 field elements regardless of target. The hash function produces target-native output that is then mapped into this canonical 5-element representation.

---

## Cross-Chain Proof Verification

The three-level architecture creates a natural bridge pattern:

1. **Write business logic in Level 1** — deploys everywhere.
2. **Write provable version in Level 1 + Level 2** — same logic, but generates STARK proofs on Triton VM.
3. **Deploy verifier contracts on other chains** — Level 3 EVM/CosmWasm/SVM contracts that verify Triton VM proofs.

The verifier contracts need:
- `GoldilocksLib` — field arithmetic (Level 1 infrastructure, already deployed)
- `Poseidon2Lib` / `Tip5Lib` — algebraic hash for proof verification
- `FRIVerifier` — STARK verification logic

Because Level 1 already requires Goldilocks field libraries on every target, the infrastructure for proof verification is partially in place by default. Adding hash and FRI verification libraries completes the bridge.

```
┌──────────────┐     STARK proof     ┌──────────────────┐
│  Neptune     │ ──────────────────→ │  EVM contract    │
│  (Triton VM) │                     │  FRIVerifier.sol │
│  Level 1+2   │                     │  + GoldilocksLib │
│  program     │                     │  + Tip5Lib       │
└──────────────┘                     └──────────────────┘
       │                                      │
       │ same Level 1 logic                   │ verifies proof of
       │ different execution model            │ that same logic
       ▼                                      ▼
  Proved locally,                     Verified on-chain,
  result is a proof                   result is trust
```

---

## Development Roadmap

### Phase 1: Level 2 Complete (Current)

Trident already implements Level 2 with the Triton VM backend. The `std/core/`, `std/io/`, `std/crypto/` modules and `ext/triton/` extensions are working. Cost analysis, bounded loops, the full type system, and the TASM emitter are operational.

### Phase 2: Level 1 Extraction

Factor out the universal subset from the existing codebase:
- Define the Level 1 boundary formally in the language specification
- Implement level checking in the compiler (error if Level 2/3 constructs used with non-ZK target)
- Build the `goldilocks` crate for CosmWasm/SVM (trivial — pure Rust u64 arithmetic)
- Build `GoldilocksLib.sol` for EVM

### Phase 3: First Non-ZK Backend

CosmWasm is the natural first target:
- Closest state model to Trident's key-value storage abstraction
- Same language family (Rust)
- Cosmos ecosystem alignment with Neptune
- Code generation: Trident AST → Rust with cosmwasm-std

### Phase 4: EVM Backend

- Code generation: Trident AST → Vyper (preferred) or Yul
- Deploy `GoldilocksLib.sol` as a shared library contract
- Storage slot derivation from Field keys
- Event encoding from Trident events to EVM logs

### Phase 5: SVM Backend

- Code generation: Trident AST → Anchor Rust
- Account model design (how Trident storage maps to Solana accounts)
- PDA derivation scheme for Trident program state

### Phase 6: Cross-Chain Proof Verification

- Tip5/Poseidon2 library contracts on EVM, CosmWasm, SVM
- FRI verifier contracts
- End-to-end: prove on Triton VM, verify on EVM

### Phase 7: Additional ZK Backends

- Miden VM backend (stack machine, similar to Triton, direct emit)
- Cairo backend (register machine, needs minimal IR)
- SP1/RISC-V backend

---

## Design Principles

**Field-native everywhere.** Goldilocks field arithmetic is core to the language, not an extension. Every target implements it. This is what makes Trident's type system portable — `Field` means the same thing on every chain.

**Hash-agnostic at Level 1.** The `hash()` function dispatches to target-native implementations. Programs that need a specific hash (Tip5, Poseidon2) use Level 2 or Level 3 explicit imports.

**Storage-abstract at Level 1.** Key-value read/write with Field keys. Each backend maps this to its native state model. Programs that need target-specific state patterns use Level 3 extensions.

**Levels are additive, not exclusive.** A program can use Level 1 + Level 3/EVM constructs. It will only compile for EVM, but it still benefits from Level 1's type system, bounded loops, and field arithmetic. The levels are capabilities, not mutually exclusive modes.

**The compiler is the gatekeeper.** Level checking is a compile-time pass, not a convention. `trident build program.tri --target evm` fails with a clear error if the source imports `std.io.io.divine()`. No surprises at deployment.

**Vyper philosophy applies universally.** One obvious way to do everything. No metaprogramming. No dynamic dispatch. No heap. No unbounded iteration. These constraints make the language safe for money on any chain, not just in ZK contexts.

---

## Appendix: Why Not An Existing Language?

**Fe** (Ethereum Foundation / Argot) is architecturally interesting — Rust-like syntax, uses Yul IR which was designed for multi-target compilation. But Fe is EVM-only, currently mid-rewrite with a non-functional master branch, and its entire semantic model (storage slots, msg.sender, contract calls) is EVM-specific. Extending Fe to other targets would require redesigning its type system and execution model.

**Rust** is used by both CosmWasm and Solana, but the contract interfaces are completely incompatible. You cannot take an Anchor program and deploy it as a CosmWasm contract. The VM-specific code (entry points, state handling, CPI/submessages) dominates the contract structure.

**Solidity** via Solang attempts EVM → SVM compilation but remains experimental and not production-grade.

**No existing language** treats field arithmetic, bounded execution, and abstract storage as core primitives. Trident does, because these properties emerged naturally from the requirements of provable computation. The discovery is that these same properties are exactly what's needed for universal smart contract deployment.

---

*This document describes an architectural extension to Trident. The Level 2 implementation (Triton VM backend) is operational. Levels 1 and 3 for non-ZK targets are proposed. The design is open for discussion and refinement.*
