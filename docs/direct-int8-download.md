# Prepared INT8 download and minimal release build

The portable release contains `model-int8.gguf` (354,200,608 bytes), tokenizer
and configuration in `models/ja-ko` next to the executables. The Rust application
selects this directory automatically regardless of the current working folder.
It runs immediately after extraction with no initial model download or conversion.
Explicit settings/environment paths still take precedence.

For optional repair/nonbundled installation, the Rust application fetches the
prepared model directly from
the versioned `models-small100-int8-v1` model release. Configuration and
SentencePiece/vocabulary files add 6,132,375 bytes. The initial download is
**360,332,983 bytes** instead of **1,337,106,147 bytes** (73.05% less).
There is no FP32 fallback or local model conversion in the shipping build.

`models/downloads.json` pins every runtime asset and its SHA-256. The model
has the same hash and tensor format as the previously validated INT8 file.
Existing valid caches are reused without creating a network client. Partial
downloads retain their bytes and resume with a validated HTTP Content-Range;
servers ignoring Range restart safely. Corrupt files never replace a good file.
The full SHA-256 must match before a temporary file is installed. A public
GitHub API fallback downloads the same pinned asset when the ordinary release
URL fails; users need no GitHub token. Normal certificate/hostname validation
and configured additional CA certificates remain enabled.

All new download, checksum, model loading, CPU operations and release audits are
Rust. No Python/transformers/bitsandbytes/accelerate/CUDA installation is needed.

## Build scope

- Default `offline-model` and optional `gui`: GGUF inference and the actual GUI
  dependencies. The release ZIP contains two executables, BAT, usage guide,
  license notices and four prepared model files (12 files total). Ordinary CI
  artifacts carry only the eight application files. Symbols are stripped;
  thin LTO remains enabled.
- `model-tools`: separate maintainer exporter. It downloads source weights on
  the build machine, prepares the reproducible INT8 release, and checks its
  pinned hash. It is never enabled by shipping builds or included in the ZIP.
- `legacy`: recovery/DLL/glossary tools, tested independently and excluded from
  shipping builds.
- Candle core's optional `tokenizers`, `archive` and original-weight `safetensors`
  adapters are disabled in production. Only `model-tools` enables safetensors. A
  small, documented feature patch keeps CPU/quantized kernels intact. Full
  Candle NN is replaced with only its two required F32 CPU kernels in `src/nn.rs`.
- `tools/release_audit.rs` uses only Rust std. CI inspects the *normal*
  production Cargo graph and rejects tokenizer, archive, legacy regex, training
  and GPU crates. It also verifies the exact release file list and PE imports.

The model build and full Windows download/portable ZIP tests run only on explicit
workflow dispatch. Ordinary CI downloads no model weights. The Windows test reuses its
already built CLI artifact, downloads into a fresh folder, verifies all four
files, confirms offline cache reuse, tests automatic bundled-model selection
from another working folder and runs direct Japanese→Korean inference.
It needs no application rebuild and uploads no duplicate model data.
The `portable-smoke` job tests the staged release ZIP itself: CRC, SHA-256,
the exact CI executables and all four model assets, followed by automatic
model selection and offline inference. Its verifier builds archive dependencies
only, without the HTTP, GUI or inference libraries.

The separate Rust `tools/release_package` executable streams model files into
the portable ZIP with Deflate and checks CRC/file count/source/provenance.
Its archive/HTTP crates are maintainer-only; they never enter the app graph.
Release workflow code is Rust. Checkout/upload/download Actions use Node.js 24,
and Linux jobs pin Ubuntu 24.04 to avoid automatic image-migration notices.

Model provenance, original MIT license, transform description and checksum
ship with the model release; see [the model card](../models/MODEL_CARD.md).
