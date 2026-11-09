# Local feature-only patch

Upstream: Hugging Face Candle `candle-core` 0.11.1,
commit `59f8e903ca80a32ea7f8514b39748fc979e69e82`.
Crates.io archive SHA-256:
`d11c8be87cfbaef9ebb62fb3f850d1fc8e00cc1d11d393fdbe55a4d5c2cdb325`.
The upstream Apache-2.0 license is retained in `LICENSE`.

Changes are limited to Cargo feature gates:

- `tokenizers` enables the optional Hugging Face tokenizer dependency and its
  `quantized::tokenizer` adapter. The app uses `sentencepiece-rs` instead.
- `archive` enables ZIP and the optional NPY/NPZ/Pickle adapters and ZIP error
  variant. The app uses GGUF; the maintainer exporter uses safetensors.
- `safetensors` enables the original-weight reader, its memory-map/yoke support
  and associated error variants. Only the separate `model-tools` build enables it.

Tokenizer/archive features are disabled in all builds. The safetensors feature
is disabled in application builds and enabled only for model export. CPU tensor
operations, numerical kernels, quantization, GGUF reading and attention math
are unchanged. The application no longer depends on the full Candle NN crate.
Upstream examples, tests and benchmarks are omitted from this source-only copy.
