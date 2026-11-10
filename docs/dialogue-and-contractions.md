# Japanese contractions and Korean prose

`src/translation/prose/` separates reviewed Japanese normalization, intent,
Korean register and typography. The pinned direct Japanese→Korean INT8 model
and its complete vocabulary remain in use. No second model or runtime Python
package is required.

* Reviewed verb forms expand `食べちゃった`, `読んどいて`, `知ってる`,
  `聞いてない`, `知らねえ`, `なんつーか` and related contractions. Bare
  conditionals such as `君が行かなきゃ、困る` are not turned into obligations.
  Regular verbs such as `捨てる` and `勝てる` are not changed by a global
  `てる` replacement.
* Explicit negative-condition obligations are expressed to the model as
  `V必要がある`. Third-person refusals, metalinguistic expressions and negated
  obligation contexts are excluded from friendly-request normalization.
* Source-qualified corrections distinguish completed actions from experiences,
  requests from inability, and casual/polite/narrative endings. Unknown Korean
  morphology is retained. Formal address such as `陛下` is protected; character
  gender or personality is not guessed from names.
* Original quotation frames, questions, exclamations and ellipses are restored.
  Generated whitespace within one source unit is cleaned while keeping spaces
  between words and preserving numbers and units. HTML formatting indentation
  is ignored; explicit paragraphs, blank paragraphs, `<br>` and preformatted
  content remain distinct. Bilingual paragraph breaks are emitted once.

An independently compressed **112-utterance** default memory supplies common
spoken forms, including `りょ`, `あざす`, `マジで？` and `それな。`.
Its reviewable source is `data/learning/ja-ko-speech.tsv`; generate the compressed
asset with `cargo run --locked --example pack_speech`. Whole-utterance matching
also supports quotation frames, punctuation width and an omitted final period.
It does not replace matching substrings inside unrelated sentences. Ambiguous
slang such as `草` and `やばい` is not globally assigned one Korean meaning.

Explicit user corrections take precedence, including quoted whole-source edits.
A persisted speech revision prevents deleted defaults from being reinserted on
each startup. `--no-learning` bypasses this memory; the model integration tests
use that empty snapshot and authored sentences absent from the defaults.
This is reviewed translation memory and preprocessing/postprocessing, not newly
trained neural weights or automatic acceptance of the model's own mistakes.

The end-to-end regression cases check obligation, negation, completed action,
friendly requests, casual prohibitions and polite endings. The model can still
mistranslate unseen content, person relationships, tense and context. These
limited tests do not certify human-level dialogue quality.

Research context: [Controlling Politeness in NMT](https://aclanthology.org/N16-1005/)
uses trained side constraints; this untrained model is not given invented
control tokens. [When a Good Translation is Wrong in Context](https://aclanthology.org/P19-1116/)
motivates checking deixis, ellipsis and lexical cohesion beyond word-level
fluency. Efficiency research and mathematical checks are recorded in
[the inference review](inference-research-2026-11.md).
