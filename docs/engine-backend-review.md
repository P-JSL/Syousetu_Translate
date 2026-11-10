# CTranslate2, ONNX and full Rust ezTrans alternatives

Reviewed on 2026-11-10 after publishing
[v0.3.5](https://github.com/P-JSL/Syousetu_Translate/releases/tag/v0.3.5).
**The tested CTranslate2 CPU configurations did not improve complete-work
time, and the 90% target remains unmet.** The published Rust/Candle engine
is retained. Full ezTrans core reconstruction is assessed in
[eztrans-rust-feasibility.md](eztrans-rust-feasibility.md).

## What was actually tested

The supplied [prepared INT8 checkpoint](https://huggingface.co/solavr/small100-ctranslate2-int8)
was downloaded at revision
`3c5af8e17d5c4f35bda849b7ffceaa1dfc62eb84`. All four runtime SHA-256 values
matched. Its base revision `8ab680e26a596d2e3d2d2d17ae0f68df1037328c` matches
the source checkpoint of the current SMaLL-100 model. No original FP32
checkpoint, Python runtime or local model conversion was used.

The test used a Rust host, the pure Rust SentencePiece tokenizer and
`ct2rs 0.10.4` calling CTranslate2 4.8.5 with oneDNN 3.14.1. Logs confirmed
`int8_float32` and DNNL INT8 GEMM, without an FP32 fallback. CUDA and MKL were
disabled. CTranslate2 and oneDNN are C++; a Rust binding does not make their
engine a pure Rust implementation. The manually invoked
[comparison crate](../tools/inference_bench/README.md) is isolated from the
application dependency graph and release packaging.

Both engines used beam 1, trigram-repeat blocking, the full base vocabulary,
no correction-memory reuse and the same locally cached HTML. SMaLL-100's
target token `__ko__` was prepended to the **encoder input**, followed by source
pieces and EOS; no extra forced Korean decoder prefix was added. Long sources
were split at 192 pieces using the application's Unicode boundary helper.
The prose postprocessor, original-row order and live TXT writer were reused.

## Complete-work results

The host has an AMD EPYC 9V45 with a four-CPU quota and no exposed GPU. Times
include a new process, SHA verification/model loading, HTML parsing, header and
body translation, and final TXT flush/fsync. No CPU build or other model test
ran during timing. The OS file cache was not cleared.

| Implementation/configuration | All 35 chapters | All 24 chapters |
| --- | ---: | ---: |
| Released Rust/Candle, three-run median | **48.5434 s** | **70.6891 s** |
| CTranslate2, 1 replica × 4 threads, AVX2, 128 input rows | 91.2752 s | Not run |
| CTranslate2, 4 replicas × 1 thread, AVX512, 1,024 input rows | 84.9468 s | 101.5905 s |
| 90% threshold against the recorded v0.3.2 references | 16.3111 s | 26.3902 s |

Both CTranslate2 settings used a maximum of 128 examples per native batch.
The second follows the official advice to consider more inter-batch workers
with a larger input window while keeping the compute budget at four CPUs.
It changes several settings together, so its difference is not attributed to
any one setting. Compared with the released medians, the second candidate
took **1.75× / 1.44×** as long. Its peak RSS was about 2.67/2.51 GiB, versus
0.76 GiB for the first 35-chapter candidate.

Every completed candidate output passed the independent verifier: chapter
count/order, original titles, every source row, nonempty translations and
paragraph boundaries. The 35-chapter outputs contain all 6,248 source rows;
the 24-chapter output contains all 17,041. This is **structural validation**,
not a translation-accuracy certificate. Different output hashes are expected
after changing the runtime and quantization scheme.

These are single exploratory runs per configuration. The probe parses the
selected corpus eagerly and translates fixed input windows, while the released
application uses bounded prefetch and refills active sentence contexts. The
probe rejects generation-limit output instead of implementing the application's
split-and-retry recovery. These results do not establish a universal runtime
ranking, multiple-PC average or GPU performance. They do establish that these
tested alternatives are not evidence of meeting this application's 90% gate.

Exact wall times, RSS, model revisions, hashes, source counts and authored
quality examples are in [engine-backend-experiments.json](engine-backend-experiments.json).
The timing samples preceded promotion of the local Rust probe into the isolated
tool; the report identifies the measured prototype separately from the
packaged research source. Novel content, vendor data and model weights remain
outside Git and release assets.

## Quality checks and model size

The same 26 authored Japanese cases were translated by both implementations.
Of the seven specific dialogue regression checks already used by the
application, the released backend passed 7 and this CTranslate2 candidate
passed 5. These checks are limited, source-specific predicates rather than a
general fluency score. Two examples explain the rejection:

| Source | Released backend | CTranslate2 candidate |
| --- | --- | --- |
| `「明日までに読んどいてくれない？」` | `“내일까지 읽어 놔 줄래?”` | `“내일까지 읽어주십시오?”` |
| `「ねえ、そんな顔しないでよ。」` | `“그런 얼굴을 하지 마.”` | `“그렇지 않아, 그런 얼굴을 써라.”` |

The latter reverses the negative request's intent. Other cases expose existing
limitations in both engines: for example, neither rendered `陛下` correctly in
the authored honorific sentence. Neither engine is certified as generally
natural or equivalent to ezTrans. Generated outputs were not added to learning
memory as approved corrections.

The prepared CTranslate2 runtime files total **344,075,494 bytes**, compared
with 360,332,983 for the current pinned model files: about **4.5% smaller**.
This is a raw-model comparison, not a measured release-ZIP reduction. It does
not support a claim of dramatically faster downloading. A local Rust metadata
reader verified the model's complete tensor-container framing and observed
row-scale arrays for quantized matrices; its scaling scheme differs from the
current 32-weight Q8 blocks, so identical generated output is not assumed.

## Applying the supplied recommendations

The current engine already uses W8A8 integer matrix products, batch processing,
decoder KV reuse and beam 1. It is not the PyTorch weight-only W8A16 path with
repeated weight casting. Therefore a suggested beam change from 5 to 3/2 is
not an additional optimization here: increasing the existing beam would add
work and require a separate quality/performance tradeoff.

The official [CTranslate2 performance guidance](https://opennmt.net/CTranslate2/performance.html)
recommends INT8, beam 1, length-sorted batches and keeping inter × intra threads
within physical CPU capacity; those recommendations informed the two trials.
Its [quantization documentation](https://opennmt.net/CTranslate2/quantization.html)
also distinguishes stored precision, effective compute type and hardware
fallback. No PyTorch-relative speedup is reused as a measurement of our engine.

The public
[`R4kSo1997/small100-onnx-int8_v2`](https://huggingface.co/R4kSo1997/small100-onnx-int8_v2/tree/d66e2265495ad8062e7289f7b484bebc7c4e9510)
card advertises an ARM64 dynamic-INT8 encoder and merged KV-cache decoder.
Its two graphs total **610,519,252 bytes** before tokenizer files. This exceeds
the current model size substantially; the base revision and on-host output
parity were not established. It was inspected as an alternative, not installed
or timed. ONNX/TensorRT GPU mappings and FP16 performance remain untested on
this CPU-only host.

The application, shipped dependency graph, model and release archive were not
changed for these candidates. There is no new 90%-achieved release. The result
supports keeping v0.3.5 and pursuing a validated pure Rust matrix/architecture
improvement or measured acceleration on available hardware, rather than
switching to this tested C++ runtime or the incomplete glossary engine.
