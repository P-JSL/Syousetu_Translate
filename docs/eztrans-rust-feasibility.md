# Full ezTrans XP engine reconstruction in Rust

Reviewed on 2026-11-10 after the request to consider replacing the complete
translation engine. The earlier adapter-only scope in
[root-dll-scope.md](root-dll-scope.md) describes that historical implementation;
it does not exclude investigating the core for this review.

**A standalone rule-based Japanese→Korean engine can be written entirely in
Rust. A faithful replacement for the supplied ezTrans engine has not been
implemented or established as a short route to the 90% speed target.**
The existing Rust glossary engine is not such a replacement.

## What the supplied archives establish

The two source ZIPs contain Java clients, not the commercial engine source.
The root `J2KEngine.dll`/`J2KEngineH.dll` is a 364,032-byte EHND wrapper with
25 exports. Its `J2KEngine.dlx` dependency is the 2,998,272-byte translation
core with 466 exports, byte-identical to `translator/J2KEngine.dll` in this
sample. Both binaries and their dictionaries were inspected locally; no vendor
payload or disassembly is included in Git or a release.

The core SHA-256 is
`141a53735eddfd950b9939af00606d8869f5665b9b1ca63be4adfbd3648f5ca0`.
PE imports include `MFC42.DLL`, `MSVCRT.dll`, `MSVCIRT.dll`, `KERNEL32.dll` and
`USER32.dll`. Its exported classes expose trie searches, rule records,
semantic rules and post-translation rules. These are useful analysis entry
points, not a specification of their data structures or behavior.

This review followed the exported global and rule-table `DecryptLine`
trampolines into the actual core functions. Both contain a small byte-pair
transformation. Identifying that operation does **not** establish that it
decodes the `RuleKJ.Dat` payload, dictionary fields or grammar operations.
The sampled rule payload does not match a simple canonical byte-pair encoding;
no inferred decoder was applied to production data.

The existing [binary analysis](legacy-binary-analysis.md) verifies only
container framing:

| Asset | Established | Still needed for translation |
| --- | --- | --- |
| `JisJK.Inf` | 376,093 fixed 128-byte records | Lexemes, fields, POS, references and encodings |
| `RuleKJ.Dat` | 6,086 fixed 1,024-byte pages | Rule syntax, matching, priority and execution |
| `CRuleKJ.Dat` | 8,860 length-prefixed records | Meaning and correspondence to other rule files |
| `YongUn.Dat` | Cache signature and `KD2G` marker | Index/trie, morphology and record interpretation |

The archive also contains `.da` indices, `JisKJ.Inf`, `YongUnKJ.Dat`, ROM
tables and replacement files. Filenames and size alone do not prove their
translation direction or usable field layouts. A guessed UTF-8, Shift-JIS or
CP949 decode would not validate a bilingual dictionary.

## Why the current glossary engine cannot replace it

The existing `legacy/native.rs` performs dictionary matching and a limited
set of conjugation/particle operations. Two authored probes executed against
the existing legacy CLI produced:

| Japanese | Existing glossary output |
| --- | --- |
| `彼はまだ帰っていません。` | `그는 ま이다돌아가고いません。` |
| `明日までに報告書を提出しなければならない。` | `내일까지에 報告書를提出しなければならない。` |

Retaining unknown text makes these gaps visible. Counting such a fast
lookup as completed novel translation would give a misleading speed result.
It must not become the default backend to meet a performance threshold.

## Implementable Rust architecture and acceptance conditions

The application already has a `Translator` interface, ordered output,
bounded worker scheduling, source-qualified correction memory and live TXT
saving. A reconstructed rule engine would implement that interface without
changing the GUI or downloading a neural model. Its missing language work is:

1. **Dictionary reader:** versioned, bounds-checked trie/record readers with
   explicit source/target encodings and borrowed immutable data. A parsed raw
   record and a validated bilingual lexeme should be different types.
2. **Japanese analysis:** token/POS and inflection analysis, ambiguity lattices,
   transition scores, compound words, unknown names and colloquial forms.
3. **Transfer rules:** decoded rule instructions with a typed instruction enum,
   phrase matching, priorities, negation, tense/aspect and argument relations.
4. **Korean generation:** conjugation, particles, spacing, politeness, speaker
   register and preservation of quotation/emotional punctuation.
5. **EHND compatibility:** documented pre/post stage ordering and dictionary
   precedence. Existing literal and regex readers provide parts of this work;
   they are not proof of complete control-flow compatibility.
6. **Differential evaluation:** the original engine as a local reference,
   authored minimal pairs and held-out novel passages, including negative
   forms, obligations, numbers, names, contractions and dialogue register.

Immutable dictionaries can be shared across independent sentence workers,
while each sentence owns its analysis lattice and rule state. A bounded Rayon
pool and indexed results can preserve source order without creating 32 model
copies. Worker count must be tuned to actual CPU capacity; 32 workers alone
do not imply a speedup.

The existing `compare-engines` command can collect local original-DLL versus
Rust results on supported 32-bit Windows. It currently compares the limited
glossary backend, so its output would have to identify a new reconstructed
backend explicitly. The Linux test host has no Wine installation or working
original-engine runtime; translation equivalence was not tested here.

An original-DLL FFI adapter, an x86 emulator executing its binary, and a parser
of opaque records all remain different from a complete Rust implementation.
No model-free replacement is enabled by this review. A future switch requires
both semantic/dialogue quality validation and complete-work timing through
the final TXT save; the current evidence supplies neither for a reconstructed
ezTrans core.

## Relationship to CTranslate2 and ONNX

CTranslate2 and ONNX Runtime are alternate neural inference engines, not
reimplementations of ezTrans. A Rust program can call either, but their native
runtimes contain C/C++; such a program must not be described as a pure Rust
translation engine. The current published application uses the existing
Rust/Candle backend. Runtime comparisons belong in isolated research tools
until quality, speed and dependency requirements are satisfied.
