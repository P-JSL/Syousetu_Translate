# Third-party notices

The application implements inference in Rust. Portable releases include the
prepared MIT-licensed INT8 model; a Rust downloader supports optional recovery.
It does not redistribute ezTrans XP or its proprietary engine assets.

* **SMaLL-100**, Alireza Mohammadshahi, Vassilina Nikoulina, Alexandre Berard,
  and Laurent Brun (2022), is the default direct Japanese→Korean model.
  [Model](https://huggingface.co/alirezamsh/small100) ·
  [source](https://github.com/alirezamshi/small100) ·
  [MIT license](licenses/MIT-SMaLL100.txt).
  The pinned safetensors revision and SHA-256 values are in `models/manifest.json`.
  A separate Rust maintainer exporter prepares persistent INT8 Q8_0 GGUF
  (`models/int8.json`), with fused Q/K/V projections. The app downloads this
  prepared model directly using `models/downloads.json`; quantization changes
  numerical precision and may change generated translations.
* **Hugging Face Transformers**, Apache-2.0, supplies the reference M2M100
  architecture and language-token specification. `src/offline/model/mod.rs` is a Rust
  implementation adapted from the v4.46.3 reference, with Q8 matrix operations,
  independently owned attention caches, and CPU inference only.
  [Reference](https://github.com/huggingface/transformers/tree/v4.46.3/src/transformers/models/m2m_100).
* **Hugging Face Candle 0.11.1**, Apache-2.0, supplies tensor operations and
  quantization. Local patches in `vendor/candle-core/PATCHES.md` make unused
  adapters optional, schedule batched CPU work on a bounded pool, specialize Q8
  register blocks, and vectorize activation quantization while preserving its
  scalar rounding/scales. Two-tile Q8 and exact full-vocabulary tile reduction
  additionally preserve per-output group/FMA order. `src/offline/model/ops.rs` adapts only Candle NN's F32 CPU layer norm and softmax,
  preserving the original reduction order; the full Candle NN crate is not a
  production dependency. **sentencepiece-rs 0.2.2**, Apache-2.0, supplies pure Rust
  SentencePiece normalization/BPE processing. Python and PyTorch are used only
  for development comparison and are not application dependencies.
* **P-JSL/gacha_pickup_calculator**, copyright 2026 P-JSL, MIT, commit
  `762038180135a2adcaf941f899e78db4fcee89aa`, supplies the reference pattern for
  bounded CPU budgets, independent mutable work slices, immutable shared data,
  ordered joining, and serial/parallel consistency checks. `src/offline/ordered.rs`
  adapts this pattern to text results; no probability, FFT, or Monte Carlo
  algorithm is used for translation.
  [Reference](https://github.com/P-JSL/gacha_pickup_calculator/blob/762038180135a2adcaf941f899e78db4fcee89aa/src/domain/feedback/runner.rs) ·
  [MIT license](licenses/MIT-gacha-pickup-calculator.txt).

The Apache-2.0 license is included in [licenses/Apache-2.0.txt](licenses/Apache-2.0.txt).
Other Rust dependencies and their licenses are recorded in Cargo.lock and their
respective crates. This project itself remains MIT licensed.

The new 112-entry speech snapshot and source-qualified prose rules are
project-authored. Research citations and implementation decisions, including
which algorithms were not ported, are in `docs/inference-research-2026-11.md`.
No extra model, learned drafter or third-party training dataset is bundled.
