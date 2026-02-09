# ZK-Native Fungible Token Standard — PLUMB

## Overview

A minimal fungible token for Triton VM with 5 operations in PLUMB order:
**pay**, **lock(time)**, **update**, **mint**, and **burn**. The ledger is a
Merkle tree of account leaves. Token configuration (authorities + hooks) is a
separate on-chain commitment. Token metadata (attributes + oracle references)
is a standalone commitment. Each operation is a zero-knowledge proof that a
valid state transition occurred.

Every operation verifies the full config hash (all 10 fields) and extracts
its dedicated authority and hook. Authorities control who can perform the
operation. Hooks are external program IDs that the verifier composes with the
token proof at the protocol level, enabling per-operation custom business logic.

## Design Influences

| Pattern | Source | How applied |
|---|---|---|
| Merkle tree state | Mina, Zcash | All balances in a binary Merkle tree |
| Account abstraction | ERC-4337, Aztec | `auth_hash` — pluggable authorization |
| Nullifiers | Zcash | `hash(id, nonce)` prevents replay |
| Time-locks | Bitcoin CLTV, Solana | `lock_until` field per account |
| Supply invariant | ERC-20, CW20 | `supply` proven invariant |
| Range checks | ZK best practice | `as_u32(balance - amount)` for non-negative |
| Authority separation | SPL Token | Separate authority key for each operation |
| Oracle references | Chainlink, Pyth | `price_oracle`, `volume_oracle` in metadata |
| Upgradable config | ERC-1967 proxy | Admin-controlled config with renounce |
| Token metadata | ERC-721, SPL Token-2022 | Hashed metadata as standalone commitment |
| Per-operation hooks | ERC-777, ERC-1155 | Dedicated hook program per operation |
| Dual authorization | Aztec, SPL Token | Config-level + account-level auth for regulated tokens |

## State Model

### Account Leaf (10 field elements, hashed to Digest)

```
leaf = hash(account_id, balance, nonce, auth_hash, lock_until, 0, 0, 0, 0, 0)
```

| Field | Type | Description |
|---|---|---|
| `account_id` | Field | Unique account identifier (pubkey hash) |
| `balance` | Field | Token balance (must fit in U32 range) |
| `nonce` | Field | Monotonic counter, prevents replay |
| `auth_hash` | Field | Hash of authorization secret |
| `lock_until` | Field | Timestamp until which tokens are locked (0 = unlocked) |

### Token Config — business logic (10 field elements, hashed to Digest)

Config contains 5 authorities and 5 hooks in PLUMB order — all 10 hash slots
used. Every operation verifies config to bind the proof to the token and
extract its authority and hook.

```
config = hash(admin_auth, pay_auth, lock_auth, mint_auth, burn_auth,
              pay_hook, lock_hook, update_hook, mint_hook, burn_hook)
```

| Field | Type | Description |
|---|---|---|
| `admin_auth` | Field | Hash of admin secret. 0 = renounced (config permanently immutable) |
| `pay_auth` | Field | Config-level pay authority. 0 = account auth only, non-zero = dual auth required |
| `lock_auth` | Field | Config-level lock authority. 0 = account auth only, non-zero = dual auth required |
| `mint_auth` | Field | Config-level mint authority. 0 = minting disabled, non-zero = authorized |
| `burn_auth` | Field | Config-level burn authority. 0 = account auth only, non-zero = dual auth required |
| `pay_hook` | Field | External program ID for pay business logic (0 = none) |
| `lock_hook` | Field | External program ID for lock business logic (0 = none) |
| `update_hook` | Field | External program ID for update business logic (0 = none) |
| `mint_hook` | Field | External program ID for mint business logic (0 = none) |
| `burn_hook` | Field | External program ID for burn business logic (0 = none) |

#### Authority Semantics

| Operation type | Auth = 0 | Auth != 0 |
|---|---|---|
| Account ops (pay, lock, burn) | Account auth only (permissionless) | Dual auth: account + config authority |
| Config ops (mint) | Operation disabled | Config authority required |
| Config ops (update) | Renounced (permanently frozen) | Admin authority required |

### Token Metadata — descriptive (standalone commitment, 10 field elements)

Metadata contains token attributes and oracle references. It is a standalone
on-chain commitment, independent of config. It does not affect circuit
business logic directly.

```
metadata = hash(name_hash, ticker_hash, teaser_hash, site_hash, custom_hash,
                price_oracle, volume_oracle, 0, 0, 0)
```

| Field | Type | Description |
|---|---|---|
| `name_hash` | Field | Hash of token name string |
| `ticker_hash` | Field | Hash of ticker symbol |
| `teaser_hash` | Field | Hash of token description/teaser |
| `site_hash` | Field | Content hash of token website/frontend (e.g. IPFS CID) |
| `custom_hash` | Field | Hash of arbitrary custom metadata (application-specific) |
| `price_oracle` | Field | Reference to external price oracle program (0 = none) |
| `volume_oracle` | Field | Reference to external volume oracle program (0 = none) |

### Merkle Tree

- Binary tree of depth `TREE_DEPTH` (e.g. 20)
- Internal node: `hash(left[0..5], right[0..5])`
- Root is the public state commitment

### Global Public State

- `state_root: Digest` — Merkle root of all accounts
- `supply: Field` — sum of all balances
- `config_hash: Digest` — token configuration commitment (5 authorities + 5 hooks)
- `metadata_hash: Digest` — token metadata commitment (standalone)
- `current_time: Field` — block timestamp for time-lock checks

## Hooks

Each operation has a dedicated hook field in config. A hook is a reference to
an external ZK program that implements custom business logic for that operation.

| Hook | Triggered by | Example use case |
|---|---|---|
| `pay_hook` | Every payment | Whitelist/blacklist, transfer limits, compliance |
| `lock_hook` | Every lock | Maximum lock duration, lock rewards |
| `update_hook` | Every config update | Multi-sig requirement, timelock on upgrades |
| `mint_hook` | Every mint | Cap enforcement, vesting schedule, KYC gate |
| `burn_hook` | Every burn | Minimum burn amount, burn tax, audit trail |

**Composition model:** The token circuit proves the state transition is valid
and that the config (including hook references) is authentic. The verifier then
composes the token proof with the hook program's proof, ensuring both are
satisfied. If `hook == 0`, no external proof is required.

## Operations

All operations verify the full config hash (divine 10 config fields, hash,
assert match) and extract their dedicated authority and hook reference.

### Op 0: Pay

Transfer `amount` tokens from sender to receiver.

**Public I/O:** `op, old_root(5), new_root(5), supply, current_time, amount, config(5)`
**Secret inputs:** config fields (10), sender fields, receiver fields, auth witnesses

**Constraints:**
1. Config verified (10 fields), `pay_auth` and `pay_hook` extracted
2. Sender leaf verifies against `old_root`
3. Account-level authorization: `hash(secret) == sender.auth_hash`
4. Config-level pay authorization: if `pay_auth != 0`, dual auth required
5. `current_time >= sender.lock_until` (not locked)
6. `sender.balance >= amount` (range check)
7. New sender: `balance -= amount`, `nonce += 1`
8. New receiver: `balance += amount`
9. New leaves produce `new_root`
10. Supply unchanged

### Op 1: Lock(time)

Lock an account's tokens until a future timestamp.

**Public I/O:** `op, old_root(5), new_root(5), supply, lock_until_time, config(5)`
**Secret inputs:** config fields (10), account fields, auth witnesses

**Constraints:**
1. Config verified (10 fields), `lock_auth` and `lock_hook` extracted
2. Account-level authorization (always required)
3. Config-level lock authorization: if `lock_auth != 0`, dual auth required
4. `lock_until_time >= account.lock_until` (can only extend locks, not shorten)
5. Updated leaf: `lock_until = lock_until_time`, `nonce += 1`
6. Merkle root updated, supply unchanged

### Op 2: Update

Update token configuration (authorities, hooks).
Setting `admin_auth = 0` in the new config permanently renounces authority.

**Public I/O:** `op, old_root(5), new_root(5), supply, old_config(5), new_config(5)`
**Secret inputs:** old config fields (10), new config fields (10), admin_secret

**Constraints:**
1. `old_root == new_root` (account state unchanged)
2. Old config fields (10) hash to `old_config`, `update_hook` extracted
3. Admin authorized: `hash(admin_secret) == old_config.admin_auth`
4. `admin_auth != 0` (not renounced — enforced by hash preimage infeasibility)
5. New config fields (10) hash to `new_config`
6. Supply unchanged

**Renounce:** Setting `admin_auth = 0` in new_config makes config permanently
immutable. No secret hashes to 0, so future update proofs are impossible.

### Op 3: Mint

Create `amount` new tokens for a recipient.

**Public I/O:** `op, old_root(5), new_root(5), old_supply, new_supply, amount, config(5)`
**Secret inputs:** config fields (10), mint_secret, recipient fields

**Constraints:**
1. Config verified (10 fields), `mint_auth` and `mint_hook` extracted
2. Mint authorization: `hash(mint_secret) == config.mint_auth` (always required, 0 = disabled)
3. `new_supply == old_supply + amount`
4. Recipient leaf updated: `balance += amount`
5. Merkle root updated

### Op 4: Burn

Destroy `amount` tokens from an account.

**Public I/O:** `op, old_root(5), new_root(5), old_supply, new_supply, current_time, amount, config(5)`
**Secret inputs:** config fields (10), account fields, auth witnesses

**Constraints:**
1. Config verified (10 fields), `burn_auth` and `burn_hook` extracted
2. Account-level authorization (always required)
3. Config-level burn authorization: if `burn_auth != 0`, dual auth required
4. `current_time >= account.lock_until`
5. `account.balance >= amount` (range check)
6. `new_supply == old_supply - amount`
7. Updated leaf: `balance -= amount`, `nonce += 1`
8. Merkle root updated

## Security Properties

1. **No negative balances**: `sub(balance, amount)` then `as_u32()` range check.
   Wrapping field arithmetic produces values > 2^32 which fail the split check.

2. **Replay prevention**: Each mutation increments `nonce`. Verifier tracks
   nullifiers `hash(account_id, old_nonce, 0, ...)` to reject replays.

3. **Time-lock enforcement**: `current_time` is a public input provided by the
   verifier (from the block timestamp). Locked accounts cannot pay or burn
   until `current_time >= lock_until`.

4. **Lock monotonicity**: Locks can only be extended, preventing an attacker
   from removing a lock early. Only the account owner can set a lock.

5. **Supply conservation**: `supply` is public. Pay leaves it unchanged.
   Mint adds exactly `amount`. Burn subtracts exactly `amount`.

6. **Account abstraction**: Authorization is `hash(secret) == auth_hash`.
   The secret can be anything: a private key, a Shamir share set, a ZK proof
   of identity, etc.

7. **Config binding**: Every operation verifies the full config hash (all 10
   fields). Proofs for TokenA cannot be reused against TokenB because the
   config commitments differ.

8. **Irreversible renounce**: Setting `admin_auth = 0` permanently freezes
   config. The hash preimage of 0 is computationally infeasible under Tip5,
   preventing any future config updates.

9. **Config-state separation**: Config updates cannot modify account balances,
   nonces, or the Merkle tree. `assert_digest(old_root, new_root)` enforces
   this invariant in Op 2.

10. **Hook composability**: Each operation's hook is cryptographically bound to
    the config hash. The verifier ensures that if a hook is non-zero, the
    corresponding external program's proof is also verified. Changing hooks
    requires admin authorization via Op 2.

11. **Symmetric authority model**: Every operation has both a dedicated authority
    and a dedicated hook in config. Account-level ops (pay, lock, burn) use
    conditional dual auth — config authority is additive, not replacing account
    auth. Config-level ops (mint, update) require config authority exclusively.

12. **Safe defaults**: `mint_auth = 0` means minting is disabled (safe).
    `pay_auth/lock_auth/burn_auth = 0` means account auth only (permissionless,
    the natural default). `admin_auth = 0` means permanently frozen.
