# Chapter batching and inference diagnostics

The source loader and translation coordinator have separate ownership. One
scoped thread owns the existing rate-limited fetcher. It downloads and parses
chapters in order, retaining the configured delays, retry policy and serial
HTTP requests. A bounded channel holds at most 64 parsed chapters; it does not
retain their HTML after parsing. A small chapter owns at most 128 KiB, counting
String/Vec headers and spare capacity as well as text. After submitting a larger
chapter, the producer waits for a lifetime guard to acknowledge its consumption
or cancellation before loading another. Thus the queue contains at most 8 MiB
of small source allocations and one large chapter, rather than 64 large chapters.

The coordinator takes the first available chapter and only collects additional
chapters that are already ready. It never waits to fill a batch. Collection
stops after 64 chapters, 8,192 parsed rows or 8 MiB of estimated source allocation,
possibly including one unusually long final chapter. The active window and the
queue may each hold small chapters; HTML parsing and inference have their own
memory costs. Large-chapter guards remain owned by the active window and release
the producer on success or error. This bounds ordinary source buffering without
adding a new download concurrency setting or waiting for the whole work.

Backends explicitly opt in through `Translator::independent_rows`. The internal
model and passthrough backend qualify. Other backends keep the original
chapter-by-chapter call order. For an independent backend, chapter titles and
bodies share one inference batch. Each source sentence still owns its encoder
state, decoder history, attention caches and original index. No attention or
translation context crosses sentence or chapter boundaries.

`translation/output/chapters.rs` associates source spans with their chapter
numbers. The existing session checks each emitted index against the next
expected index. Only a completed contiguous prefix reaches the final TXT;
headings and completed nonblank rows are flushed immediately. Completion events
follow saved chapter boundaries. A download error after ready chapters is
deferred until that ready prefix has been translated and flushed. Translation
or output errors propagate while retaining the existing original-file backup.
Dropping the receiver releases a producer waiting on a full channel. A currently
running blocking HTTP request remains subject to the existing timeout/retries;
desktop cancellation still terminates the owned translation process.

This extends iteration-level scheduling across chapter boundaries, following
the selective-batching idea in [Orca](https://www.usenix.org/conference/osdi22/presentation/yu).
It removes repeated drains of the decoder batch and batches short titles with
body text. With download time `D` and inference time `T`, ideal pipelining tends
toward `max(D,T)` instead of `D+T`; startup, dependencies, request delays and
tails prevent assuming that bound is reached. Cached-HTML measurements exclude
network time and cannot establish an Internet-download speedup.

## Optional diagnostics

Build the CLI with `--features inference-profile` to print an
`INFERENCE_PROFILE` JSON record on stderr at exit. RAII scopes collect encoder,
decoder, batched linear, attention, FFN, output projection, normalization and
embedding wall times and call counts. The records contain no source text.
Nested times overlap and must not be summed as separate stages. The default
production build excludes these counters and timing calls, and adds no
profiling dependency or payload file.

A local 35-chapter diagnostic run before cross-chapter batching measured
27.78 seconds in encoding and 35.89 seconds in decoding, including 18.33 seconds
in full-vocabulary projection and selection. Normalization accounted for
1.93 nested seconds. These measurements prioritized shared linear work and
underfilled chapter tails over rewriting a small normalization fraction.
They describe that diagnostic run, not the final release timing or an average
over different computers.

The Q8 packed cache aligns each code tile to 64 bytes and expands its F16
scales to exactly representable F32 once during packing. This removes repeated
scale conversion while retaining the original codes, scales and FMA order.
Packed tiles increase from 544 to 576 bytes (about 5.9%); the downloaded GGUF,
model weights and release payload size are unchanged. This is a speed/memory
tradeoff, not further quantization or model training.
