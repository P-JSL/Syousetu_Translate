# SMaLL-100 INT8: exact AVX2 fallback acceleration

2026-11-17 (Asia/Seoul). The complete 35-chapter first translation on the tested
Q8 AVX2 fallback decreased from **831.973 s to 287.159 s**, a **65.485% reduction**.
The full TXT is byte-identical and structurally complete. **The requested
additional 85–95% target remains unmet.** This result is specific to the tested
fallback; it is not a speed claim for the already optimized VNNI paths.

## Change

The generic Q8_0 matrix loop previously repeated the same weight loads and F16
scale conversions independently for every input row/output column. The new
AVX2/FMA kernel keeps two independent rows in registers against four shared
weight rows. The existing sixteen-row scheduling tile converts each weight
scale once; each matrix call decodes each input scale once into reusable
thread-local scratch. No weight/activation quantization is changed.

For full sixteen-row tiles, the scale-conversion count changes approximately
from `2*m*n*(k/32)` to `n*(m/16)*(k/32) + m*(k/32)`. This is a count of metadata
conversions, not an application speedup estimate. Odd rows and incomplete
column quads retain the original arithmetic. The original u64-aligned quantized
scratch remains separate from the decoded-scale buffer; tests switch quantized
types on the same thread to detect stale data or layout coupling.

For output partial lane `j` and weight block `b`, both implementations use the
same update:

`a[j,b+1] = fma(d_weight[b] * d_input[b], integer_partial[j,b], a[j,b])`.

The eight original partial sums, their block order, integer sign/saturation
operations, FMA operand order and final horizontal reduction are unchanged.
Cached values are lossless F16→F32 conversions of the same stored scales.
Every input retains its own quantization scales. The method removes redundant
work; it does not omit neurons, words, vocabulary candidates or decoder steps.

AVX2/FMA support and block lengths are checked before entering the small unsafe
kernel. Other data types/platforms retain their original path; existing VNNI
dispatch still takes priority. There is no new dependency, model download,
Python/C++ application code or weight file. The maintainer-only matrix example
is excluded from normal release binaries.

## Validation and limits

The host is an AMD EPYC 9V45 with a four-CPU quota and no GPU. Setting
`MISTRALRS_FORCE_AVX2=1` selects the Q8 fallback on this VNNI-capable host. It
does not emulate every instruction/cache property of an older AVX2-only PC;
other CPU kernels retain their normal dispatch. These are not average-PC data.

Both complete-work processes use 128 logical contexts, parallel mode,
`CANDLE_NUM_THREADS=1`, `--no-learning --no-result-cache`, cached source HTML
and a fresh process. Timing includes complete model verification/loading and
ordered live TXT saving. HTTP time is excluded and the OS page cache is not
cleared. No CPU build or other model test ran during either timed process.

Exactly **one completed control and one completed final candidate** were timed.
The intermediate two-row prototype's scheduled process was cancelled at startup
after the control was captured; it is not a timing observation. The final
candidate is not rerun for a favorable sample. Linux `VmHWM` is sampled every
10 ms, so a short final peak can be missed.

| Complete 35 chapters | Existing | Candidate |
| --- | ---: | ---: |
| Process wall time | 831.973 s | 287.159 s |
| Sampled peak RSS | 882,308 KiB | 860,336 KiB |
| Saved output | 607,849 bytes | 607,849 bytes |

Both contain all 6,248 source lines and share SHA-256
`6423d3669c60b67d5d7907f9d04c11631a4d8d2331973e5b31f830a673d07a0a`.
Measured RSS is **21.46 MiB / 2.49% lower**, within the review allowance of
at most +512 MiB and +25%. This is not a runtime-enforced global RSS limit.

The normal accelerated path additionally translated and saved all **24 chapters
once**, then matched its previously saved control byte for byte:
937,029 bytes, 17,041 source lines, SHA-256
`095a4045e8430fcd844656235002212550582173cd753bec7779a5a8579c1992`.
This checks preservation of that path; it is not a new speed comparison.

Numerical regressions compare every score against the original single-row
dots, including zero inputs, sign changes, row reordering, block sizes through
4096, odd rows, column tails, parallel scale preparation and cross-type scratch
reuse. Full production/legacy tests and strict Clippy pass. Output equality
preserves existing translations; it does not certify the original model's
semantic accuracy. [Exact metrics and hashes](first-translation-avx2.json)
separate the tested path from broader performance claims.

The initial two-row prototype's separate matrix passes showed 30–60% reductions
with identical score bits, including reversed run order. They are not included
in the full-work reduction and are not measurements of the final cached-scale
implementation.

## Research connection

The [BLIS framework and its cited matrix-kernel papers](https://github.com/flame/blis#citations)
motivate register blocking and reuse of shared matrix operands. The
[Orca selective batching design](https://www.usenix.org/conference/osdi22/presentation/yu)
is already used by the application's independent decoder contexts. This change
applies those reuse principles inside the existing CPU Q8 loop, with an explicit
floating-point equivalence constraint. BLIS/Orca runtimes and their published
speedup figures are not imported or counted as this application's results.
