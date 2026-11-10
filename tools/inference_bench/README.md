# Isolated inference comparison

This manually invoked research crate is **not** an application dependency,
workspace member, CI requirement or release asset. Its host and tokenization
code are Rust; CTranslate2 and oneDNN are third-party C++ runtimes. It must not
be described as a pure Rust engine or a replacement for ezTrans.

The tested CPU build uses `ct2rs 0.10.4`, bundled CTranslate2 4.8.5 and
`onednn-src 0.1.23` (oneDNN 3.14.1). CUDA, MKL, HF Hub, native SentencePiece,
audio processing and the optional Whisper feature are disabled. The pure Rust
`sentencepiece-rs` tokenizer is used. Only the model runtime files listed in
[`src/model.rs`](src/model.rs) are accepted, with pinned SHA-256 checks.

## Build on Linux

Install a C++17 compiler, Make and CMake. The measured build used GCC 14,
CMake 3.31.6 and Rust 1.99.1. From the repository root:

```bash
source scripts/cloud_env.sh
export CMAKE_TOOLCHAIN_FILE="$PWD/tools/inference_bench/native-options.cmake"
export CMAKE_BUILD_PARALLEL_LEVEL=4
export CMAKE_PARALLEL=4
cargo build --release --locked --manifest-path tools/inference_bench/Cargo.toml
```

The toolchain file limits oneDNN to inference MATMUL/REORDER and disables its
graph compiler. Native CTranslate2 still builds its other required operators.
The build and runtime were tested on Linux CPU; Windows/GPU support is not
certified by this tool's results.

Download the four prepared files from
[`solavr/small100-ctranslate2-int8` at the pinned revision](https://huggingface.co/solavr/small100-ctranslate2-int8/tree/3c5af8e17d5c4f35bda849b7ffceaa1dfc62eb84)
into one local directory. The model's `LICENSE` and `NOTICE` are available in
that repository. No original FP32 checkpoint, conversion or Python process is
used by this tool. It never writes translation memory.

## Run

An authored UTF-8 sentence file can be translated to JSON Lines:

```bash
target/release/syousetu-bench-text MODEL_DIR SENTENCES.txt 4 128
```

For the complete cached-HTML novel test, the arguments are model directory,
work URL, index HTML, chapter HTML directory, chapter count, output TXT,
threads per replica, batch size, optional replicas and source-window rows:

```bash
target/release/syousetu-bench-novel MODEL_DIR WORK_URL INDEX.html EPISODES_DIR 35 OUTPUT.txt 4 128
target/release/syousetu-bench-novel MODEL_DIR WORK_URL INDEX.html EPISODES_DIR 35 OUTPUT.txt 1 128 4 1024
```

Chapter files are numbered `1.html` through the selected last chapter.
URLs identify the locally parsed work; this tool does not fetch chapter pages.
`CT2_VERBOSE=1` prints the effective compute type/backend. On hardware verified
to support it, `CT2_FORCE_CPU_ISA=AVX512` enables the second tested ISA setting;
it is not a portable application default. Threads per replica × replicas
should fit the available CPU budget.

The decoder uses `__ko__ + source pieces + </s>`, beam 1, no-repeat trigram
blocking and the complete base vocabulary, without an additional forced Korean
decoder prefix. Long inputs use the application's Unicode splitting helper
at 192 source pieces. Results preserve original row order and use the same
source-aware prose postprocessor and live TXT writer as the application.
Original lines accompany translations for independent structural validation:

```bash
cargo run --release --locked --no-default-features --example verify_saved_novel -- EPISODES_DIR OUTPUT.txt 35
```

This research backend does not opt into the application's `independent_rows`
contract: bitwise invariance to batch-boundary changes has not been established
for the native runtime. The explicit manual benchmark windows are not a
production backend registration.

The benchmark parses all selected chapters eagerly and translates fixed input
windows. The application instead uses bounded prefetch and refills active
logical contexts. The probe rejects a generation-limit result, whereas the
application also has split-and-retry recovery. It does not certify linguistic
quality or evaluate a release gate. Its timer includes asset verification,
model load, HTML parse, translation and final output flush/fsync; an external
process timer can additionally include process start/exit and sample peak RSS.

Do not run a CPU build or another model test during a timed run. Review
[the measurements](../../docs/engine-backend-review.md) before drawing
performance conclusions. The candidate was not adopted.
