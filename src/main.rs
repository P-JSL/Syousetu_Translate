//! Thin CLI entry point.
#![forbid(unsafe_code)]
fn main() {
    std::env::set_var("CANDLE_NUM_THREADS", "1");
    let args = std::env::args().collect::<Vec<_>>();
    let result = syousetu_translate::cli::run(&args);
    #[cfg(feature = "inference-profile")]
    if let Ok(profile) = serde_json::to_string(&syousetu_translate::offline::profiling::snapshot())
    {
        eprintln!("INFERENCE_PROFILE {profile}");
    }
    if let Err(error) = result {
        eprintln!("ERROR: {error:#}");
        std::process::exit(1);
    }
}
