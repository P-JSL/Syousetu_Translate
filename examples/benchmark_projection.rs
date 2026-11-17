//! Local pinned-weight matrix benchmark. Not included in portable releases.
//! No warmup; default one run per variant, with an explicit maximum of two.
use anyhow::{Context, Result};
use candle_core::{Device, Module, Tensor};
use sha2::{Digest, Sha256};
use std::{hint::black_box, time::Instant};
use syousetu_translate::int8::Weights;

fn repeat_count(value: Option<&str>) -> Result<usize> {
    let count = value
        .map(str::parse::<usize>)
        .transpose()
        .context("SYOUSETU_BENCH_REPEATS must be 1 or 2")?
        .unwrap_or(1);
    anyhow::ensure!((1..=2).contains(&count), "benchmark repeats must be 1 or 2");
    Ok(count)
}

// Independent finite argmax for the example's empty block lists. Token-order
// ties, special-token filtering and total ordering match the production contract.
fn expected_token(scores: &[f32], eos: usize) -> Result<u32> {
    scores
        .iter()
        .enumerate()
        .filter(|(id, score)| (*id >= 4 || *id == eos) && score.is_finite())
        .max_by(|a, b| a.1.total_cmp(b.1).then_with(|| b.0.cmp(&a.0)))
        .map(|(id, _)| id as u32)
        .context("dense projection has no valid token")
}

fn main() -> Result<()> {
    let repeats = repeat_count(std::env::var("SYOUSETU_BENCH_REPEATS").ok().as_deref())?;
    std::env::set_var("CANDLE_NUM_THREADS", "1");
    let root = syousetu_translate::models::default_dir().join("ja-ko");
    let weights = Weights::open(&root)?;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4.min(std::thread::available_parallelism()?.get()))
        .build()?;
    for (name, input, output, rows) in [
        ("model.shared.weight", 1024, 128112, 64),
        ("model.shared.weight", 1024, 128112, 32),
        ("model.encoder.layers.0.fc1.weight", 1024, 4096, 256),
        ("model.decoder.layers.0.fc1.weight", 1024, 4096, 64),
        ("model.decoder.layers.0.fc2.weight", 4096, 1024, 64),
    ] {
        let matrix = weights.matrix(name, (output, input))?;
        let x = Tensor::from_vec(
            (0..rows * input)
                .map(|i| ((i * 17 % 997) as f32 - 498.0) / 127.0)
                .collect::<Vec<_>>(),
            (rows, input),
            &Device::Cpu,
        )?;
        let mut first_hash = None;
        let mut expected_tokens = Vec::new();
        let mut times = Vec::new();
        for run in 0..repeats {
            let start = Instant::now();
            let scores = black_box(pool.install(|| matrix.forward(&x))?);
            times.push(start.elapsed().as_secs_f64());
            if run == 0 {
                // Hash/validation run outside the timed interval, using the
                // actual first measured output instead of an extra inference.
                let values = scores.flatten_all()?.to_vec1::<f32>()?;
                let mut hash = Sha256::new();
                for value in &values {
                    hash.update(value.to_le_bytes());
                }
                first_hash = Some(format!("{:x}", hash.finalize()));
                if name == "model.shared.weight" {
                    expected_tokens = values
                        .chunks_exact(output)
                        .map(|row| expected_token(row, 2))
                        .collect::<Result<_>>()?;
                }
            }
        }
        println!(
            "{}",
            serde_json::json!({"name":name,"rows":rows,"input":input,"output":output,
                "seconds":times,"mean_seconds":times.iter().sum::<f64>() / times.len() as f64,
                "repeats":repeats,"warmup_runs":0,"maximum_runs_per_variant":2,"sha256":first_hash})
        );
        if name == "model.shared.weight" {
            let banned = vec![std::collections::HashSet::new(); rows];
            let mut times = Vec::new();
            let mut supported = true;
            for _ in 0..repeats {
                let start = Instant::now();
                let tokens =
                    black_box(pool.install(|| matrix.forward_top1(&x, output, 2, &banned))?);
                times.push(start.elapsed().as_secs_f64());
                let Some(tokens) = tokens else {
                    supported = false;
                    break;
                };
                anyhow::ensure!(
                    tokens == expected_tokens,
                    "fused tokens differ from dense output"
                );
            }
            println!(
                "{}",
                serde_json::json!({"name":"fused-top1","rows":rows,"supported":supported,
                    "tokens_match_dense":supported,"seconds":times,
                    "mean_seconds":supported.then(||times.iter().sum::<f64>() / times.len() as f64),
                    "repeats_requested":repeats,"runs":times.len(),"warmup_runs":0,"maximum_runs_per_variant":2})
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repetition_count_defaults_to_one_and_rejects_more_than_two() {
        assert_eq!(repeat_count(None).unwrap(), 1);
        assert_eq!(repeat_count(Some("1")).unwrap(), 1);
        assert_eq!(repeat_count(Some("2")).unwrap(), 2);
        for invalid in ["0", "3", "5", "-1", "", "invalid"] {
            assert!(repeat_count(Some(invalid)).is_err());
        }
    }
    #[test]
    fn independent_reference_respects_finite_scores_special_tokens_and_ties() {
        let scores = [100., f32::NAN, 2., f32::INFINITY, 2., 2., f32::NEG_INFINITY];
        assert_eq!(expected_token(&scores, 2).unwrap(), 2);
        assert_eq!(expected_token(&scores, 99).unwrap(), 4);
        assert!(expected_token(&[f32::NAN; 8], 2).is_err());
        assert_eq!(expected_token(&[-1., -1., -0., -1., 0., 0.], 2).unwrap(), 4);
    }
}
