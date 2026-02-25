# Real-World Performance — 2026-02-16

Absolute wall-clock estimates for workloads on Triton VM.

## Proving Cost Model

Padded height = next_power_of_2(max table height across 6 AETs).
Formula: `padded_height * 300 * log2(ph) * 3ns` (optimistic lower bound).
Real-world: 2-5x formula. GPU: 3-10x speedup over CPU.

| Padded Height | CPU Estimate | RAM     |
|---------------|-------------|---------|
| 2^16          | 1-3s        | ~6 GB   |
| 2^20          | 30-90s      | ~100 GB |
| 2^22          | 3-8 min     | ~800 GB |
| 2^24          | 15-40 min   | ~3 TB+  |

## Feasibility Summary

| Workload                     | Cycles | Padded | GPU Time  | Feasible |
|------------------------------|--------|--------|-----------|----------|
| Hash/Merkle proof            | <100K  | 2^17   | <1s       | Yes      |
| Token transfer               | <500K  | 2^19   | <5s       | Yes      |
| Quantum 2-5 qubits           | <20K   | 2^14   | <1s       | Yes      |
| RLWE ciphertext add          | <50K   | 2^15   | <1s       | Yes      |
| MNIST MLP (784-128-10)       | ~2.2M  | 2^22   | 30-90s    | GPU only |
| Quantum 10-12 qubits         | ~500K  | 2^19   | 5-15s     | Yes      |
| Quantum 14-16 qubits         | ~30M   | 2^25   | 30-90 min | Marginal |
| FHE multiply N=1024 (opt NTT)| ~1.3M  | 2^21   | 10-30s    | GPU only |
| FHE multiply N=1024 (current)| ~567M  | 2^30   | Hours     | No       |
| Transformer attention head   | ~12M   | 2^24   | 2-6 min   | Marginal |

## Key Bottleneck

poly.tri NTT wastes ~500x cycles from bounded loop overhead. Fix:
restructure to direct butterfly addressing. This alone makes small FHE
marginally feasible.

## Competitive Position

- 100-200x behind SOTA for AI inference (zkPyTorch, EZKL)
- Sweet spot: hash-heavy crypto, small provable computations, recursive proofs
- Novel niche: verifiable quantum simulation (2-12 qubits)
- Dual-target advantage (CUDA + zkVM from same source) is unique
