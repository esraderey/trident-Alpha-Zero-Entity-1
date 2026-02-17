# Atlas — On-Chain Package Registry

Each OS maintains an Atlas — a TSP-2 Card collection for packages. Packages
are Cards. Publishing is minting. Updating is metadata update. The registry
reuses the same [PLUMB framework](plumb.md) that powers all tokens.

See [Atlas: Why On-Chain Package Management](../docs/explanation/atlas.md)
for design rationale.

---

## 1. Registry Model

| Concept | TSP-2 Mapping |
|---------|---------------|
| Package | Card (one Card per package name) |
| Package name | `asset_id = hash(name)` |
| Package version | `metadata_hash = content_hash(compiled_artifact)` |
| Publisher | `owner_id` (Card owner) |
| Publish | Mint (TSP-2 Op 3) — requires registry mint authority |
| New version | Update metadata (TSP-2 Op 2) — owner updates `metadata_hash` |
| Transfer ownership | Pay (TSP-2 Op 0) — transfer Card to new owner |
| Deprecate | Burn (TSP-2 Op 4) — if `BURNABLE` flag set |

Each OS has its own Atlas collection with independent mint authority.

---

## 2. Package Card Format

### Card Leaf (TSP-2) — 10 field elements

```
leaf = hash(asset_id, owner_id, nonce, auth_hash, lock_until,
            collection_id, metadata_hash, royalty_bps, creator_id, flags)
```

| Field | Registry Meaning |
|-------|-----------------|
| `asset_id` | `hash(package_name)` — deterministic, globally unique per OS |
| `owner_id` | Current package publisher (account hash) |
| `nonce` | Version counter — increments on each publish |
| `auth_hash` | Hash of publisher's authorization secret |
| `lock_until` | 0 (packages are not time-locked) |
| `collection_id` | Atlas collection address for this OS |
| `metadata_hash` | `content_hash(artifact + package_metadata)` |
| `royalty_bps` | 0 (no royalties on package transfers) |
| `creator_id` | Original publisher (immutable — set at first publish) |
| `flags` | `TRANSFERABLE \| UPDATABLE \| BURNABLE` (standard package) |

### Package Metadata — 10 field elements

```
metadata = hash(name_hash, version_hash, source_hash, program_digest, dependencies_hash,
                verification_cert, tags_hash, compiler_version, 0, 0)
```

| Field | Type | Description |
|-------|------|-------------|
| `name_hash` | Field | Hash of fully qualified package name |
| `version_hash` | Field | Hash of semantic version string |
| `source_hash` | Field | Content hash of source `.tri` files |
| `program_digest` | Field | Poseidon2 hash of compiled TASM artifact |
| `dependencies_hash` | Field | Merkle root of dependency content hashes |
| `verification_cert` | Field | STARK proof that compilation was correct (0 if unverified) |
| `tags_hash` | Field | Merkle root of tag strings |
| `compiler_version` | Field | Hash of compiler version used to build |
| *reserved* | Field x2 | Extension space |

---

## 3. Operations

### Publish (Mint — Op 3)

1. Mint authority required (per-OS governance)
2. `asset_id = hash(package_name)` — must not exist (non-membership proof)
3. `metadata_hash = content_hash(artifact + metadata)`
4. `creator_id = publisher_id` (immutable forever)
5. `flags = TRANSFERABLE | UPDATABLE | BURNABLE` (standard package)
6. `nonce = 0`, `lock_until = 0`
7. New leaf inserted into Atlas Merkle tree
8. `asset_count` incremented

### Update (Op 2)

1. Owner auth required, `flags & UPDATABLE` checked
2. Only `metadata_hash` changes (new version artifact)
3. Old version content remains accessible by its content hash
4. `nonce += 1` — provides version ordering

### Transfer Ownership (Pay — Op 0)

1. Standard TSP-2 Pay constraints, `flags & TRANSFERABLE` checked
2. `owner_id` and `auth_hash` change
3. `creator_id`, `collection_id`, `metadata_hash`, `flags` unchanged

### Deprecate (Burn — Op 4)

1. Owner auth required, `flags & BURNABLE` checked
2. Leaf removed from Atlas Merkle tree, `asset_count` decremented
3. Name freed for re-registration (`asset_id` slot available)
4. Existing content hashes remain valid (content-addressed storage)

### Search / Resolve

| Method | Mechanism |
|--------|-----------|
| By name | `hash(name)` -> `asset_id` -> Card lookup |
| By tag | Scan metadata for matching `tags_hash` |
| By type signature | Match function parameter/return types |
| By content hash | Direct artifact lookup in content store |

---

## 4. Three-Tier Resolution

```text
use my_package.module

1. Local files     -> ./my_package/module.tri (project-relative)
2. Atlas cache     -> ~/.trident/cache/<os>/my_package/module.tri
3. On-chain query  -> os.<os>.atlas.my_package -> resolve Card -> fetch artifact
```

| Tier | Source | Precedence |
|------|--------|------------|
| Local | Project-relative `.tri` files | Highest — always wins |
| Cache | `~/.trident/cache/<os>/` | Previously fetched artifacts |
| On-chain | Atlas Card query via `asset_id = hash(name)` | Fallback |

Cache invalidation: the compiler compares the cached `metadata_hash`
against the on-chain Card's `metadata_hash`. A mismatch triggers re-fetch.

---

## 5. Atlas Namespace

```trident
use os.neptune.atlas.my_skill      // Neptune's on-chain Atlas
use os.ethereum.atlas.my_lib       // Ethereum's on-chain Atlas
use os.solana.atlas.my_program     // Solana's on-chain Atlas
```

Each OS namespace is independent — separate TSP-2 collections, separate
governance.

---

## 6. Import / Deploy / Reference

| Mode | Mechanism | When |
|------|-----------|------|
| **Import** | `use std.skill.liquidity` | Compile-time inlining — code becomes part of your circuit |
| **Deploy** | `trident deploy skill.tri --target neptune` | Publish compiled artifact to Atlas as a Card |
| **Reference** | Hook config points to content hash or Atlas name | Verification-time composition — proven separately |

---

## 7. CLI Commands

### trident registry publish

```
trident registry publish [options]
  --registry <url>     Atlas server URL
                       (default: $TRIDENT_REGISTRY_URL or http://127.0.0.1:8090)
  --tag <tag>          Tag definitions (repeatable)
  --input <path>       Input .tri file or directory (adds to store first)
```

Publishes all named definitions from local codebase. Optionally adds
`.tri` files from `--input` first.

### trident registry pull

```
trident registry pull <name|hash> [options]
  --registry <url>     Atlas server URL
```

Accepts a content hash (64 hex chars) or a name. Stores locally and binds
the name.

### trident registry search

```
trident registry search <query> [options]
  --registry <url>     Atlas server URL
  --type               Search by type signature instead of name
  --tag                Search by tag instead of name
```

Output per result: short hash, name, signature, verified status, tags.

### trident deploy (Atlas integration)

```
trident deploy <input> [options]
  --target <os>        Target VM or OS (default: triton)
  --profile <profile>  Compilation profile (default: release)
  --registry <url>     Atlas server URL
  --audit              Run verification before deploying
  --dry-run            Show what would be deployed
```

Accepts a `.tri` file, project directory, or `.deploy/` artifact directory.
Compiles to TASM, generates deployment artifact, publishes to Atlas.

---

## 8. Wire Protocol

HTTP/1.1 JSON API over plain TCP. Production deployments use a reverse proxy
for TLS.

### Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/v1/definitions` | Publish definitions |
| `GET` | `/api/v1/definitions/{hash}` | Pull by content hash |
| `GET` | `/api/v1/names/{name}` | Pull by name |
| `GET` | `/api/v1/search?q={query}` | Search by name |
| `GET` | `/api/v1/search?type={sig}` | Search by type signature |
| `GET` | `/api/v1/search?tag={tag}` | Search by tag |
| `GET` | `/api/v1/stats` | Registry statistics |
| `GET` | `/api/v1/deps/{hash}` | Transitive dependencies |
| `GET` | `/health` | Health check |

### PublishedDefinition (request/response)

```json
{
  "hash":               "64-char hex content hash",
  "source":             "fn source code",
  "module":             "module.path",
  "is_pub":             true,
  "params":             [{"name": "x", "type": "Field"}],
  "return_ty":          "Field",
  "dependencies":       ["<hex hash>"],
  "requires":           ["precondition expression"],
  "ensures":            ["postcondition expression"],
  "name":               "my_function",
  "tags":               ["crypto", "hash"],
  "verified":           false,
  "verification_cert":  null
}
```

### PublishResult

```json
{ "hash": "64-char hex", "created": true, "name_bound": true }
```

### SearchResult (in `results` array)

```json
{ "name": "fn_name", "hash": "64-char hex", "module": "mod.path",
  "signature": "(Field, Field) -> Field", "verified": true, "tags": ["crypto"] }
```

### PullResult

```json
{ "hash": "64-char hex", "source": "fn source", "module": "mod.path",
  "params": [{"name": "x", "type": "Field"}], "return_ty": "Field",
  "dependencies": ["<hex>"], "requires": ["pre"], "ensures": ["post"] }
```

### Environment

| Variable | Description |
|----------|-------------|
| `TRIDENT_REGISTRY_URL` | Default Atlas server URL (fallback: `http://127.0.0.1:8090`) |

---

## 9. OS Configuration

Each OS that supports Atlas declares a `[registry]` section in `target.toml`:

```toml
[registry]
collection_id = "0x..."
mint_authority = "governance"
resolution = "on-chain"
cache_dir = "~/.trident/cache"
```

| Field | Type | Description |
|-------|------|-------------|
| `collection_id` | String | TSP-2 collection address on this OS |
| `mint_authority` | String | Who can publish: `governance`, `open`, `allowlist` |
| `resolution` | String | Resolution mode: `on-chain`, `http`, `local-only` |
| `cache_dir` | String | Local cache directory for fetched artifacts |

| Mint Authority | Description |
|----------------|-------------|
| `governance` | On-chain governance approval required to publish |
| `open` | Anyone can publish (permissionless) |
| `allowlist` | Only approved accounts can publish |

---

## 10. Security Model

| Property | Mechanism |
|----------|-----------|
| Content integrity | Artifact identity = content hash (blake3); tampering produces a different hash |
| Mint authority | Per-OS governance controls who can publish new packages |
| Owner auth | Only the Card owner can update versions (TSP-2 auth_hash) |
| Creator immutability | Original publisher permanently recorded in `creator_id` (TSP-2 invariant) |
| Verification certificates | STARK proof that compilation was correct, stored in package metadata |
| Reproducible builds | Same source + same compiler version = same content hash = same Card |
| Name uniqueness | `asset_id = hash(name)` with non-membership proof at mint; one Card per name |
| Version ordering | Monotonic `nonce` provides total order over version history |

---

## 11. See Also

- [Atlas: Why On-Chain Package Management](../docs/explanation/atlas.md) — design rationale
- [PLUMB Framework](plumb.md) — shared token framework
- [TSP-2 — Card Standard](tsp2-card.md) — the underlying asset standard
- [OS Reference](os.md) — OS model and registry bindings
- [CLI Reference](cli.md) — full command reference
- [Standard Library](stdlib.md) — `std.*` modules
