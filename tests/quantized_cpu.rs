#![cfg(feature = "offline-model")]
use candle_core::quantized::k_quants::{quantize_q8k_reference, BlockQ8K, GgmlType};
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
