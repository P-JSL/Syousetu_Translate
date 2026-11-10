# Local feature gates and CPU inference patches

Upstream: Hugging Face Candle `candle-core` 0.11.1,
commit `59f8e903ca80a32ea7f8514b39748fc979e69e82`.
Crates.io archive SHA-256:
`d11c8be87cfbaef9ebb62fb3f850d1fc8e00cc1d11d393fdbe55a4d5c2cdb325`.
The upstream Apache-2.0 license is retained in `LICENSE`.

Cargo feature gates:

- `tokenizers` enables the optional Hugging Face tokenizer dependency and its
  `quantized::tokenizer` adapter. The app uses `sentencepiece-rs` instead.
- `archive` enables ZIP and the optional NPY/NPZ/Pickle adapters and ZIP error
  variant. The app uses GGUF; the maintainer exporter uses safetensors.
- `safetensors` enables the original-weight reader, its memory-map/yoke support
  and associated error variants. Only the separate `model-tools` build enables it.

Tokenizer/archive features are disabled in all builds. The safetensors feature
is disabled in application builds and enabled only for model export. The
application no longer depends on the full Candle NN crate.

CPU inference changes in `src/quantized/`:

- Large batched matrix tiles and activation quantization use the caller's
  bounded Rayon pool. Small/single-row work retains the upstream path.
- Q8_0 AVX-512 row kernels specialize register blocks of one to eight rows;
  larger blocks are reserved for large output matrices. Dot-product block order,
  signed-offset correction, scales and FMA order are retained.
- `quantize_x86.rs` vectorizes Q8K activation quantization with AVX2. It retains
  first signed-maximum selection, round-away-from-zero ties, clamping and block
  sums. Nonfinite/overflow cases fall back to `quantize_q8k_reference`, the
  unchanged scalar algorithm. CPU feature detection gates every SIMD entry.
- `repack_x86/dual_q8.rs` shares activation broadcasts between two adjacent
  output tiles, retaining independent output accumulators and group/FMA order.
- `cpu_top1.rs` and `repack_x86/top1.rs` expose an exact full-vocabulary greedy
  reduction. It retains EOS, finite filtering, repetition bans, original-ID
  ties and `f32::total_cmp`, and falls back on unsupported CPUs. Local tile
  winners avoid materializing the full output-score tensor. The application
  compares both paths and selects by measured CPU cost, not a heuristic shortlist.

GGUF format, prepared model bytes, attention math and quantization policy are
unchanged. `tests/quantized_cpu.rs` checks exact blocks/scales/sums against the
scalar reference, including ties, subnormals and nonfinite input. Actual-model
serial/batch tests and saved-novel hashes check the application path. These
checks cover the tested CPUs/workloads; they are not a cross-platform
floating-point identity guarantee.
Additional matrix tests change later rows and their order while checking earlier
score bits; fused selection is checked against the full matrix on row/tile tails,
ties and blocked winners. Integer range and row-locality reasoning, including
why low-precision Strassen was not substituted, is in
`docs/inference-research-2026-11.md` at the repository root.
Upstream examples, tests and benchmarks are omitted from this source-only copy.
