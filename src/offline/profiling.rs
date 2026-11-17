//! Opt-in diagnostic counters, excluded from production builds. Nested phase
//! times overlap; they must not be added together as independent wall time.
use serde::Serialize;
mod activations;
pub(crate) use activations::observe_relu;
pub use activations::ActivationProfile;

pub fn activation_snapshot() -> ActivationProfile {
    activations::snapshot()
}
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

#[derive(Clone, Copy)]
#[repr(usize)]
pub(crate) enum Stage {
    Encoder,
    Decoder,
    BatchLinear,
    BatchAttention,
    BatchFfn,
    Projection,
    Norm,
    Embedding,
    KvAppend,
}
const NAMES: [&str; 9] = [
    "encoder",
    "decoder",
    "batch_linear",
    "batch_attention",
    "batch_ffn_including_norm",
    "projection_including_selection",
    "norm",
    "embedding",
    "kv_append",
];
struct Counter {
    calls: AtomicU64,
    nanos: AtomicU64,
    copied_bytes: AtomicU64,
}
impl Counter {
    const fn new() -> Self {
        Self {
            calls: AtomicU64::new(0),
            nanos: AtomicU64::new(0),
            copied_bytes: AtomicU64::new(0),
        }
    }
}
static COUNTERS: [Counter; 9] = [const { Counter::new() }; 9];
pub(crate) struct Scope {
    stage: Stage,
    started: Instant,
    copied_bytes: u64,
}
impl Scope {
    pub(crate) fn start(stage: Stage) -> Self {
        Self {
            stage,
            started: Instant::now(),
            copied_bytes: 0,
        }
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        let counter = &COUNTERS[self.stage as usize];
        counter.nanos.fetch_add(
            self.started.elapsed().as_nanos().min(u64::MAX as u128) as u64,
            Ordering::Relaxed,
        );
        counter.calls.fetch_add(1, Ordering::Relaxed);
        if self.copied_bytes != 0 {
            counter
                .copied_bytes
                .fetch_add(self.copied_bytes, Ordering::Relaxed);
        }
    }
}
/// Counts destination payload bytes for both concatenations, not total memory
/// traffic or allocation size. Individual append scopes can overlap on workers.
pub(crate) fn kv_append(tensors: &[&candle_core::Tensor]) -> Scope {
    let copied_bytes = tensors.iter().fold(0u64, |total, tensor| {
        total.saturating_add(
            (tensor.elem_count() as u64).saturating_mul(tensor.dtype().size_in_bytes() as u64),
        )
    });
    let mut scope = Scope::start(Stage::KvAppend);
    scope.copied_bytes = copied_bytes;
    scope
}
pub(crate) fn measure<T>(stage: Stage, operation: impl FnOnce() -> T) -> T {
    let _scope = Scope::start(stage);
    operation()
}
#[derive(Serialize)]
pub struct Measurement {
    pub phase: &'static str,
    pub calls: u64,
    pub seconds: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub copied_bytes: Option<u64>,
}
pub fn snapshot() -> Vec<Measurement> {
    NAMES
        .iter()
        .zip(&COUNTERS)
        .map(|(&phase, counter)| Measurement {
            phase,
            calls: counter.calls.load(Ordering::Relaxed),
            seconds: counter.nanos.load(Ordering::Relaxed) as f64 / 1e9,
            copied_bytes: (phase == "kv_append")
                .then(|| counter.copied_bytes.load(Ordering::Relaxed)),
        })
        .collect()
}
