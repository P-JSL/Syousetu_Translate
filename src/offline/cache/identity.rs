use super::super::ParallelPolicy;
use crate::learning::Memory;
use anyhow::Result;
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Serialize)]
struct Identity<'a> {
    format: u32,
    executable: &'a str,
    memory: &'a Memory,
    policy: &'a str,
    workers: usize,
    cpus: usize,
    platform: [&'static str; 2],
    x86_kernels: [bool; 4],
}
pub(super) fn fingerprint(
    executable: &str,
    memory: &Memory,
    policy: ParallelPolicy,
    workers: usize,
    cpus: usize,
) -> Result<String> {
    let policy = match policy {
        ParallelPolicy::Auto => "auto",
        ParallelPolicy::Serial => "serial",
        ParallelPolicy::Parallel => "parallel",
    };
    #[cfg(target_arch = "x86_64")]
    let kernels = [
        std::is_x86_feature_detected!("avx2"),
        std::is_x86_feature_detected!("avx512f"),
        std::is_x86_feature_detected!("avx512vnni"),
        std::is_x86_feature_detected!("fma"),
    ];
    #[cfg(not(target_arch = "x86_64"))]
    let kernels = [false; 4];
    let identity = Identity {
        format: 1,
        executable,
        memory,
        policy,
        workers,
        cpus,
        platform: [std::env::consts::OS, std::env::consts::ARCH],
        x86_kernels: kernels,
    };
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&identity)?)
    ))
}
// Called only after the model's metadata and GGUF hashes were checked. The
// executable hash binds the pinned manifest, tokenizer/generation and prose code.
pub(super) fn current(
    memory: &Memory,
    policy: ParallelPolicy,
    workers: usize,
    cpus: usize,
) -> Result<String> {
    let executable = crate::storage::sha256_file(&std::env::current_exe()?)?;
    fingerprint(&executable, memory, policy, workers, cpus)
}
