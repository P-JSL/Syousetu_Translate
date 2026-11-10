use crate::{learning::Memory, models};
use anyhow::{bail, Result};
use std::{fs, path::Path, sync::Arc};

use super::{model, runner::Runner, tokenizer::Tokenizer, OfflineTranslator, ParallelPolicy};
impl OfflineTranslator {
    pub fn builder(root: &Path) -> super::TranslatorBuilder {
        super::TranslatorBuilder::new(root)
    }
    pub fn open(root: &Path, memory: Memory, threads: usize) -> Result<Self> {
        Self::open_with_policy(root, memory, threads, ParallelPolicy::Auto)
    }
    pub fn open_with_policy(
        root: &Path,
        memory: Memory,
        threads: usize,
        policy: ParallelPolicy,
    ) -> Result<Self> {
        Self::builder(root)
            .memory(memory)
            .cpu_budget(super::CpuBudget::from_requested(threads))
            .workers(threads)
            .parallel_policy(policy)
            .build()
    }
    pub(super) fn load(
        root: &Path,
        memory: Memory,
        budget: super::CpuBudget,
        policy: ParallelPolicy,
        workers: usize,
    ) -> Result<Self> {
        if usize::BITS < 64 {
            bail!("내부 모델은 64비트 실행 파일이 필요합니다");
        }
        eprintln!("직접 일→한 모델 체크섬 확인 중...");
        models::verify_metadata(root)?;
        let directory = root.join("ja-ko");
        let config: model::Config =
            serde_json::from_slice(&fs::read(directory.join("config.json"))?)?;
        let tokenizer = Arc::new(Tokenizer::open(&directory)?);
        let available = std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1);
        let budget = budget.get().min(available);
        let count = workers.clamp(1, 256);
        let load_pool = Arc::new(
            rayon::ThreadPoolBuilder::new()
                .num_threads(budget)
                .build()?,
        );
        eprintln!("직접 일→한 INT8 모델 로딩 중...");
        let model = load_pool.install(|| -> Result<_> {
            Ok(model::Model::new(
                &config,
                crate::int8::Weights::open(&directory)?,
            )?)
        })?;
        let memory = Arc::new(memory);
        let mut runners = Vec::new();
        // Logical contexts share a single bounded compute pool. More contexts
        // improve batching without creating a thread pool for every context.
        for _ in 0..count {
            runners.push(Runner::new(
                model.clone(),
                config.clone(),
                Arc::clone(&tokenizer),
                Arc::clone(&memory),
                Arc::clone(&load_pool),
            ));
        }
        eprintln!("직접 일→한 모델 준비 완료 · 작업자 {count}개 / CPU {budget}개 · 교정 문장 {}개 / 용어 {}개",memory.sentences.len(),memory.terms.len());
        Ok(Self {
            runners,
            compute_pool: Arc::clone(&load_pool),
            cache: super::cache::Cache::default(),
            single_pool: if budget == 1 {
                load_pool
            } else {
                Arc::new(rayon::ThreadPoolBuilder::new().num_threads(1).build()?)
            },
            parallel: if count == 1 || policy == ParallelPolicy::Serial {
                Some(false)
            } else if policy == ParallelPolicy::Parallel {
                Some(true)
            } else {
                None
            },
        })
    }
    pub fn translate(&mut self, text: &str) -> Result<String> {
        self.translate_prepared(text)
    }
    pub(super) fn translate_prepared(&mut self, text: &str) -> Result<String> {
        // The CPU budget is a cap. Short decode operations often run faster
        // with one CPU than with nested matrix parallelism.
        let pool = Arc::clone(&self.single_pool);
        if let Some(cached) = self.cache.get(text) {
            return Ok(cached);
        }
        let output = pool.install(|| self.runners[0].translate_inner(text))?;
        self.cache.insert(text, output.clone());
        Ok(output)
    }
    pub fn translate_lines(&mut self, lines: &[String]) -> Result<Vec<String>> {
        self.translate_lines_with_progress(lines, |_, _| {})
    }
}
