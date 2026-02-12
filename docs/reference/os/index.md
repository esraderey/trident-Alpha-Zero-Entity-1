# Operating Systems

[← Target Reference](../targets.md)

25 OSes. The OS is the runtime — storage, accounts, syscalls, billing.

## Provable

| OS | VM | Runtime binding | Doc |
|----|----|-----------------|-----|
| Neptune | TRITON | `neptune.ext.*` | [neptune.md](neptune.md) |
| Polygon Miden | MIDEN | `miden.ext.*` | [miden.md](miden.md) |
| Nockchain | NOCK | `nockchain.ext.*` | [nockchain.md](nockchain.md) |
| Starknet | CAIRO | `starknet.ext.*` | [starknet.md](starknet.md) |
| Boundless | RISCZERO | `boundless.ext.*` | [boundless.md](boundless.md) |
| Succinct | SP1 | `succinct.ext.*` | [succinct.md](succinct.md) |
| OpenVM Network | OPENVM | `openvm.ext.*` | [openvm-network.md](openvm-network.md) |
| Aleo | AVM | `aleo.ext.*` | [aleo.md](aleo.md) |
| Aztec | AZTEC | `aztec.ext.*` | [aztec.md](aztec.md) |

## Blockchain

| OS | VM | Runtime binding | Doc |
|----|----|-----------------|-----|
| Ethereum | EVM | `ethereum.ext.*` | [ethereum.md](ethereum.md) |
| Solana | SBPF | `solana.ext.*` | [solana.md](solana.md) |
| Near Protocol | WASM | `near.ext.*` | [near.md](near.md) |
| Cosmos (100+ chains) | WASM | `cosmwasm.ext.*` | [cosmwasm.md](cosmwasm.md) |
| Arbitrum | WASM + EVM | `arbitrum.ext.*` | [arbitrum.md](arbitrum.md) |
| Internet Computer | WASM | `icp.ext.*` | [icp.md](icp.md) |
| Sui | MOVEVM | `sui.ext.*` | [sui.md](sui.md) |
| Aptos | MOVEVM | `aptos.ext.*` | [aptos.md](aptos.md) |
| Ton | TVM | `ton.ext.*` | [ton.md](ton.md) |
| Nervos CKB | CKB | `nervos.ext.*` | [nervos.md](nervos.md) |
| Polkadot | POLKAVM | `polkadot.ext.*` | [polkadot.md](polkadot.md) |

## Traditional

| OS | VM | Runtime binding | Doc |
|----|----|-----------------|-----|
| Linux | X86-64 / ARM64 / RISCV | `linux.ext.*` | [linux.md](linux.md) |
| macOS | ARM64 / X86-64 | `macos.ext.*` | [macos.md](macos.md) |
| Android | ARM64 / X86-64 | `android.ext.*` | [android.md](android.md) |
| WASI | WASM | `wasi.ext.*` | [wasi.md](wasi.md) |
| Browser | WASM | `browser.ext.*` | [browser.md](browser.md) |

---

See [targets.md](../targets.md) for the full OS model, tier compatibility,
type/builtin availability, and cost model overview.
