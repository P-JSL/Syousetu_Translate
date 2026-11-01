# Binary format inspection of ezTrans XP (user-owned sample)

Status: **verified container framing, not verified translation engine semantics**.
Nothing from the proprietary binary payload is committed to this repository.

## Inspection input

The file sizes and hashes below were measured against the user's
ezTrans XP.zip sample. The values must not be assumed universal across versions.

| File | Size (bytes) | SHA-256 |
|---|---:|---|
| JisJK.Inf | 48,139,904 | e92b32e3b3e3c61762abd71f0f42a7f4d23761473e9aeb2915193bb6076585be |
| RuleKJ.Dat | 6,232,068 | 0cca74d66e7a636565f21ccd5e84db7a711f1108916f83f7a91dbcd621050cea |
| CRuleKJ.Dat | 598,219 | 5bb9c5184846a4052a6c1fe9dd7124fdc7b6031195b329d187b427c6d5fe6cf8 |
| YongUn.Dat | 23,723,232 | 30261fd88e8cfa94ebc63ccc2984094f43df744704c5b65518908df09957e28c |

## Verified structure

### JisJK.Inf

* Exact alignment: 48,139,904 / 128 = **376,093** records.
* Every one of these records contains 32 leading zero bytes in the
  inspected sample. Their meaning is unknown, so they are preserved.
* Remaining bytes have characteristic offset-dependent distributions;
  source strings and target translations **have not** been identified.
* The Rust reader exposes the entire opaque 128-byte record. It must
  **not** decode a string by assuming an encoding or padding scheme.

### RuleKJ.Dat

* File begins with a little-endian u32 6086.
* It is followed by exactly 6086 fixed-size 1024-byte pages:
  4 + (6086 * 1024) = **6,232,068** bytes.
* Many pages have trailing zero padding. This padding is **not stripped**:
  zero bytes may also occur in the unknown bytecode.
* Grammar condition bytes and output interpretation are unverified.

### CRuleKJ.Dat

* File begins with a little-endian u32 8860.
* Each following record has a little-endian 32-bit length followed by
  exactly that many raw bytes.
* Parsing all 8860 records reaches EOF exactly; lengths range 7–1023
  bytes in this sample.
* Different record counts prevent naïve positional correspondence with
  RuleKJ pages. An index-by-index comparison showed 112 byte-prefix
  matches, of which 92 were equal with zero padding. This **does not**
  prove equivalent rule semantics, offsets, or mapping.

### YongUn.Dat

* Observed signature: ChangShinSoft Co. 1998 Cache of MainDic (C)
* Signature is followed by byte 0x1A. ASCII marker KD2G appears at
  offset 138 in the supplied file.
* All other offsets, internal indices, cache compression, and word
  data semantics are unknown and intentionally not inferred.

## Rust implementation

* src/legacy_dat.rs validates exact container bounds without writing to
  proprietary files; all record data is borrowed without modification.
* CLI: cargo run -- inspect-dat --dir /path/to/Dat
* JSON output: cargo run -- inspect-dat --dir /path/to/Dat --out report.json
* Tests include valid and malformed fixed pages, compact records, cache
  headers, alignment, and truncated files.
* Set SYOUSETU_LEGACY_DAT_DIR to a local directory with the above files
  for the optional real-file test. Never upload proprietary samples to CI.

## Original DLL provenance and confirmed symbols

The archive contains **two different DLL families**:

* Root J2KEngine.dll and J2KEngineH.dll: same SHA-256
  751c7dad2fa62c9a62c7fa6eec09d691a897256bd5f308d76dd51810e12790b0
  (364,032 bytes each). The PE export directory's library name is
  EHND.dll; it has 25 exported entries, including J2K ABI wrappers.
* translator/J2KEngine.dll: SHA-256
  141a53735eddfd950b9939af00606d8869f5665b9b1ca63be4adfbd3648f5ca0
  (2,998,272 bytes). This separate original 32-bit DLL has **466 exports**.
  It must not be confused with the EHND wrapper when reconstructing grammar.

The original translator DLL includes MSVC C++ exported names identifying
these *possible reverse engineering entrypoints*:

* CRuleTbl::DecryptLine, CRuleTbl::Save, CRuleRec, CDefRuleFld,
  CSemanticRuleRec, CPostXltRule
* trie_record_jis::loadDatabase, load_P_DA, load_S_DA, load_LA,
  load_rec, inf_file_open, RetrieveRecords, SearchRecord, ReadRule
* trie_record_jis::p_get_node, s_get_node, search_letter, suf_sear,
  inv_code and code

These symbols support the hypothesis that the dictionary uses a custom
trie/index and an encoded rule table. They do NOT reveal layouts,
parameter semantics, key schedules, algorithms, or grammar behavior.
None of those internal functions has been faithfully reimplemented;
the exported names are evidence of required future analysis, not a
claim of bytecode compatibility.

## Known gaps (must not be labelled completed)

1. Grammar opcodes and phrase-matching virtual machine for RuleKJ and CRuleKJ.
2. Japanese morphology, dictionary index and compressed record interpretation
   of YongUn; token weight/transition scoring and part-of-speech semantics.
3. JisJK source-to-target encoding, exact lexeme boundaries and index mapping.
4. Cross-file references and compatibility for alternate versions.
5. Bitwise or semantic translation equivalence against the original DLL.
6. The source ZIP does not include the proprietary DLL source or a
   format specification. Its data alone does not establish translation rules.

**Important:** An exact Rust copy of raw record layouts is not a complete
translation engine. The native glossary engine remains an independent,
limited translator, while the optional licensed original DLL adapter remains
available. No commercial dictionary files should be included in public
releases without permission.
