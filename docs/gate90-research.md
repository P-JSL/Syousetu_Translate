# 90% target: experiments after v0.3.5

Reviewed on 2026-11-10. **The 90% target has not been achieved.** The published
[v0.3.5](https://github.com/P-JSL/Syousetu_Translate/releases/tag/v0.3.5)
remains the accepted implementation. Its three-run median reductions are
70.239% for the complete 35-chapter work and 73.214% for the complete 24-chapter
work. Exact samples and conditions are in [the release benchmark](gate70-benchmark.json).

## Target and measured bottleneck

Relative to the recorded v0.3.2 references, 90% requires at most **16.3111 s**
for 35 chapters and **26.3902 s** for 24 chapters, including process startup,
model verification/loading and complete live TXT saving. From v0.3.5 this needs
another approximately **2.98× / 2.68×** speedup on the tested workload.

An opt-in diagnostic build of the released implementation completed all 35
chapters in 53.7009 s with identical output. It measured 24.5487 s in encoding
and 27.9731 s in decoding, including 14.0485 s in output projection/selection.
FFN, attention and normalization measurements are nested; adding all counters
would double-count time. This diagnostic run has timing overhead and workload
noise and is not substituted for the release's three-run benchmark.

In that observed profile, even removing decoder time entirely would leave more
than the 16.3111 s target in encoding. Output projection alone cannot account
for the required reduction. This prioritizes whole-model matrix execution and
validated hardware acceleration; it is not a universal proof that 90% is
impossible on another implementation or device.

## Tested and removed prototypes

Each prototype retained the pinned model, full vocabulary and no-learning
condition. Runs used 128 logical contexts and four compute CPUs, cached HTML
and the same complete live-TXT timing scope. No CPU build ran during timing.

| Prototype | Complete 35-chapter seconds | Saved output | Decision |
| --- | ---: | --- | --- |
| Eight-row activation AoSoA packing | 49.1727 | Byte-identical | No observed overall gain |
| Eight-row packing with streamed top-one | 49.4643 | Byte-identical | No observed overall gain |
| Twelve-row packing and streamed top-one | 49.2974 | Byte-identical | No observed overall gain |
| Twelve-row explicit row expansion | 49.4737 | Byte-identical | No observed overall gain |
| Joint work title/introduction preparation | 48.5868 | Byte-identical | Difference too small to establish a gain |

These are exploratory single full-work runs, not confidence intervals or
replicated speed estimates. They do not justify replacing the released code.
The rejected candidates were not expanded into full 24-chapter trials.
[The experiment record](gate90-experiments.json) retains times, peak RSS, local
binary hashes, diagnostic counters and test results. Novel text and model files
remain private.

The matrix prototypes transposed already quantized activation codes into
row-group/K-group tiles, reusing the exact activation scales and signed-offset
correction. Their integer group sums and original F32 FMA order were retained.
Three numerical tests passed for each version, including changed/reordered
unrelated rows and full-matrix versus fused greedy selection. The five pinned
matrix output hashes also matched. Twelve rows used 24 integer/float vector
accumulators before operand temporaries; explicit row expansion tested whether
compiler indexing negated the register-layout benefit. Local matrix changes
did not establish a reduction in complete translation time. All prototype
application code was removed; no additional library or default setting ships.

## Additional recent mathematics reviewed

[Fast and Stable Arbitrary Precision Approximate Matrix Multiplication,
arXiv:2610.13905](https://arxiv.org/abs/2610.13905), §§3–5 and Theorem 4.7,
constructs products of the form `AB + epsilon*R1 + epsilon^2*R2 + ...`.
Full interpolation can recover an exact algebraic product, while truncation
trades accuracy for work. A floating-point forward-stability bound is not a
certificate of the existing Q8/F32 operator's output bits. Section 5 uses
MATLAB/binary64 examples and explicitly leaves high-performance implementation
to future work. Neither the asymptotic exponent nor fewer scalar products
establishes an INT8 CPU gain at this application's shapes. No APA replacement
was enabled.

[Fast Matrix Multiplication in fp8: Certified Coefficient Optimization and
Measured Error, arXiv:2609.26077](https://arxiv.org/abs/2609.26077), §§2–4 and
Appendix E.7, minimizes a coefficient functional over a change-of-basis orbit
using Kempf–Ness geometry and geodesic convexity. For an exact real rank-7
2×2 realization, `Phi_min = 200/9`, compared with `Phi = 8` for the cubic
reference. Under the paper's fixed-noise model the corresponding RMS ratio is
`sqrt((200/9)/8) = 5/3`; an optimal coefficient geometry does not imply the
reference's rounding behavior. Its measured throughput is hardware-dependent:
Appendix E.7 reports +3.16..8.00% on H20, −21.2..−11.3% on H100 and
−15.4..−9.9% on H200 versus cubic fp8 for the tested serving shapes.
These are fp8/GPU results, not Q8 CPU measurements. No Strassen/fp8 conversion
or basis change was applied to the shipped model.

For greedy scores, a sufficient stability condition is a winning margin larger
than twice a uniform logit-error bound: `s_best - s_second > 2*epsilon`.
A mean matrix error or loss measurement does not supply such a bound for every
token through multiple nonlinear layers. The shipped engine therefore keeps
its row-local quantization and full-vocabulary greedy selection; calibration
compares its two exact reductions. The earlier review
of [prefix invariance](https://arxiv.org/abs/2609.39816) remains relevant to
these tests; its low-precision Strassen results are not claimed as an applied
acceleration.

## Models and hardware

Public direct Japanese–Korean model candidates were searched. The results
included different encoder–decoder architectures and larger generative models;
no smaller replacement was validated for complete-work throughput, meaning
preservation and dialogue quality. A public small direct Marian checkpoint
was not established by the queried names. No replacement weights were installed
or bundled.

[ChainFlow](https://arxiv.org/abs/2611.08669) needs a compatible trained drafter;
one is not supplied for this SMaLL-100 checkpoint. Exact lookahead would require
causal multi-token verification, private-cache rollback and measured acceptance
versus verification cost. Neither is implemented or counted as a speedup.

The current test host exposes no GPU. GPU/AMX throughput and multiple-PC means
remain unmeasured. The present result supports retaining v0.3.5 and rejecting
the tested regressions; it does not support publishing a 90%-achieved release.
