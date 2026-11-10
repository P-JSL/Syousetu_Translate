//! Local pinned-weight matrix benchmark. Not included in portable releases.
use anyhow::Result;
use candle_core::{Device, Module, Tensor};
use sha2::{Digest, Sha256};
use std::{hint::black_box, time::Instant};
use syousetu_translate::int8::Weights;
fn main() -> Result<()> {
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
        let expected = pool.install(|| matrix.forward(&x))?;
        let mut hash = Sha256::new();
        for value in expected.flatten_all()?.to_vec1::<f32>()? {
            hash.update(value.to_le_bytes());
        }
        let mut times = Vec::new();
        for _ in 0..5 {
            let start = Instant::now();
            black_box(pool.install(|| matrix.forward(&x))?);
            times.push(start.elapsed().as_secs_f64());
        }
        times.sort_by(f64::total_cmp);
        println!(
            "{}",
            serde_json::json!({"name":name,"rows":rows,"input":input,"output":output,"median_seconds":times[2],"sha256":format!("{:x}",hash.finalize())})
        );
        if name == "model.shared.weight" {
            let banned = vec![std::collections::HashSet::new(); rows];
            let mut times = Vec::new();
            for _ in 0..5 {
                let start = Instant::now();
                black_box(pool.install(|| matrix.forward_top1(&x, output, 2, &banned))?);
                times.push(start.elapsed().as_secs_f64());
            }
            times.sort_by(f64::total_cmp);
            println!(
                "{}",
                serde_json::json!({"name":"fused-top1","rows":rows,"median_seconds":times[2]})
            );
        }
    }
    Ok(())
}
