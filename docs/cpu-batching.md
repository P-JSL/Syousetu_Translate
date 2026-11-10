# CPU batching, live output and validation

The prepared SMaLL-100 Q8_0 model and tokenizer are unchanged. The default
full-vocabulary decoder improves CPU execution and scheduling. The earlier
Korean-focused shortlist has been removed from development source after meaning
regressions were observed. Historical v0.3.3 measurements are retained below.
Current work is described in [the November research review](inference-research-2026-11.md).
Chapter-window ownership and profiling are described in
[chapter batching](streaming-batches.md).
All application, inference, glossary packing and new validation code is Rust.

## Applied research

Full texts and the relevant upstream implementation were reviewed. Their
published speedups concern different hardware/models; the measurements below
come from this application.

| Source | Applied idea and limits |
| --- | --- |
| [Pope et al., Efficiently Scaling Transformer Inference, 2023](https://arxiv.org/abs/2211.05102) | Separate prefill from token decoding; batch linear operations to reuse weights and amortize memory traffic. This CPU encoder–decoder implementation does not reproduce the paper's TPU deployment. |
| [Yu et al., Orca, OSDI 2022](https://www.usenix.org/conference/osdi22/presentation/yu) | Schedule at token iterations and replace completed contexts. Batch common projections/FFN while keeping variable-length attention separate. No serving framework or GPU runtime is added. |
| [Khudia et al., FBGEMM, 2021](https://arxiv.org/abs/2101.05615) | Use packed low-precision weights, fixed register blocks and fused activation packing/block sums. The existing Candle backend is specialized in Rust; FBGEMM is not a dependency. |
| [Jacob et al., Integer-Arithmetic-Only Inference, CVPR 2018](https://arxiv.org/abs/1712.05877) | Treat scales, saturation and rounding as numerical contracts. SIMD activation packing is checked against the scalar algorithm. No QAT or new integer-only quantization scheme is claimed. |
| [Jean et al., Very Large Target Vocabulary for NMT, 2015](https://arxiv.org/abs/1412.2007) and [Marian's shortlist implementation](https://github.com/marian-nmt/marian/blob/master/src/data/shortlist.h) | Reduce the expensive output projection to candidate rows, retaining an original-ID mapping. The optional Korean/source-piece heuristic is specific to this application and can change output; it is not a learned lexical shortlist from the paper. |

The original translator7 Java source uses a fixed worker pool and ordered
responses from isolated ezTrans DLL helper processes. Rust/Rayon supplies bounded
parallel computation here, without starting a full neural model per worker.
This architecture also retains the shared immutable data/ordered merge ideas
previously adapted from `gacha_pickup_calculator`.

## Contexts, compute and numerical boundaries

- Default capacity is **32 logical contexts**; `--jobs 1..256` supports 16, 32,
  64 and larger optional capacities. One Rayon pool is capped by available CPUs and the requested
  CPU budget. A four-CPU host with 32 contexts still uses four compute threads.
  The CLI sets `CANDLE_NUM_THREADS=1` before creating threads so the legacy
  kernel barrier pool does not add another compute pool. Library embedders can
  set the same variable at process startup, as the benchmark example does.
- Weights, tokenizer and correction snapshot are shared. Each active context
  owns its self/cross attention cache, positions, token history and source index.
  Source sequences are concatenated only for independent linear/FFN work;
  attention uses each sequence's original boundaries, with no padding or
  attention across source sentences.
- Embedding rows are gathered in one operation with explicit per-source
  position IDs, avoiding a tensor/allocation chain per live context. The serial
  embedding path remains an independent integration-test reference.
- New inputs refill completed contexts at iteration boundaries. Small pending
  prefills are grouped to avoid interrupting every decoding iteration. Greedy
  special-token filtering, lower-original-ID ties, repeated-trigram bans,
  output limits and bounded source splitting are retained.
- Large batched Q8 tiles and activation preparation run on the shared pool.
  Q8_0 register blocks specialize one to eight rows, with smaller blocks for
  small matrices. AVX2 activation packing retains first signed maximum, scalar
  round-away ties, saturation, scales and sums. Unusual nonfinite/overflow input
  uses the scalar fallback. Runtime CPU detection gates SIMD; portable builds
  do not require AVX-512 or `target-cpu=native`.
- Full-vocabulary top-one selection uses an AVX2 finite argmax with the exact
  signed integer keys of Rust's [`f32::total_cmp`](https://doc.rust-lang.org/std/primitive.f32.html#method.total_cmp).
  Lane reductions retain the smallest original ID on ties, including signed
  zero ordering. Special tokens and nonfinite scores are excluded. A blocked
  winner triggers the scalar repetition-aware scan, preserving its result.
  Unsupported CPUs use the scalar selector. Boundary/random/special-value tests
  compare against the original reference.
- Duplicate and cached lines are restored into every original slot. Only the
  completed contiguous prefix reaches the output callback; out-of-order
  completion cannot reorder, duplicate or skip a saved original line.

`auto` compares full serial/batch sample outputs and timings before selecting
batch execution. Explicit `parallel` skips that calibration. Tests compare
serial/batch results at 2/16/32/64/128/256 contexts, including variable-length text,
multiline input, duplicates and literal lines. Equal saved files establish
tested execution consistency, not literary translation accuracy or universal
floating-point identity across CPUs.

## Retired shortlist prototype

The opt-in shortlist from v0.3.3 reduced projection work but changed units,
names and meanings in inspected examples. Current source removes the option,
candidate gathering, mapped heads and their cache scope. `--fast-vocabulary`
returns a clear retirement error; `--full-vocabulary` remains compatible with
existing scripts. Earlier prototype timings below are historical evidence,
not a quality-preserving acceleration or an available development-source mode.

## Live TXT and compressed dictionary

Novel and text-file translation open the final `.txt` directly. Completed
ordered nonblank translations are flushed immediately; blank rows join the
next flush or chapter/chunk boundary. There is no temporary translation file
renamed at completion. Existing output is copied to an exclusively created
`.backup` before truncation. Successful completion syncs TXT and removes its
backup. Failure/cancellation preserves the completed prefix and original backup.
A leftover backup prevents silently overwriting that recovery copy on a retry.
Metadata and correction-memory transactions continue to use atomic files.

The executable embeds an independently authored **1,058-headword** Japanese–Korean
glossary in **9,144 Deflate bytes**, alongside the prior 68 sentence/name defaults
and six spelling rules. Headwords match exact sources, preserving sentence
context rather than replacing dictionary forms inside arbitrary text. A
persisted dictionary revision applies the new defaults once; user overrides and
deletions survive reload. This is correction memory, not neural retraining.
See [`data/dictionary/`](../data/dictionary/README.md) for reviewable TSV and
the Rust packing command. No proprietary DLL/dictionary is redistributed.

## Reproducible validation

Measurements use an AMD EPYC 9V45 host with a four-CPU quota, portable generic
Rust release builds and the same pinned Q8 model. Source chapter HTML is cached
privately. Timings include process startup, model verification/loading, parsing,
translation and complete bilingual TXT saving; network and download delays are
excluded. `--no-learning` disables correction-memory shortcuts, including the
new glossary. In-process result caches start empty in each invocation. Programs
run sequentially without concurrent CPU builds. Peak RSS is sampled from `/proc`.

Reference command (replace executable, paths, range and job count):

```sh
syousetu-translate download --url WORK_URL --html index.html \
  --episode-html-dir cached-html --start 1 --end 35 --out novel.txt \
  --model-dir pinned-model --no-learning --parallel-mode parallel \
  --jobs 32 --japanese --no-delay --full-vocabulary
```

The reference v0.3.2 invocation requests 16 workers, which its old backend caps
at four; the new backend uses 32/64 logical contexts on the same four compute
CPUs. The comparison measures the complete batching change, not a change in
model or available CPU allocation. Approximate timings below refer to a removed
prototype and cannot be reproduced through the current production interface.

The two user-specified works are
[35 chapters](https://kakuyomu.jp/works/11054822663814504924) and
[24 chapters](https://kakuyomu.jp/works/16818622176193367283). Results below are
single full-work runs, not confidence intervals or guaranteed speedups on every
computer. The original ezTrans 24-chapter/13-second figure is user-reported;
it uses a different engine and unverified timing/cache conditions. It is not
presented as a reproduced neural-inference result.

Measured on 2026-11-09 (elapsed seconds; smaller is better):

| Work / mode | Logical contexts | Seconds | Reduction from v0.3.2 | Saved output |
| --- | ---: | ---: | ---: | --- |
| 35 chapters, v0.3.2 reference | 4 (old cap) | 163.11 | — | Reference |
| 35 chapters, full vocabulary | 64 | 79.14 | **51.5%** | Byte-identical |
| 24 chapters, v0.3.2 reference | 4 (old cap) | 263.90 | — | Reference |
| 24 chapters, full vocabulary | 64 | 107.23 | **59.4%** | Byte-identical |

Full vocabulary clears the requested **50% release gate** on both complete
works with `--parallel-mode parallel --jobs 64 --full-vocabulary`. The application
keeps its default capacity of 32 for lower memory use; select 64 in settings to
use the measured profile. Results are hardware/workload-dependent.
Full-vocabulary files contain
627,825 bytes (35 chapters) and 994,979 bytes (24 chapters). Their SHA-256 values:

```text
35: 28966275a6b45b95bb12a182ac1fc431113c047193fe066b021fd9db537aa317
24: 71d7501ec4c2d2c027c0bc0f63d3a644c28d573578e7f8533483269d0946f202
```

Peak RSS was approximately 1,101 MiB for 35 chapters and 1,044 MiB for 24 chapters.
Capacity 64 can use more RAM on other inputs; more contexts do not always help.

Before the final embedding/argmax optimization, experimental fast-vocabulary
runs took 60.52 seconds on 35 chapters (32 contexts) and 82.34 seconds on 24
chapters (64 contexts): 62.9%/68.8% shorter than the same references. Those files
differed in 25/three lines while retaining all original/translation pairs. These
are prototype measurements, not the final full-vocabulary release gate.
Output differences are not evidence of improved semantic quality. **70% has not
yet been established**; further work follows the 50% release.

## Phase 2 development measurement

The exact implementation commit and executable/model hashes are recorded in
[phase2-benchmark.json](phase2-benchmark.json). On the same four-CPU allocation,
64 contexts, full vocabulary, cached HTML and no correction memory, complete
cold-process runs through live TXT save took **67.02 seconds for 35 chapters**
and **86.23 seconds for 24 chapters**. Relative to the recorded v0.3.2 references,
these are **58.9% and 67.3% shorter**. Each is one full-work measurement, not a
multiple-PC mean or a guaranteed improvement on every device.

All 35/24 chapters and every nonempty original/translation pair passed structural
verification (3,354/8,533 nonempty source rows). New paragraph and prose handling
intentionally changes formatting/wording from v0.3.3; equality to the old TXT is
not asserted. Structural completeness does not certify literary accuracy.
The **70% and 90% gates remain unmet**, so this change is not labeled a release
that achieved either target. The separate authored model regression suite checks
contractions, obligation, negation, requests and speech levels without memory hits.

## Saved output verification

Verify every saved original/translation pair, empty line and chapter boundary
offline with the Rust helper (default title format, `--japanese`, no furigana):

```sh
cargo run --locked --release --no-default-features --example verify_saved_novel \
  -- cached-html novel.txt 35
```

It emits only counts and checksums and fails on chapter reordering, missing or
modified originals, empty translations and extra body text. Private novel text
and cached HTML are not committed or included in releases. Unit tests also
inject out-of-order/missing callbacks, disk-write errors and interrupted files;
SIMD tests compare 507 blocks against scalar values, scales and sums.
