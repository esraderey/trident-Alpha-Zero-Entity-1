# Real-World Performance Analysis: Trident on Triton VM

Absolute wall-clock estimates for AI inference, quantum simulation, and FHE
workloads compiled from Trident and proved on Triton VM.

Date: 2026-02-16

---

## 1. Triton VM Proving Cost Model

### 1.1 Architecture

Triton VM uses 6 Algebraic Execution Tables (AETs). The proving cost is
determined by the **tallest table**, padded to the next power of 2:

| Table      | What fills it                              |
|------------|--------------------------------------------|
| Processor  | Every instruction (1 row per cycle)        |
| Hash       | Tip5 permutations (6 rows each)            |
| U32        | 32-bit operations (33 rows each)           |
| OpStack    | Stack reads/writes (1 row per access)      |
| RAM        | Random-access memory ops (1 row per r/w)   |
| JumpStack  | Calls/returns (2 rows per call)            |

The padded height = next_power_of_2(max(all table heights)).
Doubling the padded height roughly doubles the proving cost.

### 1.2 Per-Instruction Costs (from Trident cost model)

| Operation              | Processor | Hash | U32 | OpStack | RAM | Jump |
|------------------------|-----------|------|-----|---------|-----|------|
| field add/mul          | 1         | 0    | 0   | 1       | 0   | 0    |
| push/dup (literal/var) | 1         | 0    | 0   | 1       | 0   | 0    |
| mem.read / mem.write   | 2         | 0    | 0   | 2       | 1   | 0    |
| split (field -> u32s)  | 1         | 0    | 33  | 1       | 0   | 0    |
| U32 compare (<)        | 1         | 0    | 33  | 1       | 0   | 0    |
| hash                   | 1         | 6    | 0   | 1       | 0   | 0    |
| fn call overhead       | 2         | 0    | 0   | 0       | 0   | 2    |
| loop iteration ovhd    | 8         | 0    | 0   | 4       | 0   | 1    |
| if/else overhead       | 3         | 0    | 0   | 2       | 0   | 1    |

### 1.3 Trident's Internal Proving Time Estimate

The codebase uses this formula (src/cost/analyzer.rs):

```
estimated_proving_secs = padded_height * 300 * log2(padded_height) * 3e-9
```

This models the FRI polynomial commitment: the prover must evaluate 300
trace columns at `padded_height * log2(padded_height)` points, with each
field operation taking ~3ns on modern hardware.

| Padded Height | Rows      | Formula Result | Notes                       |
|---------------|-----------|----------------|-----------------------------|
| 2^16          | 65,536    | ~0.9s          | Tiny programs               |
| 2^18          | 262,144   | ~4.3s          | Small programs               |
| 2^20          | 1,048,576 | ~19s           | Medium programs              |
| 2^21          | 2,097,152 | ~40s           | Neptune verifier target      |
| 2^22          | 4,194,304 | ~84s           | Large programs               |
| 2^24          | 16,777,216| ~370s (~6 min) | Very large programs          |
| 2^26          | 67,108,864| ~1,600s (~27m) | Extreme                      |
| 2^28          | 268M      | ~6,800s (~1.9h)| Impractical today            |
| 2^30          | 1B        | ~29,000s (~8h) | Theoretical only             |

### 1.4 Real-World Calibration

The formula above is an optimistic lower bound. Real-world data from Neptune:

- **Padded height 2^21**: ~400 GB RAM required for the prover
- **Padded height 2^22**: ~800 GB RAM required
- **Block proofs**: "over a minute per coinbase transaction"
- **GPU-accelerated prover** (2025): reduces "tens of seconds" to "a few
  seconds" for moderate programs
- **Recursive verification**: 1.6M clock cycles for the smallest program,
  targeting padded height 2^21

Conservative real-world estimates (CPU, M2-class hardware, 2025):

| Padded Height | Estimated Wall-Clock | RAM Required |
|---------------|----------------------|--------------|
| 2^16          | 1-3 seconds          | ~6 GB        |
| 2^18          | 5-15 seconds         | ~25 GB       |
| 2^20          | 30-90 seconds        | ~100 GB      |
| 2^21          | 1-3 minutes          | ~400 GB      |
| 2^22          | 3-8 minutes          | ~800 GB      |
| 2^24          | 15-40 minutes        | ~3 TB+       |

With GPU acceleration (RTX 4090 class), these drop by roughly 3-10x.

### 1.5 The Dominant Table Problem

The padded height is determined by the **tallest** table, not the sum.
A program that does 1,000 field multiplications (1,000 processor rows)
but also 100 hash operations generates 600 hash table rows. If the hash
table is tallest, the padded height jumps to 1024 even though the program
is "small".

For Trident's computational workloads, the dominant tables are typically:
- **Processor + OpStack**: for compute-heavy code (matmul, loops)
- **RAM**: for memory-heavy code (large arrays, NTT)
- **U32**: for comparison-heavy code (ReLU, conditionals)

---

## 2. AI Inference: MNIST MLP (784-128-10)

### 2.1 Architecture

```
Layer 1: matvec(784x128) + bias_add(128) + relu_layer(128)
Layer 2: matvec(128x10)  + bias_add(10)
```

### 2.2 Cycle-by-Cycle Analysis

**Layer 1: matvec(784x128)**

The `matvec` function in tensor.tri has this inner structure per output element:
```
for i in 0..m (128 iterations):        // outer loop
  for j in 0..n (784 iterations):      // inner loop
    idx computations (as_field)         // 1 convert + 1 mul + 1 add = ~5 ops
    mem.read(mat_addr + offset)         // 2 proc + 1 ram
    mem.read(vec_addr + col)            // 2 proc + 1 ram
    sum = sum + mat_val * vec_val       // 2 proc (mul + add)
  mem.write(out_addr + i, sum)          // 2 proc + 1 ram
```

Per inner iteration (j loop body):
- `convert.as_field(j)`: 0 cost (as_field is free in Triton)
- `mat_addr + row_offset + col`: 2 field adds = 2 proc + 2 opstack
- `mem.read(mat_addr + ...)`: 2 proc + 2 opstack + 1 ram
- `mem.read(vec_addr + col)`: 2 proc + 2 opstack + 1 ram
- `mat_val * vec_val`: 1 proc + 1 opstack
- `sum + product`: 1 proc + 1 opstack
- Various stack ops (dup, swap): ~4 proc + 4 opstack
- Loop overhead: 8 proc + 4 opstack + 1 jump

**Inner loop body total: ~22 processor, ~16 opstack, ~2 ram per iteration**

Per outer iteration (i loop body):
- Inner loop: 784 iterations * costs above
- convert.as_field(i), address calc, mem.write: ~10 proc, ~6 opstack, 1 ram
- Loop overhead: 8 proc + 4 opstack + 1 jump

Layer 1 matvec costs:

| Table     | Per inner iter | x784 iters | Per outer iter | x128 iters | Total     |
|-----------|----------------|------------|----------------|------------|-----------|
| Processor | 22             | 17,248     | 17,266         | 128        | 2,210,048 |
| OpStack   | 16             | 12,544     | 12,554         | 128        | 1,606,912 |
| RAM       | 2              | 1,568      | 1,569          | 128        | 200,832   |
| Jump      | 1              | 784        | 785            | 128        | 100,480   |

**Layer 1: bias_add(128)**

Per iteration: 2 mem.read + 1 add + 1 mem.write + overhead
- ~12 proc + ~10 opstack + 3 ram per iteration
- 128 iterations

| Table     | Total   |
|-----------|---------|
| Processor | 1,536   |
| OpStack   | 1,280   |
| RAM       | 384     |

**Layer 1: relu_layer(128)**

Per iteration: 1 mem.read + relu() + 1 mem.write + overhead
relu() internally: half_p() [field.neg + field.inv + mul = ~5 ops] +
  convert.split(x) [1 proc + 33 u32] + convert.split(threshold) [1 proc + 33 u32] +
  if comparison [1 proc + 33 u32 + if_overhead]

Per iteration: ~25 proc + ~15 opstack + 2 ram + ~99 u32 rows
128 iterations:

| Table     | Total   |
|-----------|---------|
| Processor | 3,200   |
| OpStack   | 1,920   |
| RAM       | 256     |
| U32       | 12,672  |

**Layer 2: matvec(128x10)**

Same structure as Layer 1 but with 10 rows x 128 columns:
- Inner loop: 128 iterations, outer loop: 10 iterations
- Total: 10 * (128 * 22 + 18) = 28,340 processor cycles (approx)

| Table     | Total   |
|-----------|---------|
| Processor | 28,340  |
| OpStack   | 20,640  |
| RAM       | 2,570   |

**Layer 2: bias_add(10)**

| Table     | Total |
|-----------|-------|
| Processor | 120   |
| OpStack   | 100   |
| RAM       | 30    |

### 2.3 MNIST MLP Total

| Table     | Layer 1 matvec | Layer 1 bias | Layer 1 relu | Layer 2 matvec | Layer 2 bias | **TOTAL**     |
|-----------|----------------|--------------|--------------|----------------|--------------|---------------|
| Processor | 2,210,048      | 1,536        | 3,200        | 28,340         | 120          | **2,243,244** |
| OpStack   | 1,606,912      | 1,280        | 1,920        | 20,640         | 100          | **1,630,852** |
| RAM       | 200,832        | 384          | 256          | 2,570          | 30           | **204,072**   |
| U32       | 0              | 0            | 12,672       | 0              | 0            | **12,672**    |
| Jump      | 100,480        | 0            | 0            | 0              | 0            | **100,480**   |

**Dominant table: Processor at ~2.24M rows**
**Padded height: 2^22 = 4,194,304** (next power of 2 above 2,243,244)

Note: The actual dominant table may be OpStack, since each processor
instruction also generates opstack rows. The padded height is still 2^22.

Plus program attestation: ~2.24M / 10 * 6 = ~1.34M hash rows.
This could push hash table to ~1.34M, still below processor.

### 2.4 MNIST Proving Time Estimate

- **Padded height**: 2^22 (4,194,304)
- **Trident formula**: 4,194,304 * 300 * 22 * 3e-9 = ~83 seconds
- **Real-world CPU estimate**: 3-8 minutes
- **Real-world GPU estimate**: 30-90 seconds
- **RAM required**: ~800 GB (CPU), less with GPU-optimized prover

### 2.5 Verdict: MNIST MLP

A simple MNIST classifier (784-128-10, ~100K parameters) takes
**3-8 minutes to prove on CPU** with ~800 GB RAM, or
**30-90 seconds with GPU acceleration**.

This is technically feasible on a high-memory server. It is not feasible
on consumer hardware (the RAM requirement alone exceeds most machines).

For comparison, zkPyTorch (Polyhedra, 2025) proves VGG-16 (15M parameters)
inference in **2.2 seconds** using a purpose-built circuit compiler with
custom lookup tables. Trident on Triton VM is roughly **100-200x slower**
for equivalent workloads because:

1. Triton VM is a general-purpose zkVM, not optimized for ML
2. No lookup tables for nonlinear operations
3. The 300-column trace is wide (high per-row proving cost)
4. ReLU requires U32 splitting which is expensive (33 u32 rows each)

### 2.6 Transformer Attention Head (64x64)

A single attention head with d=64:
- Q*K^T: matmul(64, 64, 64) = 64 * 64 * (64 * ~22) = ~5.8M processor cycles
- Softmax approximation: no native exp/div, requires polynomial approx
  per element. For 64x64=4096 elements, ~50 ops each = ~200K cycles
- Attention * V: matmul(64, 64, 64) = ~5.8M processor cycles
- **Total: ~12M processor cycles minimum**
- **Padded height: 2^24 = 16,777,216**
- **Proving time: 15-40 minutes CPU, 2-6 minutes GPU**
- **RAM: 3+ TB CPU**

A single transformer layer with multi-head attention (8 heads, d_model=512):
- 8 attention heads: ~96M cycles
- Feed-forward (512x2048 + 2048x512): ~44M cycles
- **Total: ~140M cycles, padded height 2^28**
- **Proving time: ~2 hours CPU. Impractical.**

### 2.7 Maximum Feasible AI Model

| Model                        | Parameters | Est. Cycles | Padded Height | Proving Time (GPU) | Feasible? |
|------------------------------|------------|-------------|---------------|--------------------|-----------| 
| MNIST MLP (784-128-10)       | 100K       | ~2.2M       | 2^22          | 30-90s             | Yes       |
| Small CNN (MNIST)            | 200K       | ~5M         | 2^23          | 1-3 min            | Yes       |
| 1 transformer attention head | -          | ~12M        | 2^24          | 2-6 min            | Marginal  |
| ResNet-18 (one image)        | 11M        | ~500M       | 2^30          | Hours              | No        |
| GPT-2 (one token)            | 117M       | ~10B        | 2^34          | Days               | No        |

**Practical limit on Triton VM today: models with <5M cycles, which means
networks with ~50K-200K total multiply-accumulate operations.**

---

## 3. Quantum Simulation

### 3.1 State Representation

For n qubits, the state vector has 2^n complex amplitudes.
Each complex number = 2 field elements (re, im).
Total state: 2^(n+1) field elements stored in RAM.

| Qubits | Amplitudes | Field Elements | RAM Words |
|--------|------------|----------------|-----------|
| 2      | 4          | 8              | 8         |
| 3      | 8          | 16             | 16        |
| 5      | 32         | 64             | 64        |
| 8      | 256        | 512            | 512       |
| 10     | 1,024      | 2,048          | 2,048     |
| 16     | 65,536     | 131,072        | 131,072   |
| 20     | 1,048,576  | 2,097,152      | 2,097,152 |

### 3.2 Single-Qubit Gate Cost (apply_single_gate)

The `apply_single_gate` function in gates.tri:

```
for i in 0..num_states (2^n iterations, but only half are processed):
  - bit check: as_u32 + bitwise AND + comparison = ~35 u32 rows + ~5 proc
  - if target bit is 0 (half the time):
    - 4 mem.read: 8 proc + 4 ram
    - 2 complex_mul + 2 complex_mul + 2 complex_add:
      complex_mul = 4 mul + 1 neg + 1 add = ~8 proc each
      complex_add = 2 add = ~4 proc each
      Total: 4 * 8 + 2 * 4 = 40 proc
    - 4 mem.write: 8 proc + 4 ram
    - Struct construction: ~10 proc
  Loop overhead: 8 proc + 4 opstack + 1 jump
```

Per iteration (all 2^n of them):
- Bit check always happens: ~5 proc + ~35 u32 + ~3 opstack
- Half the iterations also do the gate work: ~66 proc + 8 ram average
- Loop overhead: 8 proc per iteration

**Per iteration average: ~46 proc + ~35 u32 + ~7 opstack + ~4 ram**

For 2^n iterations total, single-qubit gate cost:
- Processor: ~46 * 2^n
- U32: ~35 * 2^n
- RAM: ~4 * 2^n

### 3.3 Two-Qubit Gate Cost (product state approach)

For the inline 2-qubit operations (cnot, cz, swap on TwoQubit struct):
- These are simple field swaps/negations: ~5-10 processor cycles each
- But they only work on the explicit TwoQubit struct, not RAM-based state

For RAM-based multi-qubit gates, you would need `apply_single_gate` twice
(once for each qubit involved), plus additional logic for entanglement.
A proper CNOT on n-qubit state requires iterating over all 2^n amplitudes
with conditional logic: ~100 proc per iteration.

### 3.4 Concrete Examples

**2-Qubit Bell State: H(q0), CNOT(q0,q1), measure both**

Using the struct-based approach (not RAM-based):
- init_zero(): 2 struct inits = ~10 proc
- hadamard(q0): 2 complex_add + 2 complex_sub = ~16 proc
- two_qubit_product(): 4 complex_mul = ~32 proc
- cnot(): 2 field copies = ~4 proc
- measure_deterministic() x2: norm_sq + comparison = ~20 proc each
- Function call overheads: ~20 proc
- **Total: ~122 processor cycles**
- **Padded height: 256 (2^8)**
- **Proving time: <1 second**

This is trivial. The struct-based approach avoids RAM entirely for 2 qubits.

**5-Qubit GHZ State Preparation**

H(q0), CNOT(q0,q1), CNOT(q0,q2), CNOT(q0,q3), CNOT(q0,q4)

Must use RAM-based state (2^5 = 32 amplitudes = 64 field elements).

- Initialize state: 64 mem.write = ~128 proc + 64 ram
- H on qubit 0 (apply_single_gate): 32 iterations * ~46 proc = ~1,472 proc
  + 32 * 35 = 1,120 u32 rows + 32 * 4 = 128 ram
- CNOT on qubits 0,1 (needs custom implementation, ~100 proc per iteration):
  32 iterations * ~100 proc = ~3,200 proc + ~128 ram
- CNOT on qubits 0,2: ~3,200 proc + ~128 ram
- CNOT on qubits 0,3: ~3,200 proc + ~128 ram
- CNOT on qubits 0,4: ~3,200 proc + ~128 ram
- Function call overhead: ~50 proc

| Table     | Total    |
|-----------|----------|
| Processor | ~14,450  |
| OpStack   | ~8,000   |
| U32       | ~1,120   |
| RAM       | ~704     |

- **Padded height: 2^14 = 16,384**
- **Proving time: <1 second**

This is still very fast. 5 qubits is a tiny state space.

**Grover's Algorithm on 3 Qubits (1 oracle query)**

Steps: H^3, Oracle, H^3, Phase inversion, H^3
- State size: 2^3 = 8 amplitudes = 16 field elements
- Each H gate: 8 iterations * ~46 proc = ~368 proc
- 3 H gates per layer, 3 layers = 9 H gates = ~3,312 proc
- Oracle (marks one state): ~50 proc
- Phase inversion (conditional phase flip on all states): ~400 proc
- **Total: ~4,000 processor cycles**
- **Padded height: 2^12 = 4,096**
- **Proving time: <1 second**

### 3.5 Scaling Analysis

| Qubits | State Size | Single Gate Cycles | 10-Gate Circuit | Padded Height | Proving Time |
|--------|------------|--------------------|-----------------|----|---|
| 2      | 4          | ~190               | ~1,900          | 2^11    | <1s       |
| 5      | 32         | ~1,500             | ~15,000         | 2^14    | <1s       |
| 8      | 256        | ~12,000            | ~120,000        | 2^17    | 2-5s      |
| 10     | 1,024      | ~47,000            | ~470,000        | 2^19    | 10-30s    |
| 12     | 4,096      | ~190,000           | ~1,900,000      | 2^21    | 1-3 min   |
| 14     | 16,384     | ~750,000           | ~7,500,000      | 2^23    | 5-15 min  |
| 16     | 65,536     | ~3,000,000         | ~30,000,000     | 2^25    | 30-90 min |
| 20     | 1,048,576  | ~48,000,000        | ~480,000,000    | 2^29    | 3-10 hrs  |

Each additional qubit doubles the state size and approximately doubles
the cost of every gate. The circuit depth (number of gates) multiplies
linearly. For a circuit with G gates on N qubits:

**Total cycles ~ G * 46 * 2^N**

### 3.6 Verdict: Quantum Simulation

**Practical range: up to ~12-14 qubits with simple circuits (<100 gates)**

- 2-8 qubits: trivially fast, sub-second proving. Good for education,
  protocol verification, and proving correct execution of small quantum
  circuits.
- 10-12 qubits: feasible (minutes), useful for demonstrating specific
  quantum algorithms.
- 14-16 qubits: marginal (30 min - 1.5 hours), requires significant
  hardware.
- 20+ qubits: impractical on current hardware.

**This is a toy for demonstrations, not a real quantum simulator.** Classical
quantum simulators handle 30-40 qubits in seconds on CPUs. The value
proposition is not simulation speed but **provable correctness**: you can
prove that a quantum circuit was simulated correctly, which has applications
in verifiable quantum computing delegation.

There is no known precedent for "verifiable quantum simulation via zkVM"
in the literature. This would be a novel application area, but limited to
small qubit counts.

---

## 4. Fully Homomorphic Encryption (FHE)

### 4.1 Building Blocks

Trident's FHE support is based on polynomial arithmetic in `std.private.poly`:
- `add`: coefficient-wise addition
- `sub`: coefficient-wise subtraction
- `pointwise_mul`: coefficient-wise multiplication (NTT domain)
- `ntt`: Number Theoretic Transform (Cooley-Tukey butterflies)
- `intt`: Inverse NTT
- `poly_mul`: full polynomial multiplication via NTT

### 4.2 Per-Operation Cost Analysis

**Polynomial addition (N=1024 coefficients)**

```
for i in 0..1024:
  as_field(i): free
  mem.read(a_addr + idx): 2 proc + 1 ram
  mem.read(b_addr + idx): 2 proc + 1 ram
  field.neg(b_val): 2 proc (for sub) or a+b: 1 proc (for add)
  mem.write(out_addr + idx, result): 2 proc + 1 ram
  loop overhead: 8 proc + 1 jump
```

Per iteration: ~15 proc + ~10 opstack + 3 ram + 1 jump
Total for N=1024: **~15,360 proc + ~10,240 opstack + ~3,072 ram**

**Pointwise multiplication (N=1024)**

Same structure as add but with a multiply instead of add:
Per iteration: ~15 proc + ~10 opstack + 3 ram + 1 jump
Total for N=1024: **~15,360 proc + ~10,240 opstack + ~3,072 ram**

**NTT (N=1024, log_n=10)**

The NTT has log_n = 10 butterfly stages. At each stage, there are N/2
butterfly operations. Each butterfly:

```
mem.read(idx_lo): 2 proc + 1 ram
mem.read(idx_hi): 2 proc + 1 ram
hi * w (twiddle multiply): 1 proc
lo + t: 1 proc
lo + neg(t) (= lo - t): 2 proc (neg) + 1 proc (add)
mem.write(idx_lo, ...): 2 proc + 1 ram
mem.write(idx_hi, ...): 2 proc + 1 ram
```

But the actual Trident code has a nested loop structure that is less
efficient than optimal. The outer loop iterates over `j in 0..half`,
and the inner loop iterates over groups (`g in 0..size`). The inner loop
bound is `size` (=1024) but only processes valid indices, meaning many
iterations are wasted or exit early.

**Conservative NTT cost estimate:**

Per butterfly stage: N/2 actual butterflies * ~20 proc each + overhead
- 10 stages * 512 butterflies * ~20 proc = ~102,400 proc
- Plus twiddle factor computation: ~5 proc per butterfly = ~25,600 proc
- Plus loop overhead: 10 stages * (inner loops + outer loops)
  Outer: N/2 iterations, inner: up to N iterations
  Worst case: 10 * 1024 * 1024 iterations of the inner loop (due to
  `bounded 4096` on size, iterating over all groups)

**Critical issue**: The NTT implementation in poly.tri has the inner loop
bounded by `size` (N=1024), but for each j, it steps by `group_size` through
the array. The number of actual groups per j per stage varies. In the worst
case analysis, the total inner loop iterations across all stages:

Stage 0 (len=1): j in 0..1, k steps by 2 through 1024 = 512 groups = 512 iter
Stage 1 (len=2): j in 0..2, k steps by 4 through 1024 = 256 groups per j = 512 iter
...
Stage 9 (len=512): j in 0..512, k steps by 1024 through 1024 = 1 group per j = 512 iter

Each stage: 512 actual butterflies. But the code loops `for g in 0..size`
(1024 iterations) even though only some produce valid indices. The `bounded 4096`
annotation means the compiler assumes up to 4096 iterations.

**Actual NTT cost (accounting for loop structure):**

Per stage, per j value, the inner `g` loop runs `size` (=1024) times but
only executes the butterfly for valid k values. Since this is a zkVM with
no early-exit from bounded loops, ALL iterations are paid for.

This is a major inefficiency. The actual loop iterations:
- Each stage: `half * size` inner iterations = (N/2) * N at first stage,
  but half varies. Summing across stages:
  Stage s: half = 2^s, so iterations = 2^s * 1024

Wait -- re-reading the code more carefully: the inner loop `for g in 0..size`
increments k by group_size each time. But in a zkVM, the bounded loop runs
the full bound regardless. With `bounded 4096`, each (stage, j) pair runs
4096 inner iterations. This is extremely wasteful.

**Revised NTT cost with loop waste:**

For N=1024 (log_n=10):
- 10 stages
- Stage s: half = 2^s iterations of j, each with up to 4096 g-iterations
  (but only N/(2*half) = N/2^(s+1) valid)
- Total j-iterations across all stages: sum(2^s for s=0..9) = 1023
- Each j runs 4096 g-iterations (bounded)
- Each g-iteration: ~15 proc (address calc + condition check + maybe butterfly)
- Total: 1023 * 4096 * ~15 = **~62.8M processor cycles just for the loop overhead**

This is a devastating result. The NTT implementation's nested loop structure,
combined with zkVM's inability to early-exit from bounded loops, makes it
~60x more expensive than necessary.

With a properly structured NTT (butterfly-addressed, no wasted iterations):
- 10 stages * 512 butterflies * ~25 proc = ~128,000 proc
- An optimal NTT for N=1024 would cost ~130K cycles

The current implementation costs ~63M cycles. This is the single biggest
performance problem in the FHE module.

**For the analysis below, I'll use both numbers:**
- **Optimal NTT**: ~130K cycles (if the code were restructured)
- **Current NTT**: ~63M cycles (actual code as written)

### 4.3 RLWE Ciphertext Operations

**Ciphertext Addition (polynomial add, trivial)**

An RLWE ciphertext is a pair of polynomials (a, b). Ciphertext addition
is two polynomial additions.

- 2 * poly_add(N=1024) = 2 * 15,360 = **~30,720 processor cycles**
- **Padded height: 2^15 = 32,768**
- **Proving time: <1 second**

This is cheap and entirely practical.

**Ciphertext Multiplication (poly_mul via NTT)**

Ciphertext multiplication in RLWE requires ~3 polynomial multiplications
(for the three terms of (a1,b1) * (a2,b2)) plus additions. Each poly_mul
calls: copy inputs, NTT(a), NTT(b), pointwise_mul, INTT(result).

Per poly_mul:
- 2 copies (N reads + N writes each): 2 * 1024 * ~7 proc = ~14,336 proc
- 2 NTTs: costs below
- 1 pointwise_mul: ~15,360 proc
- 1 INTT (= NTT + scale): NTT cost + ~15,360 proc (scale)

With **current NTT** (~63M each):
- Per poly_mul: 14,336 + 3 * 63M + 15,360 + 15,360 = **~189M cycles**
- 3 poly_muls for ciphertext multiply: **~567M cycles**
- Plus additions: ~50K cycles
- **Total: ~567M cycles**
- **Padded height: 2^30 = 1,073,741,824**
- **Proving time: 8+ hours CPU. Completely impractical.**

With **optimal NTT** (~130K each):
- Per poly_mul: 14,336 + 3 * 130K + 15,360 + 15,360 = **~435K cycles**
- 3 poly_muls: **~1.3M cycles**
- Plus additions: ~50K cycles
- **Total: ~1.35M cycles**
- **Padded height: 2^21 = 2,097,152**
- **Proving time: 1-3 minutes CPU, 10-30s GPU**
- **RAM: ~400 GB**

### 4.4 FHE Parameter Scaling

Real FHE schemes use N=2048 to N=65536 (not N=1024).

With optimal NTT, scaling is O(N log N):
| Ring Dim N | NTT Cycles | Poly_mul Cycles | Ciphertext Mul | Padded Height | Proving Time |
|------------|------------|-----------------|----------------|---------------|--------------|
| 1,024      | 130K       | 435K            | 1.35M          | 2^21          | 1-3 min      |
| 2,048      | 286K       | 930K            | 2.9M           | 2^22          | 3-8 min      |
| 4,096      | 614K       | 1.9M            | 6M             | 2^23          | 5-15 min     |
| 8,192      | 1.3M       | 4M              | 13M            | 2^24          | 15-40 min    |
| 16,384     | 2.8M       | 8.6M            | 27M            | 2^25          | 30-90 min    |
| 32,768     | 5.9M       | 18M             | 56M            | 2^26          | 1-3 hours    |
| 65,536     | 12.6M      | 39M             | 120M           | 2^27          | 2-5 hours    |

With current (wasteful) NTT, multiply all by ~500x. Nothing is feasible.

### 4.5 Verdict: FHE

**Ciphertext addition**: trivially practical at any ring dimension (<1 second).

**Ciphertext multiplication**:
- With optimized NTT (N=1024): marginal (1-3 minutes, 400GB RAM)
- With optimized NTT (N=4096, realistic params): 5-15 minutes
- With current NTT implementation: completely impractical at any dimension

**The current poly.tri NTT implementation is the critical bottleneck.**
Its nested loop structure wastes ~500x cycles due to bounded loop overhead
in the zkVM execution model. Restructuring the NTT to use direct butterfly
addressing (no inner guard loops) would make small-parameter FHE marginally
feasible.

Even with an optimal NTT, proving a single FHE multiplication at production
parameters (N=32768) would take 1-3 hours. A bootstrapping operation (which
requires many multiplications) would take days.

**Proving FHE operations is not practical on Triton VM today.** The value
proposition would be proving that an FHE operation was computed correctly
(verifiable FHE), but the proving cost is too high for interactive use.

---

## 5. Comparison to Other zkVMs

### 5.1 Triton VM vs. Modern zkVMs

| System           | Architecture   | Field        | Proving Speed (relative) | ML Support |
|------------------|---------------|--------------|--------------------------|------------|
| Triton VM        | STARK, custom ISA | Goldilocks (64-bit) | 1x (baseline)      | None (manual) |
| RISC Zero        | STARK, RISC-V | BabyBear (31-bit)   | ~3-5x faster       | Via RISC-V |
| SP1 (Succinct)   | STARK, RISC-V | BabyBear (31-bit)   | ~3-5x faster       | Via RISC-V |
| SP1 Hypercube    | STARK, RISC-V | BabyBear (31-bit)   | ~10-20x faster     | Via RISC-V |
| StarkWare S-two  | Circle STARK  | M31 (31-bit)        | ~100-1000x faster  | Custom circuits |
| zkPyTorch        | Custom circuits| BN254               | ~1000x faster (ML) | Native     |
| EZKL             | Halo2/KZG     | BN254               | ~100x faster (ML)  | Native     |

### 5.2 Key Differences

**Triton VM's disadvantages for computation:**
1. **Wide trace (300 columns)**: Proving cost scales with column count.
   RISC-V zkVMs have narrower traces. S-two uses ~50 columns.
2. **No precompiles or lookup tables for ML ops**: zkPyTorch/EZKL have
   custom circuits for ReLU, softmax, etc. Triton VM does everything
   in basic field arithmetic.
3. **No GPU prover (until recently)**: Neptune's GPU prover is new (2025)
   and still maturing.
4. **64-bit field**: Goldilocks is great for recursive proofs but means
   each field multiplication is more expensive to prove than 31-bit fields.

**Triton VM's advantages:**
1. **Hash acceleration**: Tip5 hash is a first-class co-processor. For
   hash-heavy workloads (Merkle trees, commitment schemes), Triton VM
   is competitive or superior.
2. **Recursive verification**: Triton VM can verify its own proofs,
   enabling proof composition. This is the killer feature for Neptune.
3. **Goldilocks field**: 64-bit field means less overflow handling and
   more natural representation of data.
4. **Deterministic execution model**: Great for financial applications
   and consensus-critical computation.

### 5.3 State of the Art for Each Domain

**Verifiable AI Inference (2025-2026):**
- zkPyTorch: VGG-16 (15M params) in 2.2 seconds
- DeepProve: Full GPT-2 inference proved
- EZKL: Production-ready for small-medium models
- Trident on Triton VM: MNIST MLP in 3-8 minutes. **~100x behind SOTA.**

**Verifiable Quantum Simulation:**
- No established precedent in the literature
- Trident would be among the first to offer this
- Practical for 2-12 qubits, which covers educational and protocol
  verification use cases

**Verifiable FHE:**
- Active research area (verifiable computation on encrypted data)
- No production systems exist
- Trident's approach (proving NTT/poly_mul) is theoretically sound
  but practically limited by the NTT implementation and proving costs

---

## 6. What's Practical Today vs. What Needs Hardware Acceleration

### 6.1 Practical Today (CPU, commodity hardware)

| Workload                           | Cycles    | Proving Time | RAM   |
|------------------------------------|-----------|--------------|-------|
| Hash verification (Merkle proof)   | <100K     | <1s          | <4GB  |
| Token transfer validation          | <500K     | <5s          | <16GB |
| Small quantum circuit (2-5 qubits) | <20K      | <1s          | <4GB  |
| RLWE ciphertext addition           | <50K      | <1s          | <4GB  |
| Simple smart contract logic        | <1M       | <30s         | <64GB |

### 6.2 Feasible with GPU Acceleration (RTX 4090 or better)

| Workload                           | Cycles    | Proving Time | RAM   |
|------------------------------------|-----------|--------------|-------|
| MNIST MLP inference                | ~2.2M     | 30-90s       | ~100GB|
| 8-10 qubit quantum circuit         | ~500K     | 5-15s        | <32GB |
| RLWE ciphertext multiply (N=1024)  | ~1.3M*    | 10-30s       | ~100GB|
| Medium smart contract              | ~5M       | 2-5 min      | ~200GB|

*Requires NTT optimization

### 6.3 Needs Next-Generation Provers (S-two class, 100x improvement)

| Workload                           | Cycles    | Current Time | Future |
|------------------------------------|-----------|--------------|--------|
| Small CNN inference                | ~5M       | 5-15 min     | 5-15s  |
| Transformer attention head         | ~12M      | 15-40 min    | 15-40s |
| FHE multiply (N=4096)              | ~6M*      | 5-15 min     | 5-15s  |
| 14-16 qubit quantum circuit        | ~30M      | 30-90 min    | 30-90s |

### 6.4 Impractical for Foreseeable Future

| Workload                           | Cycles    | Even at 100x | Reason |
|------------------------------------|-----------|--------------|--------|
| ResNet-18 inference                | ~500M     | 30+ min      | Too many MACs |
| GPT-2 single token                 | ~10B      | Hours        | Way too large |
| FHE bootstrapping                  | ~1B+      | Hours        | Inherently expensive |
| 20+ qubit quantum simulation       | ~500M     | 30+ min      | Exponential scaling |
| Real-time anything                 | -         | -            | Proving is never real-time |

---

## 7. Honest Verdict

### Where Trident on Triton VM is Competitive

1. **Hash-heavy cryptographic workloads**: Merkle proofs, commitment
   schemes, signature verification. Triton VM's hash co-processor makes
   these genuinely efficient. This is the sweet spot.

2. **Small provable computations for consensus**: Token transfers,
   state transitions, voting protocols. Programs under ~500K cycles
   prove in seconds and are the natural use case.

3. **Recursive proof composition**: Triton VM's ability to verify its
   own proofs enables proof aggregation and compression. This is
   architecturally superior to most zkVMs.

4. **Novel "verifiable quantum circuit" niche**: Small quantum circuit
   simulation (2-10 qubits) with a STARK proof of correctness has no
   real competition. It is a legitimate novel capability, even if limited
   to toy-scale quantum systems.

### Where Trident on Triton VM is Not Competitive

1. **AI inference**: 100-200x slower than purpose-built ZKML systems.
   For any serious ML proving, use zkPyTorch, EZKL, or DeepProve.
   Trident's tensor module is a proof of concept, not a production tool.

2. **FHE operations**: The NTT implementation is catastrophically
   inefficient (~500x overhead from loop waste). Even with optimal NTT,
   FHE multiply at production parameters takes hours. Use dedicated
   FHE-in-ZK systems instead.

3. **General-purpose computation at scale**: Anything over ~5M cycles
   becomes impractical. RISC-V zkVMs (SP1, RISC Zero) are better for
   large general-purpose programs because they have narrower traces,
   more mature tooling, and GPU provers.

4. **Latency-sensitive applications**: STARK proving is inherently
   batch-oriented. There is no path to sub-second proving for non-trivial
   programs on Triton VM.

### Actionable Recommendations

1. **Fix the NTT**: The poly.tri NTT wastes ~500x cycles due to its
   loop structure. Restructure to use direct butterfly addressing with
   precomputed stride patterns, eliminating the inner guard loop entirely.
   This alone would make small-parameter FHE marginally feasible.

2. **Add ReLU lookup tables**: If the VM added a precompile for ReLU
   (or general comparison-to-zero), AI inference costs would drop by
   ~30-50% by eliminating the expensive U32 split operations.

3. **Focus on the sweet spot**: Hash-heavy cryptographic protocols,
   small provable computations, recursive proof composition. Do not
   market Trident as an AI or FHE platform.

4. **Benchmark honestly**: Include proving time and RAM requirements
   in the cost model output. The current formula underestimates by
   2-5x for real-world usage.

5. **Track S-two and GPU provers**: A 100x proving speedup (achievable
   within 2-3 years based on S-two trajectory) would move the feasibility
   boundary from ~1M cycles to ~100M cycles, opening up small CNNs
   and medium quantum circuits.

---

## Appendix: Methodology Notes

### Cycle Count Estimation Method

Cycle counts were estimated by tracing through the Trident source code
(tensor.tri, gates.tri, poly.tri) and mapping each operation to the
Triton VM cost model defined in src/cost/model/triton.rs. The key
cost constants are:

- SIMPLE_OP (field add/mul, push, dup): [1, 0, 0, 1, 0, 0]
- RAM_RW (mem.read/write): [2, 0, 0, 2, 1, 0]
- U32_OP (split, comparison): [1, 0, 33, 1, 0, 0]
- loop_overhead: [8, 0, 0, 4, 0, 1]
- call_overhead: [2, 0, 0, 0, 0, 2]

### Proving Time Estimation

Two approaches were used:
1. **Trident formula**: `padded_height * 300 * log2(ph) * 3ns` (lower bound)
2. **Neptune calibration**: Real-world data showing ~400GB RAM at 2^21,
   GPU prover reducing "tens of seconds" to "a few seconds", block proofs
   taking "over a minute"

The "real-world estimate" column uses 2-5x the formula result, calibrated
against Neptune's published data.

### Sources

- Trident codebase: src/cost/model/triton.rs, std/nn/tensor.tri,
  std/quantum/gates.tri, std/private/poly.tri
- Neptune blog: "Habemus Verifier" (2024)
- Neptune forum: "Performance numbers for Triton VM proving"
- RISC Zero: zkVM 1.0 benchmarks
- Polyhedra: zkPyTorch VGG-16 benchmarks (2025)
- Lagrange: DeepProve-1 announcement (2025)
- StarkWare: S-two prover benchmarks (620K hashes/sec on M3)

---

## 8. Trident-to-CUDA: Competing with Zama on FHE

The analysis above assumes Triton VM as the sole target. If Trident adds
a CUDA backend, the competitive landscape changes fundamentally.

### 8.1 Core Operation Parity

**Performance parity on core operations (NTT, poly_mul)**: achievable.
These are regular enough that a good compiler matches hand-tuned code
within 10-20%.

**Performance parity on bootstrapping**: hard. This requires FHE-specific
compiler passes that understand blind rotation, key switching schedules,
and noise management. Doable but significant engineering.

### 8.2 The Dual-Target Advantage

The killer feature is not matching Zama's speed. It is **the same source
code compiling to both CUDA (for speed) and Triton VM (for proofs)**.

Write your FHE scheme once in Trident. Compile to CUDA for production
performance. Compile to Triton VM to prove the operation was correct.
No other system offers this.

That is verifiable FHE from a single source — fast execution on GPU with
optional STARK proof that the computation was faithful. Zama cannot do
that. Nobody can do that today.

### 8.3 Competitive Position vs. Zama

| Dimension              | Zama Concrete         | Trident-to-CUDA        |
|------------------------|-----------------------|------------------------|
| NTT throughput         | Hand-tuned CUDA       | Compiler-generated, ~1.2x gap |
| Bootstrapping          | Optimized pipeline    | Needs FHE-specific passes |
| Developer experience   | Python + calibration  | Native Trident, no calibration |
| Provability            | None                  | Same source to Triton VM |
| Multi-target           | CUDA only             | CUDA + zkVM + future targets |
| Ecosystem              | Mature (2+ years)     | New, but growing       |

### 8.4 Implementation Path

The LIR (Low-level IR) in `src/ir/lir/` already supports register-based
targets. CUDA maps naturally:

- LIR registers -> CUDA registers
- LIR loops with known bounds -> `<<<grid, block>>>` launches
- LIR memory ops -> global memory load/store
- Parallel loop annotations -> thread-level parallelism

Main engineering work:
1. Thread mapping heuristics (which loops become thread grids)
2. Memory coalescing analysis
3. Shared memory tiling for NTT butterflies
4. PTX/NVVM emission
5. FHE-specific passes for bootstrapping fusion

Estimated scope: 3-5K LOC for the backend, plus FHE-specific optimization
passes.
