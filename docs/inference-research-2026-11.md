# Inference research and mathematical checks

Reviewed on 2026-11-10. These decisions concern the pinned SMaLL-100 Q8_0
Japanese→Korean model, not the throughput of unrelated large GPU models.
Application and maintainer implementations are Rust. Research downloads and
private novel text stay outside the repository and portable packages.

## Recent sources and decisions

| Source | Finding relevant here | Decision |
| --- | --- | --- |
| [MobiInfer, Nov 2026](https://arxiv.org/abs/2611.08496), §§3–4, Algorithm 1 | Distinguishes phases, reuses validated state, verifies speculative candidates. Its reduced output vocabulary is explicitly approximate. | Keep immutable weights and private per-source KV state. Measure exact projection paths on the actual CPU. Remove the earlier shortlist after observed unit/name/meaning loss; do not transfer GUI-schema assumptions to prose. The CPU path tuner is our implementation, not a reproduction of MobiInfer. |
| [ChainFlow, Nov 2026](https://arxiv.org/abs/2611.08669), §§3–4 | Trained latent flow drafters and target verification can reduce sequential decoding on Qwen3.5. Draft-only shortlists affect acceptance, while target verification remains necessary. | No compatible trained SMaLL-100 drafter is supplied. Do not claim its GPU/model speedups or ship an untrained intermediate head. |
| [Beyond Accuracy: Prefix-Invariant Realizations…, Sep 2026](https://arxiv.org/abs/2609.39816), Proposition 1, §§4–5 | A bounded-integer certificate can preserve a row-local INT8 operator under Strassen mixing. Full-range codes need overflow correction; the reported implementations are slower than classical INT8 at the tested shape. | Apply row-local quantization, fixed rescaling/FMA order, integer range analysis and changed-suffix/permuted-row regression checks. Do not replace the current Q8 kernel with an uncertified low-precision Strassen realization. |
| [Ozaki Scheme II on CPUs, Sep 2026](https://arxiv.org/abs/2609.27831), §§2–4 | Residue arithmetic and CRT reconstruct multiple-precision products using several low-precision GEMMs. | Our product already uses bounded integer dots followed by F32 scales; it does not require a 53–2048-bit product. Multiple moduli and CRT would add work. AMX-specific gains do not establish a gain on the tested AMD CPU. |
| [DIVERSED, Apr 2026](https://arxiv.org/abs/2604.07622) and [current Transformers assisted-decoding docs](https://huggingface.co/docs/transformers/main/en/assisted_decoding) | Relaxed/ensemble verification changes the target distribution. Prompt lookup and trained early-exit heads have different prerequisites. | Retain target-only greedy selection. Japanese source tokens rarely provide Korean continuations; no source-copy speedup is assumed. SMaLL-100 has no trained early-exit head in this package. |
| [Lookahead Decoding, 2024](https://arxiv.org/abs/2402.02057), §§2–3 | Treats greedy decoding as a triangular fixed-point problem and verifies candidate n-grams. | A candidate is usable only after matching the target's conditional greedy result. KV rollback, causal masks and acceptance/cost measurements would be required for a separate implementation; lookahead decoding is not implemented or enabled here. |
| [FlashAttention, 2022](https://arxiv.org/abs/2205.14135) | Attention benefits from accounting for memory traffic and intermediate materialization. | CPU attention reuses scratch and removes intermediate Tensor/layout objects, retaining the original GEMM/softmax reduction order. This is not the GPU FlashAttention algorithm. |
| [Google DeepMind AlphaEvolve, 2025](https://deepmind.google/blog/alphaevolve-a-gemini-powered-coding-agent-for-designing-advanced-algorithms/) | Reports verified kernel search and 48 scalar products for 4×4 **complex** matrices. | Neither complex arithmetic nor a 4×4 scalar count establishes faster rectangular Q8 projections. Evaluate actual matrix shapes and exact output checks instead. |

Earlier batching/packing sources and measurements are in [CPU batching](cpu-batching.md).
Preprints and vendor measurements are evidence to investigate, not guarantees for
this model. No paper's reported acceleration is presented as our measured result.

## Exactness conditions applied

For an activation row `a` and output row `w`, the existing Q8 kernel computes
integer products in groups of 32, applies that group's weight scale and the
activation block's scale, and accumulates in a fixed F32 FMA order. Repacking
shares loads across rows; it does not add or quantize different source rows
together. Scales, groups and accumulation order stay unchanged.

For signed byte magnitudes bounded by 128, the unsigned-offset dot and correction
satisfy

```text
abs(sum((w + 128) * a)) <= 32 * 255 * 128 = 1,044,480
abs(128 * sum(a))       <= 32 * 128 * 128 =   524,288
abs(sum(w * a))         <= 32 * 128 * 128 =   524,288
```

These integers fit INT32 and the exact-integer range of F32 (`2^24`). This
justifies the conversion and offset correction; it does not justify changing
the later floating-point scale/FMA order. Scalar quantization tests separately
cover ties, subnormals and nonfinite fallback. Prefix tests change unrelated
rows and their order, comparing existing output **bits**, rather than relying
on an aggregate translation score to detect cross-source interference.

The streamed output selector partitions the entire vocabulary into tiles.
After excluding nonfinite, special and repetition-blocked tokens, order a
candidate by `(f32::total_cmp(score), -original_token_id)`. This is a total order,
so

```text
max(union of all tiles) = max(tile maximum for every tile).
```

Every valid original ID is still considered. The lower-ID tie rule, signed zero
ordering and EOS handling match the dense reference. The selector needs no
softmax because greedy softmax argmax has the same maximizer for finite logits.
No probability threshold, reduced vocabulary or approximate nearest-neighbor
index is substituted for the original maximum.

`model/projection.rs` warms both implementations, discards the first timing,
alternates timing order and compares three paired samples for each row-count
bucket. It chooses fused selection only when its median is at least 5% shorter.
Reference tokens are returned during calibration; a disagreement or unsupported
backend selects the dense path. Lock poisoning is an error. This is a bounded
empirical policy, not a statistical guarantee of optimal latency.

The fused bias/ReLU operation computes the same per-element expression
`max(x + bias, 0)` with the same F32 addition, while removing a separate buffer
and pass. CPU attention likewise keeps independent source/head reductions.
Tests compare each fused operation against independent Tensor operations.
Bitwise checks cover the tested CPU/operators; they are not a cross-platform
floating-point identity guarantee or proof of general translation fluency.

## Cost model and release gates

For `r` decoder rows, hidden width `d=1024` and vocabulary `V=128112`, output
projection alone requires approximately `2*r*d*V` integer arithmetic operations.
The dense F32 score tensor has `4*r*V` bytes: **32,796,672 bytes** for `r=64`.
Streaming removes that dense score write/read, but still computes all logits.
The Q8 output weights occupy `V*d*(34/32) = 139,385,856` bytes. Actual cache
traffic depends on tile reuse, so a scalar operation count is insufficient.

The roofline lower bound is
`T >= max(operations / effective_compute_rate, bytes / effective_bandwidth)`;
allocation, normalization, parsing and I/O add further time. Effective rates
must be measured. More logical workers need not reduce either bound.

For a draft of length `k` with conditional acceptance probabilities `alpha_i`,
expected committed tokens per verification are
`E[L] = 1 + sum(j=1..k, product(i=1..j, alpha_i))` under the usual matched-prefix
verification scheme. A speedup requires draft plus verification cost to be
less than `E[L]` ordinary steps. A high acceptance rate alone is insufficient
when the CPU already batches many independent sources. Acceptance rates for
this model have not been measured; no speculative-decoding gain is claimed.

Amdahl's law also bounds end-to-end claims: if fraction `f` is accelerated by
`s`, total speedup is `1 / ((1-f) + f/s)`. A 90% time reduction requires a 10×
end-to-end speedup, not a 90% reduction in one small kernel.

Use cold-process, no-learning, full-work translation **through final TXT save**
for gates. The original v0.3.2 references are 163.1107 seconds for 35 chapters
and 263.9015 seconds for 24 chapters on the recorded four-CPU environment.
Thus 70% requires at most 48.9332 / 79.1705 seconds, and 90% requires at most
16.3111 / 26.3902 seconds, respectively. Timing and structural output evidence
belongs in the accompanying benchmark record; these equations are not evidence
that either gate has been achieved. No GPU is exposed in the current test
environment, so GPU throughput and multiple-PC averages remain unmeasured.
