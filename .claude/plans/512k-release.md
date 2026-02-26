# 512K Release Plan

## Context

First public release of Trident. Kelvin versioning: 512K = hot,
experimental, developer preview. "Everything lies" — not production
ready. The goal is to get it on GitHub as a tagged release and on
crates.io so people can `cargo install trident-lang`.

## Current State

- **Package:** `trident-lang` v0.1.0
- **Remote:** `cyberia-to/trident` (public GitHub)
- **Cargo.toml `repository`:** points to `mastercyb/trident` (WRONG — must fix)
- **License:** Cyber License (custom, non-SPDX) via `license-file = "LICENSE.md"`
- **Tests:** 934 passing (912 lib + 22 integration), 0 failing
- **`cargo publish --dry-run`:** succeeds (520 files, 12.6 MiB / 10.0 MiB compressed)
- **CI:** none
- **Tags:** none
- **Binaries:** `trident` (CLI) + `trident-lsp` (language server)

## Crates.io: Yes, with caveats

crates.io accepts custom licenses via `license-file`. The dry run
passes. Publishing gives `cargo install trident-lang` which builds
both binaries.

**Problem:** 12.6 MiB is large but under crates.io's 10 MiB compressed
limit. The `burn` + `wgpu` dependencies make the binary ~43 MB. This
is the neural optimizer (burn uses wgpu as its GPU training backend).
GPU proving lives in trisha, not trident. For 512K this is fine —
users building from source expect heavy deps.

**Problem:** `repository` URL in Cargo.toml points to `mastercyb/trident`
but the actual remote is `cyberia-to/trident`. Must fix before publish.

## Pre-Release Cleanup

### 1. Fix Cargo.toml metadata
- `repository` → `https://github.com/cyberia-to/trident`
- `homepage` → `https://github.com/cyberia-to/trident`
- Consider adding `rust-version = "1.75"` (or whatever MSRV is)

### 2. Version: keep 0.1.0 or change?
Kelvin says 512K but Cargo uses semver. Options:
- **Keep `0.1.0`** — standard Rust convention for first experimental release
- Semver and Kelvin are orthogonal: semver for Cargo, Kelvin for the
  project. The GitHub release/tag carries the Kelvin label.

Recommendation: keep `0.1.0`, tag as `v0.1.0`, GitHub release title
"512K — Developer Preview".

### 3. Release notes
`CHANGELOG.md` in repo root (Rust convention). Extract 512K section
for `gh release create --notes-file`. Content:

```
# 512K — Developer Preview

Trident is a provable programming language. This is the first public
release. It is hot, experimental, and not production ready.

## What ships

- Full compiler: source → TASM (Triton VM assembly)
  - Lexer, parser, type checker, TIR, optimizer, lowering, linker
  - 912 tests passing
- CLI: build, check, test, fmt, audit, bench, lsp, package, deploy
- Standard library: std.crypto (poseidon2, sha256, ecdsa, merkle, bigint),
  std.nn (tensor, dense, attention), std.private (poly, ntt),
  std.quantum (gates)
- Self-hosted compiler stages: lexer, parser, typechecker, codegen,
  optimizer, lowering, pipeline (9,195 lines of Trident)
- Language Server Protocol (trident-lsp)
- Editor support: Zed extension, Helix config
- Formal verification: trident audit with #[requires]/#[ensures]
- Neural optimizer: 13M-parameter GNN+Transformer (training)
- Neptune token standards: Coin (TSP-1), Card (TSP-2), lock/type scripts

## Install

    cargo install trident-lang

## What's next (256K)

Self-hosting, on-chain registry (Atlas), revolution demos
(proven inference, FHE circuits, quantum simulation).

## License

Cyber License: Don't trust. Don't fear. Don't beg.
```

### 4. Tag and GitHub release
```
git tag -a v0.1.0 -m "512K — Developer Preview"
git push origin master --tags
gh release create v0.1.0 --title "512K — Developer Preview" \
  --notes-file CHANGELOG.md --prerelease
```

Mark as **pre-release** on GitHub. This signals "not production ready."

### 5. Publish to crates.io
```
cargo publish
```

After this, `cargo install trident-lang` works globally.

### 6. Optional: binary releases via GitHub
For users who don't want to compile 743 dependencies:
- Add `.github/workflows/release.yml` that builds on
  `ubuntu-latest` + `macos-latest` + `macos-latest` (arm64)
  on tag push, uploads binaries to the GitHub release
- This is nice-to-have for 512K, not blocking

## Files Modified

1. `Cargo.toml` — fix `repository` and `homepage` URLs
2. `CHANGELOG.md` (new) — release notes, grows with each Kelvin milestone
3. `reference/roadmap.md` — mark 512K as released with date

## Execution Order

1. Fix Cargo.toml URLs
2. Write CHANGELOG.md
3. Update roadmap.md with release date
4. Commit: `chore: prepare 512K release`
5. `cargo test` — verify clean
6. `cargo publish --dry-run` — verify clean
7. `git tag -a v0.1.0 -m "512K — Developer Preview"`
8. `git push origin master --tags`
9. `gh release create v0.1.0 --prerelease --title "512K — Developer Preview" --notes-file CHANGELOG.md`
10. `cargo publish`

## Verification

- `cargo install trident-lang` from a clean machine/directory
- `trident --version` prints `trident 0.1.0`
- `trident build` works on a hello.tri
- GitHub release page shows pre-release badge
- crates.io page shows correct metadata and README
