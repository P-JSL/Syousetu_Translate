//! Rust-only portable release commands. Shipping apps never depend on this crate.
mod archive;
mod digest;
#[cfg(feature = "package")]
mod models;
mod source;
use anyhow::Result;
use std::path::Path;
#[allow(dead_code)]
#[path = "../../release_audit.rs"]
mod audit;
fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        Some("stage-web") if args.len()==3 => source::stage(&args[1],&args[2]),
        #[cfg(feature="package")]
        Some("package-web") if args.len()==8 => archive::package(Path::new(&args[1]),Path::new(&args[2]),Path::new(&args[3]),&args[4],&args[5],args[6].parse()?,Path::new(&args[7])),
        Some("verify-portable") if args.len()==6 => archive::verify_portable(Path::new(&args[1]),Path::new(&args[2]),Path::new(&args[3]),Path::new(&args[4]),&args[5]),
        _ => anyhow::bail!("Usage: stage-web TAG RUN | package-web ARTIFACT METADATA OUT TAG COMMIT RUN CACHE | verify-portable ZIP SUMS OUT EXPECTED_CLIENT COMMIT"),
    }
}

#[cfg(test)]
mod tests;
