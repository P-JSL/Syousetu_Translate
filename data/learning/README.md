# Default Japanese → Korean corrections

`ja-ko-natural.json` contains 60 project-authored sentence examples and eight
reviewed common names from the user-supplied EHND dictionary. Six term rules use
observed outputs from the pinned SMaLL-100 INT8 as matching variants; their target
spellings come from the supplied dictionary. Machine output is not a correct label.

The 5,822-byte source compresses to 2,424 bytes and is embedded in the Rust binary.
Rebuild with `cargo run --locked --no-default-features --example pack_memory`.
The test suite compares the complete decompressed snapshot with this source.

Terms require the Japanese source qualifier and the previous Korean spelling;
they do not apply globally or feed replacements into later rules. Exact sentence
corrections take precedence. User-owned files remain editable and can override
defaults. This is translation-memory adaptation; it does not retrain model weights.

Resource scope and importer exclusions: [resource audit](../../docs/resource-audit.md).

`ja-ko-speech.tsv` supplies the original 112 independently authored spoken
utterances (1,425 compressed bytes). `ja-ko-speech-v2.tsv` adds 180 more
(4,211 compressed bytes): reassurance, apology, requests, prohibition,
permission, hesitation, emotion, monologue and explicit formal address.
Generate both assets with `cargo run --locked --example pack_speech`.
Revision 2 adds only its new entries; it does not reinstall deleted revision-1
entries or overwrite existing user sentences/terms. Deletions made after the
upgrade also remain deleted. This is 292 authored speech entries in two sets,
not 292 additional training samples or a model-quality score.
New revision-2 sources are disjoint from the original sentence defaults,
the 1,058 headwords and revision 1; old deletions cannot be restored through
a duplicate entry in a newer set.

Quoted/periodless whole-utterance aliases do not authorize substring replacement
inside other sentences. The 128 model-review sentences are excluded from these
defaults. The application's internal, native and ezTrans compatibility modes
can use the same memory; `--no-learning` bypasses it. Explicit filter-dictionary
entries take precedence in the compatibility pipeline. See
[dialogue processing](../../docs/dialogue-and-contractions.md) and the
[November review](../../docs/natural-language-review-2026-11.md).
