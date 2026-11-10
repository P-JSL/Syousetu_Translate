//! Opt-in diagnostic counters, excluded from production builds. Nested phase
//! times overlap; they must not be added together as independent wall time.
use serde::Serialize;
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
}
const NAMES: [&str; 8] = [
    "encoder",
    "decoder",
    "batch_linear",
    "batch_attention",
    "batch_ffn_including_norm",
    "projection_including_selection",
    "norm",
    "embedding",
];
struct Counter {
    calls: AtomicU64,
    nanos: AtomicU64,
}
impl Counter {
    const fn new() -> Self {
        Self {
            calls: AtomicU64::new(0),
            nanos: AtomicU64::new(0),
        }
    }
}
static COUNTERS: [Counter; 8] = [const { Counter::new() }; 8];
pub(crate) struct Scope {
    stage: Stage,
    started: Instant,
}
impl Scope {
    pub(crate) fn start(stage: Stage) -> Self {
        Self {
            stage,
            started: Instant::now(),
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
    }
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
}
pub fn snapshot() -> Vec<Measurement> {
    NAMES
        .iter()
        .zip(&COUNTERS)
        .map(|(&phase, counter)| Measurement {
            phase,
            calls: counter.calls.load(Ordering::Relaxed),
            seconds: counter.nanos.load(Ordering::Relaxed) as f64 / 1e9,
        })
        .collect()
}
