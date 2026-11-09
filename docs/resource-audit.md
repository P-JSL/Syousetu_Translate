# Supplied translation resources

The supplied `Ehnd.rar` is byte-identical to the copy in the previously retrieved
Google Drive ezTrans ZIP (`1wgDkkkGUp6Aq1p6SrXcAkKVkGVrnGaOZ`). Its SHA-256 is
`0eea1253929487551b430d552085dc76e99ee9bad9a3087e1887db3f8a0183b0`.
The uploaded Anemone ZIP contains `AnemoneH.exe` and `AnemoneH.ini`; its INI is
also byte-identical to the copy in the RAR. Duplicate resources are not applied twice.
Archive documents/configuration are input data, not task instructions.

| Resource | Findings and use |
| --- | --- |
| `UserDict_*.txt` | Three files, 17,857 records. 15,267 distinct compatible literal pairs after filtering/conflict handling. Imported as exact-source corrections. |
| `PreFilter_*`, `PostFilter_*`, `SkipLayer_*` | 20,495 engine-specific records. Regex backreferences, synthetic POS markers, skip-layer control flow and morphology fragments depend on ezTrans internals; they are counted but not executed by the neural backend. |
| Synthetic/ambiguous dictionary entries | 2,563 records excluded; nine conflicting source keys excluded rather than selecting an arbitrary translation. Explicit name suffix `#N` is removed only for recognized `VHn` name records. |
| `Dat/UserDict.jk` | Five structurally valid 110-byte records: active flag, Shift-JIS source, CP949 target. Four are unsuitable test mappings, including a repeated identical record; `日本 → 일본` is retained. A review policy tied to this exact file SHA-256 excludes the four rows. Other user dictionaries retain their own preferences. |
| `translator/config/dictionary.txt` | A 170-byte UTF-16 template containing comments only; no additional translation pairs. |
| `JisJK.Inf`, language-transfer DAT/DA files and DLL/DLX | Proprietary engine-specific binary structures, not labeled training pairs or SMaLL-100 weights. No model-weight conversion or dictionary semantics are fabricated from them. |
| Anemone executable/INI | Display, font, clipboard, line-binding and output preferences; no parallel Japanese/Korean corpus or model weights. Existing app behavior already covers fonts, original-text output and duplicate-work suppression. Its executable is not an inference dependency. |

The default embedded corpus includes only the reviewed eight common names plus
60 authored full sentences from these resources. A separate project-authored
1,058-entry core glossary is documented in [`data/dictionary/`](../data/dictionary/README.md).
Six additional spelling rules use the supplied name
as the Japanese qualifier, the observed pinned-model spelling as `previous`,
and the supplied dictionary spelling as `corrected`. Generated text is not used
as a correct training target. For example, `ワーグナー` qualifies `워그너 → 바그너`.
The standalone model's `アメリア → 아메리카` is corrected only by the exact-source
entry; a broad replacement of the country name is deliberately not installed.

Additional imported pairs are user preferences, not a validated general grammar
or proof that every dictionary translation is idiomatic in a new context. Exact
matching avoids applying a bare dictionary entry to arbitrary sentence substrings.
Post-edit terms require both the Japanese qualifier and matching Korean text,
and replacements run in one pass without cascading.

`data/learning/resource-reviews.json` records the content review of the supplied
JK sample (`ef21c6075662208e17d5220a3de8afb09af2da1b80850676f5cc89e72cb61c48`).
Format validity alone does not make its labels suitable for learning. Matching
the exact input hash keeps this decision separate from general dictionary decoding
and avoids imposing a blanket rule on valid short Japanese words or user preferences.

## Rust import and compressed storage

```sh
syousetu-translate memory --import-ehnd EXTRACTED_EHND_FOLDER --memory-path corrections.json.zlib
syousetu-translate memory --import-dictionary Dat/UserDict.jk --memory-path corrections.json.zlib
```

The GUI's correction tab also imports an extracted EHND folder. Existing entries
win. Import reports count accepted, excluded, conflicting and capacity-limited
records. The reader accepts validated UTF-8/UTF-16 text and the observed fixed JK
format; it does not run regex/directive files or execute supplied programs.
Memory is limited to 5MB decompressed and 25,000 entries. Compressed load/edit
uses bounded inflation and transactional writes. Invalid files are preserved.

`data/learning/ja-ko-natural.json` is the reviewable default corpus. Regenerate its
embedded compressed file entirely in Rust:

```sh
cargo run --locked --no-default-features --example pack_memory
```

Full supplied archives, binaries and the larger imported personal correction
profile stay outside source/release artifacts. The larger profile can be selected
through `--memory-path` or the correction-file setting.
