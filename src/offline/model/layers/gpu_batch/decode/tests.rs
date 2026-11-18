use super::*;

struct Reference {
    encoder: Tensor,
    source: Vec<Vec<f32>>,
    history: Vec<Vec<f32>>,
}

fn values(seed: usize, size: usize) -> Vec<f32> {
    (0..size)
        .map(|i| ((i * 71 + seed * 29) % 313) as f32 / 156. - 1.)
        .collect()
}

fn reference(heads: Heads, q: &[f32], kv: &[Vec<f32>]) -> Vec<f32> {
    let mut context = Vec::new();
    for head in 0..heads.count {
        let start = head * heads.width;
        let range = start..start + heads.width;
        let mut scores = kv
            .iter()
            .map(|pair| {
                q[range.clone()]
                    .iter()
                    .zip(&pair[range.clone()])
                    .map(|(q, k)| q * k / (heads.width as f32).sqrt())
                    .sum::<f32>()
            })
            .collect::<Vec<_>>();
        let max = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        scores
            .iter_mut()
            .for_each(|score| *score = (*score - max).exp());
        let sum = scores.iter().sum::<f32>();
        for i in range {
            context.push(
                scores
                    .iter()
                    .zip(kv)
                    .map(|(score, pair)| score / sum * pair[heads.hidden() + i])
                    .sum(),
            );
        }
    }
    context
}

fn check(expected: &[f32], actual: &[f32], step: usize) {
    assert_eq!(actual.len(), expected.len());
    for (i, (&actual, &expected)) in actual.iter().zip(expected).enumerate() {
        assert!(
            (actual - expected).abs() < 2e-6,
            "step {step}, element {i}: {actual} vs {expected}"
        );
    }
}

fn compare_slots(device: &Device) {
    let heads = Heads::new(2, 4);
    let hidden = heads.hidden();
    let mut decoder = Decoder::new(heads, 35, 1, 32, 8);
    let mut oracle = std::iter::repeat_with(|| None)
        .take(35)
        .collect::<Vec<Option<Reference>>>();
    for step in 0usize..16 {
        // Slot zero first wraps its two-row ring, then keeps one sentence long
        // enough to grow it across the wrap boundary. Other slots go inactive,
        // get reused with shorter sources, and span two K/V banks.
        let mut slots = vec![0];
        let mut past = vec![if step < 9 { step % 2 } else { step - 9 }];
        if !(3..12).contains(&step) {
            slots.push(2);
            past.push(step.saturating_sub(12));
        }
        slots.push(32);
        past.push(step);
        if step % 2 == 0 {
            slots.push(34);
            past.push(0);
        }
        let mut encoders = Vec::new();
        let mut input = Vec::new();
        let mut projected_sources = Vec::new();
        let mut new_sources = Vec::new();
        let mut expected_self = Vec::new();
        let mut expected_cross = Vec::new();
        let mut cross_queries = Vec::new();
        for (row, (&slot, &past)) in slots.iter().zip(&past).enumerate() {
            let seed = slot * 23 + step * 11;
            if past == 0 {
                let length = 1 + (step * 3 + slot) % 7;
                let source = (0..length)
                    .map(|i| values(seed + i * 17, 2 * hidden))
                    .collect::<Vec<_>>();
                projected_sources.extend(source.iter().flatten().copied());
                oracle[slot] = Some(Reference {
                    encoder: Tensor::zeros((1, length, hidden), candle_core::DType::F32, device)
                        .unwrap(),
                    source,
                    history: Vec::new(),
                });
                new_sources.push(row);
            }
            let history = oracle[slot].as_mut().unwrap();
            assert_eq!(history.history.len(), past);
            let row = values(seed + 101, 3 * hidden);
            let cross_q = values(seed + 440, hidden);
            history.history.push(row[hidden..].to_vec());
            expected_self.extend(reference(heads, &row[..hidden], &history.history));
            expected_cross.extend(reference(heads, &cross_q, &history.source));
            input.extend(row);
            cross_queries.extend(cross_q);
            encoders.push(history.encoder.clone());
        }
        let mut plan = decoder.prepare(&slots, &past, &encoders).unwrap();
        assert_eq!(plan.new_sources(), new_sources);
        let qkv = Tensor::from_vec(input, (1, slots.len(), 3 * hidden), device).unwrap();
        let context = plan.self_context(0, &qkv).unwrap();
        let flatten = |x: Tensor| x.flatten_all().unwrap().to_vec1::<f32>().unwrap();
        check(&expected_self, &flatten(context), step);

        if !projected_sources.is_empty() {
            let length = projected_sources.len() / (2 * hidden);
            let kv = Tensor::from_vec(projected_sources, (1, length, 2 * hidden), device).unwrap();
            plan.store_sources(0, &kv).unwrap();
        }
        let q = Tensor::from_vec(cross_queries, (1, slots.len(), hidden), device).unwrap();
        check(
            &expected_cross,
            &flatten(plan.cross_context(0, &q).unwrap()),
            step,
        );
        plan.finish();
    }
    let another = Tensor::zeros((1, 3, hidden), candle_core::DType::F32, device).unwrap();
    assert!(
        decoder.prepare(&[32], &[16], &[another]).is_err(),
        "a continuation must keep its encoder identity"
    );
}

#[test]
fn retained_kv_matches_independent_sentences_across_wrap_growth_and_slot_reuse() {
    compare_slots(&Device::Cpu);
}

#[cfg(feature = "cuda")]
#[test]
#[ignore = "requires a real CUDA device and driver"]
fn cuda_batched_kv_matches_reference_on_device() {
    compare_slots(&Device::new_cuda(0).unwrap());
}
