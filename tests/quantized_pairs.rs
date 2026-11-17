#![cfg(feature = "offline-model")]
use candle_core::quantized::k_quants::{BlockQ8_0, GgmlType};

#[test]
fn shared_weights_preserve_each_original_dot_bit_for_bit() {
    let mut seed = 71u64;
    for n in [0, 32, 64, 256, 1024, 4096] {
        let mut make = |kind| {
            let values = (0..n)
                .map(|index| {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    match kind {
                        0 => 0.,
                        1 => ((index % 255) as f32 - 127.) * 0.5,
                        2 => (seed >> 32) as i32 as f32 / i32::MAX as f32,
                        _ => -((index % 128) as f32),
                    }
                })
                .collect::<Vec<_>>();
            let mut quantized = vec![BlockQ8_0::zeros(); n / 32];
            BlockQ8_0::from_float(&values, &mut quantized);
            quantized
        };
        let weights = [make(0), make(1), make(2), make(3)];
        let inputs = [make(2), make(1)];
        let weight_refs = weights.each_ref().map(Vec::as_slice);
        let input_refs = inputs.each_ref().map(Vec::as_slice);
        let flat_inputs = inputs.concat();
        let mut scales = Vec::new();
        BlockQ8_0::fill_input_scales(&flat_inputs, &mut scales);
        let actual = BlockQ8_0::vec_dot_4_rows(n, weight_refs, &flat_inputs, &scales, 2);
        for (row, input) in input_refs.iter().enumerate() {
            let (a, b, c, d) = BlockQ8_0::vec_dot_4(
                n,
                weight_refs[0],
                weight_refs[1],
                weight_refs[2],
                weight_refs[3],
                input,
            );
            assert_eq!(
                actual[row].map(f32::to_bits),
                [a, b, c, d].map(f32::to_bits),
                "n={n}, row={row}"
            );
        }
        let swapped_inputs = [input_refs[1], input_refs[0]].concat();
        BlockQ8_0::fill_input_scales(&swapped_inputs, &mut scales);
        let swapped = BlockQ8_0::vec_dot_4_rows(n, weight_refs, &swapped_inputs, &scales, 2);
        assert_eq!(swapped[0].map(f32::to_bits), actual[1].map(f32::to_bits));
        assert_eq!(swapped[1].map(f32::to_bits), actual[0].map(f32::to_bits));
    }
}

#[test]
fn generic_matrix_tiles_and_odd_tails_match_original_single_row_dots() {
    use candle_core::quantized::k_quants::matmul;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();
    for (m, n, k) in [
        (1, 4, 32),
        (2, 4, 256),
        (3, 7, 1024),
        (15, 3, 32),
        (17, 9, 256),
        (33, 16, 1024),
        (65, 48, 256),
        (128, 4, 4096),
    ] {
        let source = (0..m * k)
            .map(|i| ((i * 53 % 991) as f32 - 495.) / 127.)
            .collect::<Vec<_>>();
        let weights = (0..n * k)
            .map(|i| ((i * 17 % 997) as f32 - 498.) / 127.)
            .collect::<Vec<_>>();
        let mut weights_q = vec![BlockQ8_0::zeros(); n * k / 32];
        let mut inputs_q = vec![BlockQ8_0::zeros(); m * k / 32];
        BlockQ8_0::from_float(&weights, &mut weights_q);
        BlockQ8_0::from_float(&source, &mut inputs_q);
        let mut actual = vec![0.; m * n];
        pool.install(|| matmul::<BlockQ8_0>((m, k, n), &source, &weights_q, &mut actual))
            .unwrap();
        for row in 0..m {
            for column in 0..n {
                let expected = BlockQ8_0::vec_dot(
                    k,
                    &weights_q[column * (k / 32)..(column + 1) * (k / 32)],
                    &inputs_q[row * (k / 32)..(row + 1) * (k / 32)],
                );
                assert_eq!(
                    actual[row * n + column].to_bits(),
                    expected.to_bits(),
                    "{m}/{n}/{k}, {row}/{column}"
                );
            }
        }
    }
}

#[test]
fn decoded_scale_scratch_never_leaks_between_quantized_types() {
    use candle_core::quantized::k_quants::{matmul, BlockQ4K, BlockQ4_0};
    fn compare<T: GgmlType>() {
        let (m, n, k) = (3, 4, 256);
        let source = (0..m * k)
            .map(|i| ((i * 53 % 991) as f32 - 495.) / 127.)
            .collect::<Vec<_>>();
        let weights = (0..n * k)
            .map(|i| ((i * 17 % 997) as f32 - 498.) / 127.)
            .collect::<Vec<_>>();
        let blocks = k / T::BLCK_SIZE;
        let mut weights_q = vec![T::zeros(); n * blocks];
        let mut inputs_q = vec![T::VecDotType::zeros(); m * blocks];
        T::from_float(&weights, &mut weights_q);
        T::VecDotType::from_float(&source, &mut inputs_q);
        let mut actual = vec![0.; m * n];
        matmul::<T>((m, k, n), &source, &weights_q, &mut actual).unwrap();
        for row in 0..m {
            let (a, b, c, d) = T::vec_dot_4(
                k,
                &weights_q[..blocks],
                &weights_q[blocks..2 * blocks],
                &weights_q[2 * blocks..3 * blocks],
                &weights_q[3 * blocks..4 * blocks],
                &inputs_q[row * blocks..(row + 1) * blocks],
            );
            assert_eq!(
                actual[row * n..(row + 1) * n]
                    .iter()
                    .map(|value| value.to_bits())
                    .collect::<Vec<_>>(),
                [a, b, c, d].map(f32::to_bits)
            );
        }
    }
    rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap()
        .install(|| {
            compare::<BlockQ8_0>();
            compare::<BlockQ4_0>();
            compare::<BlockQ4K>();
            compare::<f32>();
            compare::<BlockQ8_0>();
        });
}
