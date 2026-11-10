use anyhow::{ensure, Result};
use std::{fs, path::Path};
use syousetu_inference_bench::{model, Backend};
use syousetu_translate::translation::engine::Translator;

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    ensure!(args.len() == 4, "usage: text MODEL INPUT THREADS BATCH");
    let lines = fs::read_to_string(&args[1])?
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    model::verify(Path::new(&args[0]))?;
    let mut backend = Backend::open(Path::new(&args[0]), args[2].parse()?, args[3].parse()?)?;
    let translated = backend.translate_lines(&lines)?;
    ensure!(translated.len() == lines.len(), "missing lines");
    for (source, korean) in lines.iter().zip(translated) {
        println!(
            "{}",
            serde_json::json!({"source":source,"translation":korean})
        );
    }
    Ok(())
}
