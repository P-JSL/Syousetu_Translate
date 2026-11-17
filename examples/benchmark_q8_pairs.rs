//! One control/candidate matrix pass per shape; no warmups or repetitions.
use anyhow::Result;
use candle_core::quantized::k_quants::{BlockQ8_0, GgmlType};
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use std::time::Instant;

fn run<const SHARED: bool>(
    m: usize,
    n: usize,
    k: usize,
    weights: &[BlockQ8_0],
    inputs: &[BlockQ8_0],
) -> Vec<f32> {
    let blocks = k / 32;
    let mut scales = Vec::new();
    if SHARED {
        BlockQ8_0::fill_input_scales(inputs, &mut scales);
    }
    let mut output = vec![0.; m * n];
    output
        .par_chunks_mut(16 * n)
        .enumerate()
        .for_each(|(tile, out)| {
            let rows = out.len() / n;
            for column in (0..n).step_by(4) {
                let w = std::array::from_fn(|c| {
                    &weights[(column + c) * blocks..(column + c + 1) * blocks]
                });
                if SHARED {
                    let start = tile * 16 * blocks;
                    let end = start + rows * blocks;
                    let values = BlockQ8_0::vec_dot_4_rows(
                        k,
                        w,
                        &inputs[start..end],
                        &scales[start..end],
                        rows,
                    );
                    for (row, values) in values.iter().take(rows).enumerate() {
                        out[row * n + column..row * n + column + 4].copy_from_slice(values);
                    }
                } else {
                    for row in 0..rows {
                        let start = (tile * 16 + row) * blocks;
                        let (a, b, c, d) = BlockQ8_0::vec_dot_4(
                            k,
                            w[0],
                            w[1],
                            w[2],
                            w[3],
                            &inputs[start..start + blocks],
                        );
                        out[row * n + column..row * n + column + 4].copy_from_slice(&[a, b, c, d]);
                    }
                }
            }
        });
    output
}

fn main() -> Result<()> {
    std::env::set_var("CANDLE_NUM_THREADS", "1");
    let pool = rayon::ThreadPoolBuilder::new().num_threads(4).build()?;
    let candidate_first = std::env::var("SYOUSETU_BENCH_CANDIDATE_FIRST").as_deref() == Ok("1");
    for (m, n, k) in [(256, 4096, 1024), (64, 1024, 4096), (32, 4096, 1024)] {
        let quantize = |size| {
            let values = (0..size)
                .map(|i| ((i * 17 % 997) as f32 - 498.) / 127.)
                .collect::<Vec<_>>();
            let mut output = vec![BlockQ8_0::zeros(); size / 32];
            BlockQ8_0::from_float(&values, &mut output);
            output
        };
        let weights = quantize(n * k);
        let inputs = quantize(m * k);
        let control = || {
            let start = Instant::now();
            let values = pool.install(|| run::<false>(m, n, k, &weights, &inputs));
            (values, start.elapsed().as_secs_f64())
        };
        let candidate = || {
            let start = Instant::now();
            let values = pool.install(|| run::<true>(m, n, k, &weights, &inputs));
            (values, start.elapsed().as_secs_f64())
        };
        let ((control, control_seconds), (candidate, candidate_seconds)) = if candidate_first {
            let candidate = candidate();
            (control(), candidate)
        } else {
            let control = control();
            (control, candidate())
        };
        anyhow::ensure!(
            control
                .iter()
                .map(|x| x.to_bits())
                .eq(candidate.iter().map(|x| x.to_bits())),
            "score mismatch"
        );
        let mut hash = Sha256::new();
        for value in control {
            hash.update(value.to_le_bytes());
        }
        println!(
            "{}",
            serde_json::json!({"m":m,"n":n,"k":k,"control_seconds":control_seconds,
            "candidate_seconds":candidate_seconds,"reduction_percent":100.*(1.-candidate_seconds/control_seconds),
            "one_pass_per_variant":true,"candidate_first":candidate_first,"output_sha256":format!("{:x}",hash.finalize())})
        );
    }
    Ok(())
}
