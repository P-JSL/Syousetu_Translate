# Modularization and optimization validation

The local comparison uses the same machine, pinned SMaLL-100 INT8 assets, four
workers, fixed parallel mode and cached HTML for all 35 chapters of
`https://kakuyomu.jp/works/11054822663814504924`. Baseline is the v0.3.1 application
at `4fcfd86bafeb3d501f6fad44fbb1aeec4df6086d`. Both runs disable correction memory
and network delays and save Japanese/Korean output for chapters 1 through 35.
This isolates inference, scheduling, parsing and saving from network variability
and changes in correction data. Reading the original pages is still subject to
the site's availability when downloading a new work.

| Measurement | Baseline | Refactored |
| --- | ---: | ---: |
| Elapsed time, seconds | 183.267 | 174.758 |
| Peak resident memory, KiB | 842,780 | 833,076 |
| Saved bytes | 627,825 | 627,825 |
| Process exit code | 0 | 0 |

Elapsed time was 4.64% shorter in this sequential paired local measurement.
Peak resident memory was about 9.48 MiB lower. These are one-machine observations,
not a guarantee for every CPU, chapter length or worker configuration.

The two complete saved files compare byte-for-byte equal. Both have SHA-256
`28966275a6b45b95bb12a182ac1fc431113c047193fe066b021fd9db537aa317`.
The book text and cached pages are not committed or redistributed.

The production GUI feature suite, optional legacy suite, maintainer packaging
tests and Clippy checks cover the changed module boundaries. Targeted inference
validation with the real pinned assets checks direct Japanese-to-Korean decoding,
serial/parallel consistency and user corrections. Unit tests compare borrowed
logit selection with the previous reference behavior, including ties, invalid
scores, repetition bans, vocabulary bounds and tensor storage offsets.

Correction validation is independent of the speed comparison: bounded compressed
load/edit, exact embedded-source equivalence, user override/deletion persistence,
dictionary conflict rejection and the supplied JK file's hash-scoped content review
are tested. The larger personal import contains 15,328 exact-source entries and
six qualified spelling rules in a 165,691-byte compressed file; it is kept outside
source and release archives. See [resource audit](resource-audit.md).
