# Native VNNI follow-up: bounded calibration, rejected M7 prototype

2026-11-17 (Asia/Seoul). The additional **85–95% first-translation reduction
has not been achieved**. The previous 65.5% result concerns the forced AVX2
fallback, not this native VNNI path.

## Completed comparison

One existing executable and one prototype translated the complete 35-chapter
work `https://kakuyomu.jp/works/11054822663814504924` in fresh processes.
The AMD EPYC 9V45 host has a four-CPU quota and no GPU. Both used normal CPU
dispatch, 128 logical contexts, `CANDLE_NUM_THREADS=1`, `--no-learning`,
`--no-result-cache`, and cached HTML. Wall time includes model integrity
verification/loading and ordered live TXT saving, excludes HTTP time, and does
not clear the OS file cache. No compilation or other inference ran concurrently.

| Complete first translation | Existing | M7 + calibration prototype |
| --- | ---: | ---: |
| Wall time | 52.283880468 s | 52.022779975 s |
| Sampled peak RSS | 1,373,800 KiB | 1,329,852 KiB |
| Complete chapters | 35 | 35 |
| Saved bytes | 607,026 | 607,026 |

Both TXT files have SHA-256
`a01ef9cac141faa625e740b353a6c2ed8800c9cec58f68e8fa5599fe879007bb`.
Independent structural checks found all 6,248 source lines, all chapters and
their original order. This preserves the existing model's translation; it does
not certify its semantic accuracy.

The observed difference is only **0.4994%**, with one pair and no statistical
confidence estimate. It is insufficient evidence to accept a faster kernel.
**The M7 kernel and its experimental dispatch were removed.** The measured
42.92 MiB / 3.20% RSS reduction belongs to the combined prototype and must not
be presented as a separate final-code memory result. The +512 MiB and +25%
review limits are measurement criteria, not a runtime global memory limit.
No second timing pair was run to obtain a more favorable result.

The exact model SHA, executable hashes and measurements are recorded in
[the companion JSON](first-translation-native-followup.json). The prototype
changed only the encoder's existing M6 dispatch to M7 for `m>=256,n<16384`,
added the matching single-K-group specialization, and used the final bounded
projection calibration. Its integer products and per-output FMA order were
unchanged. An independent scalar oracle also checked a 260-row, odd-N-tile
matrix before the prototype was removed.

## Retained change

Projection calibration previously executed a warmup pair and three additional
dense/fused pairs per row-count bucket. It now executes at most **two pairs**:
the first warms both paths, and the second reverses order and decides. Fused
selection must be more than 5% shorter in the deciding sample. Token mismatch
or an unsupported backend selects dense; calibration always returns the dense
reference. Lock poisoning remains an error.

This removes up to two unnecessary alternate full-vocabulary evaluations per
bucket. It does not establish an 85–95% application speedup or a statistically
optimal choice from two observations. Tests check discarded warmup timing,
the strict 5% boundary and disagreement on either pair. The matrix benchmark
example also defaults to one run, permits at most two, and hashes its first
measured output rather than executing extra inference for hashing. It was not
run as an additional performance experiment.

After removing M7, the final executable translated the original sixteen-line
dialogue/narration fixture once as a correctness check, without timing it.
Its output is byte-identical to the saved reference, SHA-256
`9ae7d435e10db609d46393830b1a46769f489e14b06987fd9ee7c0d3ba3b4199`.
The complete production regression suite, the matrix-example boundary tests,
the release CLI build and strict Clippy passed locally:

```sh
cargo test --locked --release --features gui --lib --bins --tests
cargo test --locked --release --example benchmark_projection
cargo build --locked --release --features gui --bin syousetu-translate
cargo clippy --locked --features gui --all-targets -- -D warnings
```

## Why a larger change is needed

In the earlier diagnostic, encoder and decoder scopes took 28.70 s and 33.43 s
of 67.39 s; projection's 17.88 s is inside the decoder. Eliminating projection
entirely would remove only about 26.5% in that diagnostic. This is a scope-based
Amdahl check, not a prediction for the uninstrumented executable. Relative to
the current same-session control, 85%, 90% and 95% reductions would require
7.8426 s, 5.2284 s and 2.6142 s respectively. Neither this prototype nor the
existing profile demonstrates those rates.

[CTranslate2's official performance guidance](https://opennmt.net/CTranslate2/performance.html)
recommends INT8, packed matrices, bounded CPU threads, batching and greedy
selection without unused softmax scores. Those principles are already used
here. Its [quantization documentation](https://opennmt.net/CTranslate2/quantization.html)
uses a per-weight-row scale; this GGUF uses per-32-weight scales and the native
CPU kernel uses per-256-activation scales. An engine change is therefore not
an identity-preserving substitution, and its speed claims are not our results.

A GPU implementation could accelerate more of the encoder/FFN/projection,
but this repository is currently CPU-specific: weight/tensor placement,
normalization, bias operations, attention and selection need device support.
Candle's CUDA Q8 path also uses a different activation quantization. A working
GPU path requires real-device quality, memory and missing-driver validation;
no such path or GPU speedup is claimed or enabled by this change. No dependency,
model replacement, pruning, approximate shortlist or unvalidated GPU switch is
added to the production release.
