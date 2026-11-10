use anyhow::{ensure, Context, Result};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Instant,
};
use syousetu_inference_bench::{model, Backend};
use syousetu_translate::{
    config::Config,
    novel::site::{parse_episode, parse_work},
    storage::{sha256_file, StreamingFile},
    translation::{output, session::Session},
};

fn main() -> Result<()> {
    let started = Instant::now();
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    ensure!(
        args.len() == 8 || args.len() == 10,
        "usage: novel MODEL URL INDEX_HTML EPISODE_HTML_DIR CHAPTERS OUT THREADS BATCH [REPLICAS WINDOW_ROWS]"
    );
    let root = PathBuf::from(&args[0]);
    model::verify(&root)?;
    let verified = started.elapsed().as_secs_f64();
    let threads: usize = args[6].parse()?;
    let batch: usize = args[7].parse()?;
    let replicas = args.get(8).map(|v| v.parse()).transpose()?.unwrap_or(1);
    let window_rows = args.get(9).map(|v| v.parse()).transpose()?.unwrap_or(batch);
    let mut session = Session::direct(Box::new(Backend::configured(
        &root,
        threads,
        batch,
        replicas,
        window_rows,
    )?));
    let loaded = started.elapsed().as_secs_f64();
    let work = parse_work(&args[1], &fs::read_to_string(&args[2])?)?;
    let count: usize = args[4].parse()?;
    ensure!(
        count > 0 && count <= work.episodes.len(),
        "invalid chapter range"
    );
    let episodes = (1..=count)
        .map(|number| {
            parse_episode(
                work.site,
                &fs::read_to_string(Path::new(&args[3]).join(format!("{number}.html")))?,
                false,
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let mut output = StreamingFile::create(Path::new(&args[5]))?;
    output.write_all(output::format_header(&mut session, &work)?.as_bytes())?;
    output.flush()?;
    let mut completed = 0;
    output::write_episodes(
        &mut output,
        &mut session,
        &episodes,
        1,
        count,
        &Config {
            print_japanese: true,
            ..Default::default()
        },
        &mut |number, _| {
            ensure!(number == completed + 1, "chapter order mismatch");
            completed = number;
            Ok(())
        },
    )?;
    ensure!(completed == count, "missing chapters");
    output.finish()?;
    let seconds = started.elapsed().as_secs_f64();
    let metadata = fs::metadata(&args[5]).context("missing output")?;
    println!(
        "{}",
        serde_json::json!({
            "engine":"CTranslate2 via Rust host (third-party C++ runtime)",
            "compute":"int8_float32", "threads_per_replica":threads, "replicas":replicas,
            "window_rows":window_rows,"batch":batch, "beam":1,
            "verification_seconds":verified,"load_including_verification_seconds":loaded,
            "seconds":seconds,"chapters":count,"completed_chapters":completed,
            "output_bytes":metadata.len(),"sha256":sha256_file(Path::new(&args[5]))?,
            "semantic_quality_verified":false,"release_gate_evaluated":false,
            "cache_limit":1024,"corrections":"none (--no-learning equivalent)",
            "corpus":"locally cached HTML; parses entire selected corpus before inference"
        })
    );
    Ok(())
}
