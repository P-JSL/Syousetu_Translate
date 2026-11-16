# Legacy rule syntax and neural-model applicability

Reviewed against GitHub main `1c8ee5d` on 2026-11-16 (KST). That upstream
revision changes the README and implementation plan, not the model or Rust
engine. Those documents are preserved. This change implements a read-only
syntax reader; it does not claim a full ezTrans engine or neural retraining.

## Verified reconstruction

Complementing each payload byte (`byte ^ 0xff`) reveals field/operator/literal
expressions in the supplied `RuleKJ.Dat` and `CRuleKJ.Dat`. The Rust reader
preserves quoted bytes, signed relative offsets, `=`, `==`, `!=`, mixed `|`/`&`
set separators, consecutive semicolons and field-reference values. It does not
interpret the original matching priorities or POS codes. A Shift-JIS trail
byte `0x5c` is retained instead of being guessed to be an ASCII escape.

| Sample | Records | Parsed | Predicates |
| --- | ---: | ---: | ---: |
| RuleKJ.Dat | 6,086 | 6,086 | 22,339 |
| CRuleKJ.Dat | 8,860 | 8,860 | 35,583 |

Both tables contain two field-reference values; treating every value as a
quoted string would miss one record in each table. The record positions differ,
so positional correspondence between the two files is still not established.
Fixed-page trailing raw-zero padding is excluded only from the syntax view;
the original framing reader and source bytes are unchanged. Internal zeros are
not stripped. Input is bounded to 64 MiB/file and 4 KiB/decoded record.

[The aggregate audit](legacy-rule-syntax-audit.json) includes exact input hashes
and encoding-validity counts. It contains no dictionary/rule literal payload.
Synthetic tests cover malformed inputs, mixed encodings, field references,
quoted delimiters, original-file preservation and rejection of partial parses.
An opt-in test read all 14,946 original records successfully. The private files
and disassembly remain outside Git and release assets.

```sh
cargo run --locked --no-default-features --features legacy -- \
  inspect-rules --dir LOCAL_EZTRANS_DAT --out syntax-audit.json
SYOUSETU_LEGACY_DAT_DIR=LOCAL_EZTRANS_DAT cargo test --locked \
  --no-default-features --features legacy --lib original_tables_if_available -- --ignored
```

## Application to the repository's translation model

The bundled neural model is SMaLL-100, an encoder–decoder translation model.
Its GGUF INT8 matrices are loaded by `int8/weights.rs` and used by
`offline/model/`. A legacy rule record is neither a model weight nor a labelled
training pair. Assigning it to a tensor or tokenizer ID would be invalid.

All 19,486/30,060 `S` literals decode as CP949, while all 3,948/6,053 `D`
literals decode as Shift-JIS. Together with inspected local examples, this
strongly suggests Korean-source/Japanese-target rules. Decoder validity alone
does not prove field meaning or original runtime equivalence; many byte strings
are valid in both encodings. These tables must not be reversed and installed
as Japanese→Korean model corrections. Their source-relative token/POS context
also differs from SentencePiece tokens.

The repository's actual integration points were rechecked:

| Input/operation | Existing model path | Applicability |
| --- | --- | --- |
| Prepared SMaLL-100 INT8 | manifest/hash verification → shared weights → private decoder state | Preserved; weights/tokenizer are unchanged |
| Reviewed Japanese→Korean EHND literal pairs | `learning/import/ehnd.rs` → `Memory` → exact-source lookup | Already supported; existing user corrections win |
| Source-qualified spelling corrections | `Memory::post_edit` after generation | Already supported; not global or cascading replacement |
| Japanese contractions and speech style | `prose::japanese::expand` before tokenization, `prose::finish` after generation | Existing shared Rust language processing |
| Legacy transfer-rule syntax | new `legacy/rules/` byte parser and aggregate inspection | Readable, but not eligible for default Japanese→Korean execution |
| Engine-specific regex/SkipLayer/POS directives | legacy analysis/explicit experimental paths | Not silently imported into neural memory |

`Memory` is adaptation through approved corrections, not weight training.
`--no-learning` disables that memory. The internal runner returns exact user
sentence corrections before generated-text processing, preserving their
priority. This work does not replace human review of imported labels.

The syntax reader is gated by the optional `legacy` feature and adds no crate
dependency or proprietary payload to the normal GUI package. Completing a
transfer VM still requires verified field meanings, morphology/POS context,
direction, rule ordering and differential testing against the original engine.
Source-aware, engine-independent prose corrections are a separate integration
boundary; they must not be advertised as executing this unverified VM.
That boundary is now shared by the native/ezTrans compatibility pipelines and
the neural runner, including approved correction memory. The
[natural-language review](natural-language-review-2026-11.md) distinguishes
these independently authored Japanese→Korean safeguards from the parsed
Korean→Japanese legacy tables.
