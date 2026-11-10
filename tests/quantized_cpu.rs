#![cfg(feature = "offline-model")]
use candle_core::quantized::k_quants::{quantize_q8k_reference, BlockQ8K, GgmlType};
use candle_core::{
    quantized::{GgmlDType, QMatMul, QTensor},
    Device, Module, Tensor,
};
use std::collections::HashSet;

#[test]
fn changing_or_reordering_other_rows_never_changes_existing_q8_scores() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();
    for (rows, n, k) in [
        (17, 48, 256),
        (65, 128, 1024),
        (257, 48, 256),
        (271, 112, 512),
        (7, 16_400, 256),
    ] {
        let weight = Tensor::from_vec(
            (0..n * k)
                .map(|i| ((i * 17 % 997) as f32 - 498.) / 127.)
                .collect::<Vec<_>>(),
            (n, k),
            &Device::Cpu,
        )
        .unwrap();
        let matrix =
            QMatMul::from_qtensor(QTensor::quantize(&weight, GgmlDType::Q8_0).unwrap()).unwrap();
        let original = (0..rows * k)
            .map(|i| ((i * 53 % 991) as f32 - 495.) / 127.)
            .collect::<Vec<_>>();
        let prefix = Tensor::from_vec(original[..3 * k].to_vec(), (3, k), &Device::Cpu).unwrap();
        let expected = pool
            .install(|| matrix.forward(&prefix))
            .unwrap()
            .to_vec2::<f32>()
            .unwrap();
        for reorder in [false, true] {
            let mut values = original.clone();
            for (i, value) in values.iter_mut().enumerate().skip(3 * k) {
                *value = ((i * 7 % 113) as f32 - 56.) * 11.;
            }
            if reorder {
                for index in 0..k {
                    values.swap(index, (rows - 1) * k + index);
                }
            }
            let x = Tensor::from_vec(values, (rows, k), &Device::Cpu).unwrap();
            let scores = pool
                .install(|| matrix.forward(&x))
                .unwrap()
                .to_vec2::<f32>()
                .unwrap();
            for (row, expected) in expected.iter().enumerate() {
                let index = if reorder && row == 0 { rows - 1 } else { row };
                assert_eq!(
                    scores[index]
                        .iter()
                        .map(|v| v.to_bits())
                        .collect::<Vec<_>>(),
                    expected.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                    "prefix invariant: {rows}/{n}/{k}/{row}"
                );
            }
        }
    }
}

#[test]
fn fused_q8_selection_matches_full_matrix_scores_with_tails_ties_and_bans() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();
    let mut seed = 19u64;
    for (m, n, k) in [
        (2, 16, 256),
        (3, 32, 512),
        (4, 48, 1024),
        (5, 64, 256),
        (7, 112, 512),
        (8, 64, 1024),
        (17, 48, 256),
        (32, 128, 1024),
        (13, 16_400, 256),
    ] {
        let mut random = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            (seed >> 32) as i32 as f32 / i32::MAX as f32
        };
        let weight = Tensor::from_vec(
            (0..n * k).map(|_| random()).collect::<Vec<_>>(),
            (n, k),
            &Device::Cpu,
        )
        .unwrap();
        let matrix =
            QMatMul::from_qtensor(QTensor::quantize(&weight, GgmlDType::Q8_0).unwrap()).unwrap();
        for ties in [false, true] {
            let input = Tensor::from_vec(
                (0..m * k)
                    .map(|_| if ties { 0.0 } else { random() })
                    .collect::<Vec<_>>(),
                (m, k),
                &Device::Cpu,
            )
            .unwrap();
            let scores = pool
                .install(|| matrix.forward(&input))
                .unwrap()
                .to_vec2::<f32>()
                .unwrap();
            for vocab in [n, n - 7] {
                let mut blocked = vec![HashSet::new(); m];
                for (row, bans) in blocked.iter_mut().enumerate() {
                    bans.insert(2);
                    for id in (4..vocab).filter(|id| id % 3 == row % 3) {
                        bans.insert(id as u32);
                    }
                }
                let expected = scores
                    .iter()
                    .zip(&blocked)
                    .map(|(scores, bans)| {
                        scores
                            .iter()
                            .take(vocab)
                            .enumerate()
                            .filter(|&(id, score)| {
                                (id == 2 || id >= 4)
                                    && score.is_finite()
                                    && !bans.contains(&(id as u32))
                            })
                            .max_by(|(a, x), (b, y)| x.total_cmp(y).then_with(|| b.cmp(a)))
                            .unwrap()
                            .0 as u32
                    })
                    .collect::<Vec<_>>();
                if let Some(actual) = pool
                    .install(|| matrix.forward_top1(&input, vocab, 2, &blocked))
                    .unwrap()
                {
                    assert_eq!(
                        actual, expected,
                        "shape {m}/{n}/{k}, ties={ties}, vocab={vocab}"
                    );
                    let all = vec![(0..n as u32).collect(); m];
                    assert!(pool
                        .install(|| matrix.forward_top1(&input, vocab, 2, &all))
                        .is_err());
                }
            }
        }
    }
}
#[test]
fn q8_activation_simd_matches_scalar_rounding_ties_and_nonfinite_values() {
    let mut inputs = vec![vec![0.0f32; 256], vec![-0.0f32; 256]];
    for first in [127.0, -127.0] {
        let mut block = (0..256)
            .map(|i| (i % 127 - 63) as f32 + 0.5)
            .collect::<Vec<_>>();
        block[0] = first;
        block[1] = -first;
        block[2] = 0.49999997;
        block[3] = -0.49999997;
        inputs.push(block);
    }
    let mut seed = 7u64;
    for scale in [1.0, 1e-20, 1e20, 1e-40, f32::MAX / 4.0] {
        for _ in 0..100 {
            inputs.push(
                (0..256)
                    .map(|_| {
                        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                        ((seed >> 32) as i32 as f32 / i32::MAX as f32) * scale
                    })
                    .collect(),
            );
        }
    }
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut block = vec![1f32; 256];
        block[17] = value;
        inputs.push(block);
    }
    for input in inputs {
        let mut expected = vec![BlockQ8K::zeros()];
        let mut actual = vec![BlockQ8K::zeros()];
        quantize_q8k_reference(&input, &mut expected);
        BlockQ8K::from_float(&input, &mut actual);
        assert_eq!(actual, expected);
    }
}
