# SMaLL-100 INT8: exact-zero activation diagnostic

2026-11-17 (Asia/Seoul). This is a **diagnostic, not a speedup benchmark**.
The fixed model and accepted inference path remain unchanged; no neurons,
tokens or nonzero operations are removed. The additional 85–95% first-translation
target is still unmet.

The `inference-profile` feature now observes the already-computed fused ReLU
output without changing it. It samples at most eight evenly spaced rows per
call and counts exact F32 zeros and wholly zero aligned groups of four/32.
Counters and the observer call are absent from normal/release builds. No new
dependency is added. A separate `INFERENCE_ACTIVATIONS` diagnostic line preserves
the existing stage-counter format.

The [16-line Japanese fixture](../examples/data/offline_activation_ja.txt)
contains dialogue and narration written for this diagnostic. It was run once
through the accepted control and once through the diagnostic executable, with
32 logical contexts on the four-CPU host. Both used the pinned SMaLL-100 INT8,
no learned corrections and no previous-result cache. Their complete TXT bytes
match, SHA-256
`9ae7d435e10db609d46393830b1a46769f489e14b06987fd9ee7c0d3ba3b4199`.

| Sampled quantity | Count | Fraction |
| --- | ---: | ---: |
| Exact zero F32 values | 1,732,797 / 2,347,008 | 73.830% |
| Wholly zero aligned groups of four | 199,979 / 586,752 | 34.082% |
| Wholly zero aligned groups of 32 | 1,558 / 73,344 | 2.124% |

There were 84 observed calls, 4,059 total rows and 573 sampled rows. These
fractions describe the sampled fused ReLU outputs only. They do not establish
whole-model sparsity, quantized-code zeros, zero patterns shared by multiple
active decoder rows, or a proportional latency reduction. The small proportion
of zero 32-value groups does not justify assuming a large speedup from skipping
such groups in the existing kernel. Arbitrary low-value neuron pruning is not
enabled.

Tests cover independently known zero/group counts, nonfinite and subnormal
values, bounded sampling and invalid/empty row shapes. The profile-feature
tests and strict Clippy check pass. [Exact counts and hashes](first-translation-activations.json)
make this limited observation inspectable; output equality is not a semantic
accuracy certificate for all translations.

[Liu et al., Deja Vu, ICML 2023](https://proceedings.mlr.press/v202/liu23am.html)
studies contextual sparsity with trained predictors and approximate subsets.
No predictor or pruning threshold from that method is applied to this model.
Any later sparse path must first preserve the applicable INT8 arithmetic,
token selection and complete saved output within the agreed memory allowance.

To reproduce the diagnostic, build the CLI with `--features inference-profile`
and run `file` on the fixture with `--no-learning --no-result-cache
--parallel-mode parallel --jobs 32 --no-japanese`. Use a new private output path.
One diagnostic pass is sufficient for collecting these counters; performance
comparisons default to one pair and are capped at two trials per variant.
