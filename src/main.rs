//! Thin CLI entry point.
#![forbid(unsafe_code)]
fn main() {
    std::env::set_var("CANDLE_NUM_THREADS", "1");
    let args = std::env::args().collect::<Vec<_>>();
    if let Err(error) = syousetu_translate::cli::run(&args) {
        eprintln!("ERROR: {error:#}");
        std::process::exit(1);
    }
}
