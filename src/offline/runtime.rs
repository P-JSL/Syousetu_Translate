use crate::{learning::Memory, models};
use anyhow::{bail, Result};
use std::{fs, path::Path, sync::Arc};

use super::{
    model, runner::Runner, tokenizer::Tokenizer, InferenceDevice, OfflineTranslator, ParallelPolicy,
};

fn worker_count(requested: Option<usize>, cpu: bool, budget: usize) -> usize {
    requested.unwrap_or(if cpu && budget >= 4 { 128 } else { 32 })
}

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
        workers: Option<usize>,
        result_cache: Option<&Path>,
        device_request: (InferenceDevice, usize),
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
        let load_pool = Arc::new(
            rayon::ThreadPoolBuilder::new()
                .num_threads(budget)
                .build()?,
        );
        eprintln!("직접 일→한 INT8 모델 로딩 중...");
        let mut device = device_request.0.resolve(device_request.1)?;
        let prepare = |device: &candle_core::Device| -> Result<_> {
            load_pool.install(|| {
                let model = model::Model::new(
                    &config,
                    crate::int8::Weights::open_on_device(&directory, device)?,
                )?;
                if !device.is_cpu() {
                    // Exercise cuBLAS and synchronize allocations before output starts.
                    use candle_core::{DType, Tensor};
                    let probe = Tensor::ones((2, 16), DType::F16, device)?;
                    let _ = probe.matmul(&probe.t()?)?.to_dtype(DType::F32)?;
                    device.synchronize()?;
                }
                Ok(model)
            })
        };
        let model = match prepare(&device) {
            Ok(model) => model,
            Err(error) if !device.is_cpu() && device_request.0 == InferenceDevice::Auto => {
                eprintln!("GPU 모델 준비 실패로 CPU에서 시작합니다: {error:#}");
                device = candle_core::Device::Cpu;
                prepare(&device)?
            }
            Err(error) => return Err(error),
        };
        let count = worker_count(workers, device.is_cpu(), budget);
        if !device.is_cpu() {
            eprintln!(
                "CUDA GPU {} · SMaLL-100 INT8 파일 / FP16 행렬 · F32 정규화·attention",
                device_request.1
            );
            if result_cache.is_some() {
                // CPU and GPU use different arithmetic. Never restore a CPU
                // result as a GPU result or mix different GPU/driver versions.
                eprintln!("GPU 경로는 디스크 번역 결과를 재사용하지 않습니다");
            }
        }
        let cache = result_cache
            .filter(|_| device.is_cpu())
            .and_then(|directory| {
                match super::cache::Cache::persistent(directory, &memory, policy, count, budget) {
                    Ok(cache) => Some(cache),
                    Err(error) => {
                        eprintln!("이전 번역 결과를 사용할 수 없어 새로 번역합니다: {error:#}");
                        None
                    }
                }
            })
            .unwrap_or_default();
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
            cache,
            single_pool: if budget == 1 {
                load_pool
            } else {
                Arc::new(rayon::ThreadPoolBuilder::new().num_threads(1).build()?)
            },
            parallel: if count == 1 || policy == ParallelPolicy::Serial {
                Some(false)
            } else if policy == ParallelPolicy::Parallel || !device.is_cpu() {
                Some(true)
            } else {
                None
            },
            cpu_batch_token_limit: (device.is_cpu() && workers.is_none() && count > 64).then_some(
                64 * (super::tokenizer::MAX_SOURCE_TOKENS
                    + 2
                    + super::runner::generation::MAX_GENERATED_TOKENS),
            ),
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
    pub fn result_cache_stats(&self) -> super::CacheStats {
        self.cache.stats()
    }
    pub fn flush_result_cache(&mut self) -> Result<()> {
        self.cache.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::worker_count;

    #[test]
    fn automatic_capacity_uses_the_actual_cpu_budget_and_keeps_explicit_values() {
        assert_eq!(worker_count(None, true, 3), 32);
        assert_eq!(worker_count(None, true, 4), 128);
        assert_eq!(worker_count(None, true, 32), 128);
        assert_eq!(worker_count(None, false, 4), 32);
        assert_eq!(worker_count(Some(16), true, 4), 16);
        assert_eq!(worker_count(Some(128), false, 4), 128);
    }
}
