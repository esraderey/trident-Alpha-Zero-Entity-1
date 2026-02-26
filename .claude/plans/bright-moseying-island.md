# README Narrowing

## Context

The README currently has 7 major sections before Quick Start, with
significant overlap. The thesis section re-explains what "One Field"
already covers. The Rosetta Stone and Trinity bench results are
separated from the thesis they demonstrate. "Why a New Language" is
structurally about "write once, prove anywhere" but the title doesn't
say that. "Self-Hosting" is about trustless compilation but the
connection to "don't trust — verify" isn't sharp.

The user wants three focused narrative sections before the operational
content (apps, quick start, source tree, etc.).

## Proposed Structure

```
# Trident
  [epigraph + gif + one-paragraph intro — keep as-is]

## One Field. Three Revolutions.
  - Merge current "One Field" intro + "The Thesis" + "The Rosetta Stone"
    into one flowing section
  - End with Trinity bench results as concrete proof that all three
    revolutions run in one STARK trace
  - Link to trinity-bench.md for the full 7-phase walkthrough

## Write Once, Prove Anywhere.
  - Rename "Why a New Language" → "Write Once, Prove Anywhere"
  - Lead with: the language exists because provable VMs need native
    field arithmetic — you write Trident once, it compiles to any
    provable target
  - Keep the 4 structural facts (arithmetic circuits, proof composition,
    bounded execution, field type system) — they explain WHY a new
    language is needed for write-once-prove-anywhere
  - Keep "What follows" subsection (hash perf, formal verification,
    content-addressed code)
  - Multi-target paragraph: Triton VM today, quantum/ML/ZK/classical
    backends as they ship

## Self-Hosting: Proving Compilation
  - Sharpen around "don't trust — verify"
  - Lead with the thesis: every compiler is a trusted third party;
    self-hosting on a provable VM eliminates that trust
  - Lexer results as concrete proof
  - The trajectory: src/ shrinks, std/compiler/ grows
  - End: proof certificate accompanies every build — no trusted
    compiler, no trusted server, you verify

[Then the rest unchanged: Apps, Quick Start, Source Tree, Std Library
Vision, Documentation, Design Principles, Editor Support, License]
```

## Files Modified

- `README.md` — the only file

## What Gets Cut

- "The Thesis" standalone section → merged into "One Field"
- "The Rosetta Stone" standalone section → merged into "One Field"
- "Why a New Language" title → renamed
- Redundant quantum-acceleration paragraph at end of Self-Hosting
  (already covered in thesis)

## Verification

- Read the result, check no broken links
- Ensure all internal doc links still resolve (`docs/explanation/*.md`)
