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

`ja-ko-speech.tsv` adds 112 independently authored whole spoken utterances.
Its versioned zlib JSON is 1,425 bytes and is embedded separately. Generate it
with `cargo run --locked --example pack_speech`. A persisted revision applies
these entries once without restoring later user deletions. Quoted/periodless
whole-utterance aliases do not authorize substring replacement of slang inside
other sentences. See [dialogue processing](../../docs/dialogue-and-contractions.md).
