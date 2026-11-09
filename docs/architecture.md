# Rust module boundaries and optimization

The maintained Rust application has thin entry points and a shared library.
Domain modules own their state; frontend code calls their public facades.
The public `config`, `models`, `learning`, `jobs`, `offline`, `int8` and optional
legacy import paths remain available. Third-party Candle kernels and model
weights are unchanged.

| Folder | Responsibility |
| --- | --- |
| `src/cli/` | Typed command dispatch, validated arguments, backend construction and command orchestration |
| `src/novel/site/` | URL validation, shared DOM extraction, site adapters, index completeness, chapter parsing |
| `src/novel/network.rs` | Bounded website fetching with the shared TLS client |
| `src/translation/` | Translator trait, backends, filtering, bounded file translation and output formatting |
| `src/translation/filter/` | Literal dictionary parsing, character handling, protected replacements and Korean particle selection |
| `src/translation/session/` | Private session state, subprocess ownership and bounded JSON protocol |
| `src/offline/` | Builder, CPU budget, policy, shared bounded result cache and duplicate batch restoration |
| `src/offline/runner/` | Sentence generation, repetition guard and borrowed-logit selection |
| `src/offline/model/` | Model configuration, transformer layers and isolated CPU operations |
| `src/models/` | Cached manifests, portable discovery, verification and resumable download transport |
| `src/int8/` | Quantized weights and metadata; exporter enabled only by `model-tools` |
| `src/learning/` | Compressed defaults, correction storage, source-qualified terms and dictionary import |
| `src/jobs/` | Desktop queue, process RAII and output/progress event reduction |
| `src/config/` | Application settings and independent Java-properties syntax |
| `src/storage/` | Atomic streaming transactions, text decoding and shared SHA-256 |
| `src/legacy/` | Explicitly enabled recovery/compatibility code, outside production builds |
| `src/bin/desktop/` | Desktop state, individual views, widgets and font loading |
| `tools/release_package/src/` | Separate maintainer crate: source validation, model fetching, archive packaging and digest verification |

```mermaid
flowchart TD
  CLI[CLI entry] --> Commands[Typed commands]
  GUI[Desktop entry and views] --> Queue[Job queue]
  Queue --> CLI
  Commands --> Novel[Novel parsing and fetching]
  Commands --> Translation[Translation sessions]
  Translation --> Offline[Offline runners and model]
  Offline --> Models[Verified model assets]
  Offline --> Learning[Correction snapshot]
  Novel --> Storage[Storage and shared TLS]
  Translation --> Storage
  Models --> Storage
  Learning --> Storage
```

## Patterns applied

- **Builder and newtype:** `OfflineTranslator::builder(path)` requires the model
  location. `CpuBudget` contains a nonzero budget capped at eight; policy uses
  `ParallelPolicy`, rather than interchangeable integers/strings inside inference.
  Existing `open`/`open_with_policy` APIs delegate to the same construction path.
- **Ownership and RAII:** `AtomicFile` owns a unique `create_new` temporary file.
  Writing proceeds in bounded chunks; `commit(self)` consumes the transaction.
  Failure/drop closes and removes its temporary file, preserving the destination.
  Worker/desktop process guards retain their kill/wait/cleanup behavior.
- **Enums and composition:** commands use exhaustive enum dispatch. Session
  variants are private, so consumers cannot create an empty worker pool.
  Frontend views compose shared configuration, queue and correction services.
- **Borrowing:** greedy decoding scans the existing contiguous CPU logits under
  a storage guard; it does not allocate/copy a vocabulary-sized float vector per
  generated token. Tokenized pieces retain their IDs instead of being encoded twice.
- **Scoped unsafe:** the application denies unsafe code by default. Explicit
  exceptions are the two adapted CPU kernels and optional vendor ABI boundaries.
  Both entry points forbid unsafe code.
- **Concurrent state:** workers own attention caches; weights/tokenizer/memory
  snapshots are shared immutably. Result-cache mutation stays with the coordinator.
  Progress reporting treats poisoned locks as errors and remains monotonic.

The design follows the [Builder](https://rust-unofficial.github.io/patterns/patterns/creational/builder.html),
[RAII guards](https://rust-unofficial.github.io/patterns/patterns/behavioural/RAII.html),
[newtype](https://rust-unofficial.github.io/patterns/patterns/behavioural/newtype.html)
and [unsafe-boundary guidance](https://doc.rust-lang.org/book/ch20-01-unsafe-rust.html).
These patterns express actual invariants; no training/GPU/framework dependencies
are added for architecture alone. Standard Deflate uses the existing GUI graph's
`miniz_oxide` version, now directly used for compressed correction data.

## Performance and compatibility

- A single 1,024-entry result cache replaces four separate 512-entry caches.
  Oversized entries are excluded. A pending batch translates each unique source
  once, then restores every original position, duplicate and empty line.
  Workers claim longer sources first to reduce end-of-batch idle time; result
  slots and error line numbers continue to refer to original input order.
- Greedy selection preserves finite-score filtering, special-token exclusions,
  repetition bans, total float ordering and the lower-ID tie rule. Membership in
  the repetition set is checked only for a possible new winner.
- Tokenizer decoding uses dense token-ID lookup. Decoder input vectors are reused.
- Site selectors are cached per parsing thread. Static title/filename regular
  expressions and immutable model manifests are initialized once. Non-paginated
  sites avoid an extra DOM parse; first-page embedded data is reused.
- Chapter output is streamed into an uncommitted transaction. Text-file translation
  retains the decoded input but only translates/stores 128-line chunks in memory.
- Large correction lists render visible rows. Configuration text fields borrow
  their strings instead of cloning them on every frame. Queue events locate the
  active job once per poll rather than once per log line.

The regression suite checks UTF-8/UTF-16/legacy encoding, final-newline behavior,
bilingual line alignment, failures preserving output, worker initialization,
queue snapshots, TOC completeness, serial/parallel consistency and user edits.
Full-work speed comparison uses the same pinned INT8, four workers, fixed parallel
policy, cached 35-chapter HTML and `--no-learning`; correction changes are tested
separately. See [the validation record](refactor-validation.md) for measurements;
source-matched Windows/Linux CI is attached to the PR.

## Default paths and corrections

On Windows, the default download folder is `C:\Users\rabbit\OneDrive\문서`.
An explicit output file or `--output-dir` takes precedence. `download_dir` in
settings overrides the default; the save dialog and automatic filenames use it.
Other platforms retain the current-directory default.

Default correction data is Deflate-compressed into the executable. It contains
60 authored sentence examples, eight reviewed names from the supplied dictionary,
and six source-qualified spelling corrections verified against the pinned model.
There is no initial correction download. Existing user corrections take precedence;
the persisted initialization flag prevents deleted defaults from reappearing.
New profiles use `corrections.json.zlib`; existing JSON profiles remain readable.
`--no-learning` uses an empty correction snapshot. This adaptation is translation
memory, not neural-weight training or a guarantee of fluent output for unseen text.

See [the supplied-resource audit](resource-audit.md) for import scope and exclusions.
