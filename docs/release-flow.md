# Minimal application release flow

An application release reuses an already successful, exact-source **Rust CI**
artifact. Do not rebuild/export the unchanged model or dispatch `model` and
`model-smoke` merely to publish a new application version.

1. Complete Linux/Windows production checks, lint and compatibility regression
   checks for the exact source. Shipping builds enable `gui`, not `legacy` or
   `model-tools`. Production test targets are libraries, binaries and integration
   tests; maintainer examples are excluded from those release-profile tests.
2. Create a new immutable version tag and a draft. Existing tags are fetched
   through `FETCH_HEAD` during validation, without replacing a local tag.
3. Dispatch `release.yml` once with `kind=web`, the new tag and successful CI run
   ID. The Linux job validates the tag/CI/draft/source artifact, packages that
   application and the pinned prepared INT8 assets, and uploads ZIP/checksums.
   Model download cache contents are revalidated; cached files are not trusted
   instead of their pinned hashes.
4. The same workflow automatically runs the dependent **read-only Windows**
   portable check. It checks out the verified source SHA, downloads the identical
   staged artifact using `actions: read`, verifies hashes/ZIP CRC/allowlists and
   runs offline translation from a different working directory. It compiles only
   the small archive verifier, with HTTP/inference dependencies disabled; the
   application is not rebuilt.
5. Publish the verified draft after the combined workflow succeeds. A packaging
   upload alone is not a completed release.

`portable-smoke` remains available for an explicit verification retry using
`package_run_id`; it does not need content-write permission. `model` is reserved
for model maintenance, and `model-smoke` for an explicit model validation. The
application payload excludes source archives, legacy binaries, maintainer tools,
unused DLLs, training frameworks and private novel text.

Current speed-target work must still satisfy the recorded 70%/90% full-work
gates and preserve quality before being labeled as achieving those targets.
