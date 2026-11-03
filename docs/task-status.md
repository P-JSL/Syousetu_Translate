# Recovery and implementation status

The original environment became unavailable. The replacement environment restored
GitHub commit `7969c4360b820597a8ebe0c9673e9b28bfd06d4e`, without the previous
uncommitted source changes, toolchain, reference ZIPs, or execution logs. The
changes in this working tree were recovered from the conversation and agent
context. They have not been committed or pushed.

## Recovered implementation

- Java Properties settings, Unicode escapes/continuations and CLI overrides.
- Ordered isolated translation workers; worker errors preserve source/output.
- Configurable chapter headings, chapter-number formatting, bilingual output,
  description translation, author comments and automatic output filenames.
- UTF-8 BOM/UTF-16 and explicit input encodings; aliased input paths are rejected.
- Dictionary BOM/name/extra-column handling, placeholder collision prevention,
  and opt-in legacy annotation cleanup.
- Source-backed selectors, short stories, author notes, ordered real-link TOC
  pagination and missing/duplicate-page detection for the six supported sites.
- Kakuyomu `tableOfContentsV2` and explicitly public, non-rental Alphapolis
  `app-cover-data` JSON layouts. No authentication, paid-content or CAPTCHA bypass.
- The installer includes the required root `J2KEngine.dlx`; incomplete archives
  are rejected before extraction, without substituting another DLL family.

## Validation record: avoid repeated unchanged checks

The user explicitly requested that checks of unchanged code not be repeated.

Before the old environment disconnected, the parent agent observed passing Rust
library/CLI tests, seven synthetic CLI workflow tests, five Python installer
tests, Clippy with existing warnings, and Linux executable/shared-library builds.
The three proprietary-asset checks were also run with the actual reference
paths, not their usual absent-file early returns. Installing the original ZIP
selected 32 files (144,064,700 bytes); root-wrapper identity and Dat container
framing passed. These records describe the old instance, not a new run.

The site agent reported 24 synthetic parser tests passing before recovery,
including five V2/Alphapolis JSON cases. The restored tests are retained for
future relevant changes; they were not rerun in the replacement environment.
Actual complete live-work `inspect` success remains unverified.

The replacement environment ran **only the new**
`ehnd::tests::underscore_patterns_are_not_control_keywords` library test:
1 passed, 28 existing tests filtered out. A development executable was built
to make the restored application runnable. No full suite or existing smoke
script was rerun.

That new correction classifies only `_TGL`, `_SkipLayer` and `_skip_sub` as
control keywords. Numeric literal/regex patterns beginning with `_` are ordinary
rules. The corpus regression expectations were updated to the shared analysis's
8,668 literal / 8,597 regex / 3,173 control / 7 unknown counts. Those updated
real-corpus expectations have not been rechecked against downloaded files here.

## Reference provenance and remaining work

Previously downloaded, user-provided archives (not included in this repository):

| Archive | Bytes | SHA-256 |
|---|---:|---|
| translator7.zip | 102042123 | 23ef00e3053dfcd061599139a2c1776ff11d4b5bc527e3f76834cb5cb0ca17b7 |
| ezTrans XP.zip | 287405116 | cbd954b128235d805278171c5dd6bc3f187492a2393e4acc9d541399bc4b96a4 |

The nested source archives contain 45 Java files in translator7 and 39 in the
ezTrans-root reader. They contain client/wrapper code, not the implementation of
the commercial language engine. The root EHND wrapper delegates translation to
`J2KEngine.dlx`.

**Full original-engine equivalence has not been achieved.** EHND conditional
state, SkipLayer/skip_sub execution, unknown control calls and export contracts,
dictionary codecs, morphology, grammar transfer and Korean generation still
require verified reconstruction and Windows x86 differential tests. The native
glossary engine must not be described as a complete replacement.

The replacement environment currently allows package-manager destinations. The
provided Drive URL returned HTTP 403. Further reference-file recovery and live
site validation need the relevant external domains enabled through environment
settings. Draft settings do not themselves apply policy or publish a snapshot.

Keep vendor files outside this repository. Back up the working-tree patch before
switching environments; a fresh repository checkout does not carry local changes.
