# SMaLL-100 INT8: first-translation K/V review

2026-11-17, Asia/Seoul. **The additional 85–95% target has not been achieved.**
SMaLL-100 INT8 and the accepted runtime at `6d73310` remain unchanged. All K/V
storage prototypes described here were removed after testing. No alternate
model or inference library is included in the application or release.

The current acceptance baseline is the existing SMaLL-100 INT8 executable,
measured in the same session. Historical v0.3.2 times are supplementary history,
not the current target baseline. The benchmark now defaults to **one pair**
(one control and one candidate), permits at most two trials per variant, and
reports the proper midpoint for two observations. Results are not reused from
earlier translations.

## Conditions and final candidate

Each trial starts a new process with `--no-learning --no-result-cache`, 128
logical contexts, parallel mode and `CANDLE_NUM_THREADS=1`. The host has an
AMD EPYC 9V45, four-CPU quota and no GPU. Timing covers model verification/load,
translation and complete live TXT saving. Source HTML is cached; HTTP time is
excluded and the OS page cache is not cleared. No CPU builds or other neural
tests ran concurrently with the measurements.

The candidate replaces per-token tensor concatenations with private planar K/V
buffers. Geometric growth is bounded to 32 spare tokens per context/layer;
`reserve_exact` avoids an additional implicit doubling. Buffers over 32 tokens
are released on sentence reset. The pinned three-layer, 1024-wide decoder at
the supported maximum 256 contexts requests at most **192 MiB of spare K/V
payload**. This is not a bound on process RSS, temporary growth allocations,
allocator overhead, source text, weights or necessary attention state.

The numerical tests compare against independent concatenation and masked
attention with 16 heads of width 64, multi-token appends and growth boundaries.
They also cover invalid inputs, clone isolation and sentence reset. Actual
model tests cover serial/parallel equality through 256 contexts, dialogue
regressions and user-correction precedence. Those tests passed. No zero-valued
or nonzero-valued neural operations were skipped by these storage experiments.

## Results and decision

| Complete work | Existing | Bounded candidate | Change |
| --- | ---: | ---: | ---: |
| 35 chapters, midpoint of two pairs | 50.8948 s | 49.9090 s | 1.937% shorter |
| 24 chapters, one pair | 72.1930 s | 74.3884 s | **3.041% longer** |

All reported completed TXT files match the control byte for byte and pass the
independent chapter/line/order verifier. This establishes implementation
equivalence, not semantic accuracy certification of the original model.

Measured process memory is sampled from Linux `VmHWM` every 10 ms; a short
final peak can be missed. Maximum sampled RSS for the reported observations:

| Work | Existing | Candidate | Increase |
| --- | ---: | ---: | ---: |
| 35 chapters | 1,417,184 KiB | 1,467,760 KiB | 49.39 MiB / 3.57% |
| 24 chapters | 1,350,664 KiB | 1,457,224 KiB | 104.06 MiB / 7.89% |

Both pass the review allowance of **at most +512 MiB and +25%**, but the
24-chapter latency regression prevents acceptance. These are measurements on
one host, not multiple-PC averages or runtime-enforced RSS limits.

Exact executable/model hashes, samples and acceptance flags are in the
[35-chapter record](first-translation-kv-35.json) and
[24-chapter record](first-translation-kv-24.json). The earlier three-pair loop
was stopped after the user's two-trial limit; its extra completed control is
disclosed in the 35-chapter record and excluded from the two-pair comparison.
There was no further 35-chapter run. Only one pair was run for the final
24-chapter comparison.

Earlier unbounded-buffer experiments, performed before the two-trial limit,
are retained separately in the [35](first-translation-kv-unbounded-35.json) and
[24](first-translation-kv-unbounded-24.json) prototype records. Their apparent
1.96% / 8.24% median reductions are not measurements of the final bounded
candidate. A separate 35-chapter pilot increased sampled RSS by 258.51 MiB /
20.97%. Separate-vector and token-major prototypes also failed to establish a
better default. None of these implementations ships.

## Research boundary

[Kwon et al., PagedAttention, 2023](https://arxiv.org/abs/2309.06180) motivates
examining redundant K/V copies and memory fragmentation. This CPU experiment
uses Rust vectors, not the paper's GPU paging kernels; its published serving
speedups are not claimed here.

[Liu et al., Deja Vu, ICML 2023](https://proceedings.mlr.press/v202/liu23am.html)
uses trained contextual-sparsity predictors and approximate subsets. None is
enabled here. Further work must first measure **exact zero** activations and
INT8 matrix costs in the pinned SMaLL-100 checkpoint. Guessed neuron removal,
token shortlists, other models and retranslation caches do not satisfy the
current first-translation target.
