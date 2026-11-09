//! Construction policy: required model location, typed CPU budget and defaults.
use super::{OfflineTranslator, ParallelPolicy, VocabularyPolicy};
use crate::learning::Memory;
use anyhow::Result;
use std::{
    num::NonZeroUsize,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug)]
pub struct CpuBudget(NonZeroUsize);
impl CpuBudget {
    pub fn from_requested(threads: usize) -> Self {
        Self(NonZeroUsize::new(threads.clamp(1, 64)).expect("clamped budget is nonzero"))
    }
    pub fn get(self) -> usize {
        self.0.get()
    }
}
pub struct TranslatorBuilder {
    root: PathBuf,
    memory: Option<Memory>,
    budget: CpuBudget,
    policy: ParallelPolicy,
    workers: usize,
    vocabulary_policy: VocabularyPolicy,
}
impl TranslatorBuilder {
    pub(super) fn new(root: &Path) -> Self {
        Self {
            root: root.to_owned(),
            memory: None,
            budget: CpuBudget::from_requested(
                std::thread::available_parallelism()
                    .map(usize::from)
                    .unwrap_or(1),
            ),
            policy: ParallelPolicy::Auto,
            workers: 32,
            vocabulary_policy: VocabularyPolicy::Full,
        }
    }
    pub fn memory(mut self, memory: Memory) -> Self {
        self.memory = Some(memory);
        self
    }
    pub fn cpu_budget(mut self, budget: CpuBudget) -> Self {
        self.budget = budget;
        self
    }
    pub fn parallel_policy(mut self, policy: ParallelPolicy) -> Self {
        self.policy = policy;
        self
    }
    pub fn workers(mut self, workers: usize) -> Self {
        self.workers = workers.clamp(1, 64);
        self
    }
    pub fn vocabulary_policy(mut self, policy: VocabularyPolicy) -> Self {
        self.vocabulary_policy = policy;
        self
    }
    pub fn build(self) -> Result<OfflineTranslator> {
        let memory = match self.memory {
            Some(memory) => memory,
            None => Memory::bundled()?,
        };
        OfflineTranslator::load(
            &self.root,
            memory,
            self.budget,
            self.policy,
            self.workers,
            self.vocabulary_policy,
        )
    }
}
