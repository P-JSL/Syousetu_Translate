//! Offline inference facade. Weights are immutable; runner state is isolated.
mod builder;
mod model;
mod ordered;
mod policy;
mod runner;
mod runtime;
mod scheduler;
mod text;
mod tokenizer;
pub use builder::{CpuBudget, TranslatorBuilder};
pub use policy::ParallelPolicy;
use runner::Runner;
use std::sync::Arc;
pub struct OfflineTranslator {
    runners: Vec<Runner>,
    cache: cache::Cache,
    single_pool: Arc<rayon::ThreadPool>,
    parallel: Option<bool>,
}

#[cfg(test)]
mod tests;

mod batch;
mod cache;
