# SMaLL-100 INT8 and inference performance (0.3.1)

## Model and preparation

The default is direct Japanese→Korean SMaLL-100 with persistent **Q8_0 INT8**
weights. `models/manifest.json` pins the upstream MIT model and tokenizer files;
`models/int8.json` pins the converted GGUF size and SHA-256. The conversion packs
self-attention Q/K/V rows into one projection and cross-attention K/V rows into
one projection. Block quantization is along complete rows. Biases, normalization,
softmax and residual activations remain floating point; this is not an entirely
integer arithmetic network or SmoothQuant.

The FP32 weight file is 1,330,973,772 bytes. The INT8 file is **354,200,608 bytes**
(73.4% smaller). Conversion is a separate maintainer release step in Rust.
Loading verifies the INT8 hash and reads quantized tensors directly. Immutable
weights are shared across runners; each runner keeps independent attention caches.

A fresh preparation now downloads **360,332,983 bytes** of prepared INT8 and
tokenizer/configuration files, about 73% less than the old 1,337,106,147 bytes.
The app neither downloads nor converts the original weights. For an offline copy,
place only `config.json`, `vocab.json`, `sentencepiece.bpe.model` and
`model-int8.gguf` inside the selected model directory's `ja-ko` subdirectory.
`models --verify` validates this INT8-only installation without FP32 weights.
No model data is committed to Git or included in the application executables.
See [direct INT8 preparation and minimal build](direct-int8-download.md).

## Research and applied changes

The following papers were inspected, including the full text of Jacob et al.,
Pope et al. and TVM. Published benchmark numbers belong to their own models and
hardware and are not claimed as this application's speedups.

| Source | Finding and application |
| --- | --- |
| [SMaLL-100, Mohammadshahi et al. (2022)](https://arxiv.org/abs/2210.11621) | The shallow decoder reduces autoregressive work. Retain the trained 12-encoder/3-decoder architecture and translate directly to Korean. |
| [Quantization and Training of Neural Networks for Efficient Integer-Arithmetic-Only Inference, Jacob et al. (2018), §§2–4](https://arxiv.org/abs/1712.05877) | Low precision trades storage/latency against accuracy; threading overhead depends on workload. Apply existing Candle Q8_0 kernels to persistent weights, and measure rather than assume benefit. This implementation does not reproduce the paper's affine QAT scheme. |
| [Efficiently Scaling Transformer Inference, Pope et al. (2023), §2](https://arxiv.org/abs/2211.05102) | Prefill and autoregressive decoding have different compute/memory costs. Keep decoder/cross-attention KV caching, share INT8 weights and measure bounded sentence concurrency separately from model loading. TPU partitioning results are not transferred to this CPU app. |
| [TVM, Chen et al. (2018), §3](https://arxiv.org/abs/1802.04799) | Reduce dispatch and redundant memory traffic by combining compatible operators. Pack Q/K/V projection weights once and run one matrix operation instead of three; combine cross-attention K/V too. TVM is not a runtime dependency, and general TVM graph/epilogue fusion is not implemented. |
| [SmoothQuant, Xiao et al. (2023)](https://arxiv.org/abs/2211.10438) | Activation outliers need calibration and weight/activation transformations. Defer W8A8 SmoothQuant rather than applying an uncalibrated transform to the trained translation model. |
| [Fast Transformer Decoding: One Write-Head is All You Need, Shazeer (2019)](https://arxiv.org/abs/1911.02150) | Multi-query attention changes the trained architecture. Do not replace SMaLL-100's multi-head attention or alter its weights to claim a cache speedup. |

Candle 0.11.1 has a process-global quantized-kernel barrier pool independent of
Rayon pools. Its concurrent callers share a lock. The CLI sets
`CANDLE_NUM_THREADS=1` **before creating threads**, preventing host-wide nested
parallelism and global kernel-pool contention. Private Rayon pools and at most
four sentence runners divide the configured CPU budget (capped at eight).
Library embedders should set this environment variable at process launch; the
library does not mutate process environment while application threads run.

Workers claim the next input and a distinct preallocated output slot under a
short lock, then release the lock before inference. This balances variable-length
lines and empty lines while keeping exact source order. Every worker is joined
on failure; missing results prevent saving. User correction memory is unchanged.

Automatic mode measures serial and parallel execution on four distinct lines
from the first sufficiently large body batch, compares complete outputs, and
uses parallel only if it is at least 10% faster. It retains the decision for the
job. A higher worker count is not a promise of better throughput on every CPU.

## HTTPS certificate repair

Windows uses native TLS/Schannel and the Windows trusted certificate store.
Linux uses rustls with native system roots. All requests share this policy,
including model preparation and novel downloads. `ca_bundle` in settings or
`--ca-bundle certificates.pem` adds an administrator-provided PEM CA bundle;
`SSL_CERT_FILE`, `REQUESTS_CA_BUNDLE` and `CURL_CA_BUNDLE` are fallback choices.
TLS chain/hostname verification stays enabled. An unknown issuer produces a
specific instruction to fix the trust store or provide the additional CA.

Tests check that an untrusted local HTTPS certificate is rejected and the same
kind of endpoint with an explicitly trusted CA succeeds. The Windows CI also
downloads and hash-checks only the pinned 890-byte Hugging Face configuration,
exercising Schannel against the actual service without fetching model weights.

## Build and release scope

The production library is an `rlib`; a compatibility DLL is no longer produced
by default. Legacy recovery inspection, glossary/EHND and DLL-loading code are
behind `--features legacy` and tested in a separate job. The GUI enables only
its required backend features. Some upstream Candle/GUI transitive dependencies
remain required by their published crates; they are not separate release files.

The Windows artifact is checked against an exact eight-file allowlist: two
application executables, BAT launcher, usage guide, notices and three licenses.
PE imports are checked for undocumented runtime libraries. No vendor DLLs,
Python/Java/CUDA runtime, Cargo dependency source or model weights are bundled.
Windows/MSVC runtime dependencies are documented when packaging a release.

CI runs Linux production tests, Windows production tests, lint and legacy
checks concurrently. Tests and shipping binaries share the release dependency
profile. Rust caches are separated by platform/purpose; superseded runs cancel,
and branch-push/PR duplicate runs are avoided. The always-running, non-blocking
live-novel builds, compatibility DLL export step and duplicate CLI/DLL upload
are removed. Uncompressed artifact upload avoids redundant compression before
the final release ZIP. Release staging fetches only its tag and packages the
exact verified source; draft-only and source/CI/artifact matching checks remain.

## Verification

Reproduce microbenchmarks with the `benchmark_offline` example and the explicit
`offline_model` integration test described in [offline-engine.md](offline-engine.md).
For the 35-chapter work, use its locally cached original HTML with
`download --html ... --episode-html-dir ... --start 1 --end 35 --japanese`, then
run `scripts/validate_kakuyomu_bilingual.py`. Timing, resident-memory and
completeness results are recorded in the change's pull request. Private novel
text and full model files are not uploaded. Hash equality checks output
consistency, not semantic translation accuracy. Small-model fiction/name/idiom
translation still needs human review.
