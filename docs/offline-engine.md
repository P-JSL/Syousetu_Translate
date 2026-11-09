# Direct offline translation (0.3.1)

The default `internal` engine runs **SMaLL-100 INT8** directly from Japanese to Korean
in Rust. The source-visible recovery client is retained; neither ezTrans XP's
proprietary core nor an English pivot is used by this engine.

## Prepare and run

```sh
cargo build --locked --release --features gui --bins
target/release/syousetu-translate models --prepare
target/release/syousetu-translate models --verify
target/release/syousetu-translate translate --text '私は学生です。'
target/release/syousetu-translate download \
  --url https://kakuyomu.jp/works/11054822663814504924 \
  --start 1 --end 1 --out novel.txt --japanese --jobs 4
```

On Windows append `.exe`, or use the portable BAT/GUI package. The portable ZIP
already includes the model and automatically selects the sibling `models` folder.
Optional preparation for a nonbundled install or repair downloads
the prebuilt INT8 model directly: **360,332,983 bytes** including small tokenizer
and configuration files. `models/downloads.json` pins the runtime URLs, sizes
and SHA-256 values. `models/manifest.json` records upstream source provenance
for the separate maintainer exporter. The shipping app never downloads FP32
weights or performs conversion. The prepared GGUF is pinned in `models/int8.json`.
Existing assets are verified before reuse, partial files support HTTP Range
resume, and concurrent preparation is locked. A GitHub API asset fallback
handles public-download routing failures without requiring a token. Both paths
must match the same immutable model hash.

`--model-dir PATH`, `SYOUSETU_MODEL_DIR` or the `model_dir` setting can select a
different local directory. Defaults are under `%LOCALAPPDATA%\SyousetuTranslate`
on Windows and `$XDG_DATA_HOME/SyousetuTranslate` (or `~/.local/share`) on Linux.
The portable release includes the model in `models/ja-ko`. Model inference uses no network
client; a saved-HTML or local-file translation can run with networking disabled.
Live novel downloads still need website access. Allow about 1GB of working RAM;
immutable INT8 weights are shared. No original-weight conversion memory is used.
`download --cache-html-dir PATH` saves the parsed work index as `work.json` and
each original chapter as `1.html`, `2.html`, etc., for private local verification.

## Lightweight adaptation

The **교정 기억** tab stores approved corrections in `corrections.json.zlib`.
The executable embeds compressed defaults: 60 sentence examples, eight reviewed
names and six source-qualified spelling corrections. Existing JSON profiles are
still supported. User edits win, and deleted defaults stay deleted after reload.
Sentence entries match the trimmed original line/segment exactly. Terminology
entries contain a Japanese source term, an existing Korean expression and a
corrected expression. A term qualifies only when its Japanese source is present;
qualifying replacements run once with longest-match precedence, without chaining.
Exact sentence corrections take precedence over generated output.

```sh
target/release/syousetu-translate memory --learn \
  --source '私は学生です。' --corrected '저는 학생입니다.'
target/release/syousetu-translate memory --learn \
  --source 'アリス' --previous '앨리스' --corrected '아리스'
target/release/syousetu-translate memory
target/release/syousetu-translate memory --forget --source '私は学生です。'
```

Forgetting a term also requires `--previous`. `--memory-path PATH` overrides the
file; `--no-learning` disables corrections. The GUI supports create/edit/delete
and reload. Copy the correction file to back it up. Edits lock, re-read and merge the
latest file before atomic replacement. Invalid files are preserved rather than
silently reset. Limits are 5MB decompressed/25,000 entries. A job reads a snapshot when it
starts; edits affect subsequent jobs. This is translation memory, **not neural
weight training**. Generated output is only a bounded, in-process cache and is
never persisted as verified learning data. Legacy placeholder TSV dictionaries
are not fed through the neural model.

## Parallelism and merge consistency

The reference is `P-JSL/gacha_pickup_calculator` commit
`762038180135a2adcaf941f899e78db4fcee89aa`, especially `feedback/runner.rs` and
`app/compute.rs`. The adapted parts are:

* A single CPU budget, capped at eight, divided between at most four active runners.
* Shared immutable quantized weights; each runner owns its decoder/cross-attention
  caches. KV state resets after every segment, including failed generation.
* Disjoint preallocated output slots and ordered joins; completion order cannot
  reorder lines. Empty and duplicate lines are retained.
* Every submitted task is joined on error. Missing/failed results prevent saving;
  existing output remains intact through the atomic writer.

Probability convolution, FFT, random seeds and Monte Carlo aggregation are not
translation algorithms and were not copied. Neural matrix operations are
memory intensive, so more workers do not guarantee higher throughput.

Default `parallel_mode=auto` measures the first sufficiently large body batch
(at least 16 untranslated Japanese lines, four distinct calibration samples).
It compares the complete sample outputs and chooses parallel only when they
match exactly and parallel is at least 10% faster. Otherwise it retains serial
execution. The choice is reused for the job; calibration does not alter model
weights or user corrections. Short batches use serial. A CPU setting is a maximum,
not a requirement to occupy every core.

Use `--parallel-mode serial|parallel` for a controlled comparison. The GUI exposes
the same three modes. Explicit parallel mode remains bounded by `--jobs`.

```sh
SYOUSETU_MODEL_DIR=/path/to/models cargo run --locked --release --example benchmark_offline
SYOUSETU_MODEL_DIR=/path/to/models cargo test --locked --release --test offline_model -- --ignored --nocapture
SYOUSETU_MODEL_DIR=/path/to/models cargo test --locked --release --lib tokenization_matches_official -- --ignored
```

The benchmark excludes model verification/loading, warms up separately, compares
the entire ordered output vector, and prints output SHA-256 and timing. Results
depend on hardware and workload. Never infer semantic translation accuracy from
matching hashes alone.

## Translation and error boundaries

For a complete saved Kakuyomu novel with `--japanese` and the default title
format, independently check chapter order and every original/translation pair:

```sh
python scripts/validate_kakuyomu_bilingual.py --source-dir /path/to/cached-html \
  --output novel.txt --chapters 35 --report completeness.json
```

This checker compares all original lines, including empty/duplicate lines and
ruby handling, rejects missing translations and extra body lines, and writes
counts/hashes without republishing novel text. It does not certify translation
meaning or literary quality.

The tokenizer uses the model's SentencePiece BPE and shared vocabulary ids. The
target Korean prefix is `128052`; decoding begins with EOS, as in SMaLL-100.
Inference uses pre-normalized encoder/decoder layers, offset sinusoidal positions,
shared Q8_0 embeddings/output head, greedy decoding, special-token suppression
and repeated-trigram blocking. Long input is split at Unicode-safe boundaries,
preferring punctuation. A generation length failure retries smaller source
segments with a bounded recursion depth. Input is never silently truncated to
the model limit; unrecoverable generation fails without saving partial output.

Q8 conversion and decoding guards can change results relative to the original
floating-point, five-beam model. This small multilingual model can mistranslate
fiction, names, slang and ambiguous dialogue. User corrections improve preferred
expressions, not the model's general language understanding. No ezTrans output
equivalence or publication-level translation accuracy is claimed.

Internal inference runs once in the CLI process for the whole job, outside the
old 30-second DLL worker protocol. It does not spawn a model per CPU. The legacy
worker protocol now reports initialization failure in a structured ready/error
response and distinguishes process termination from a response timeout.

The optional `native` glossary and compatibility ABI require `--features legacy`; their small dictionary is not the neural engine. See
[THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md) for model/code licenses.

See [INT8 performance research and validation](int8-performance.md) for fused
attention projections, kernel thread limits, TLS trust and CI changes.
