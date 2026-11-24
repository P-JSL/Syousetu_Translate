//! Offline inference facade. Weights are immutable; runner state is isolated.
mod builder;
mod device;
mod model;
mod ordered;
mod policy;
#[cfg(feature = "inference-profile")]
pub mod profiling;
mod runner;
mod runtime;
mod scheduler;
mod selection;
mod text;
mod tokenizer;
pub use builder::{CpuBudget, TranslatorBuilder};
pub use cache::CacheStats;
pub use device::InferenceDevice;
pub use policy::ParallelPolicy;
use runner::Runner;
use std::sync::Arc;
pub struct OfflineTranslator {
    runners: Vec<Runner>,
    cache: cache::Cache,
    single_pool: Arc<rayon::ThreadPool>,
    compute_pool: Arc<rayon::ThreadPool>,
    parallel: Option<bool>,
    prefill_budget: runner::batched::PrefillBudget,
}

#[cfg(test)]
mod tests;

mod batch;
mod cache;
