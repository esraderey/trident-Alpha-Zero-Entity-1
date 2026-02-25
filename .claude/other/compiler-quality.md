# Compiler TASM Quality — 2026-02-16

1.07x average across 13 modules (misleading — dominated by modules
where function-level counting favors compiler). Median per-function
overhead: ~1.50x, worst: 9.50x.

## Three Root Causes (account for ~95% of overhead)

**A. Digest/Struct Copy (60-70%)**: Compiler copies entire multi-element
values (Digest=5, State8=8) via dup chains before calls. Hand TASM
operates in-place. Example: authenticate_field 19 ops vs 2 hand.

**B. Multi-Return Cleanup (20-25%)**: swap/pop chains or RAM scratch
to remove dead locals below multi-element return values.

**C. State Reconstruction (10-15%)**: Modifying one field of a struct
rebuilds the entire value.

## High-Impact Fixes

- **H1**: Extend detect_pass_through() for width > 1 params
- **H2**: In-place struct element modification
- **H3**: Argument consumption analysis (skip dup for consumed args)

## Width Correlation

- Width-1 values: ~5% overhead (near-optimal)
- Digest (width-5): 60-80% overhead
- State8 (width-8): 50-85% overhead

Grade: B- (early production). H1-H3 would bring most functions to <1.5x.
