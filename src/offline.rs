//! Direct Japanese→Korean CPU translation in Rust. Model preparation alone
//! accesses the network. Runners share Q8 weights and own independent caches.
#[path = "m2m.rs"]
mod m2m;
use crate::{learning::Memory, models, ordered};
use anyhow::{bail, Context, Result};
use candle_core::{DType, Device};
use candle_nn::VarBuilder;
use sentencepiece_rs::SentencePieceProcessor;
use std::{
    collections::{HashMap, VecDeque},
    fs,
    path::Path,
    sync::Arc,
};

struct Tokenizer {
    sp: SentencePieceProcessor,
    vocab: HashMap<String, u32>,
    inverse: HashMap<u32, String>,
    ja: u32,
    ko: u32,
}
impl Tokenizer {
    fn open(root: &Path) -> Result<Self> {
        let sp = SentencePieceProcessor::open(root.join("sentencepiece.bpe.model"))?;
        let vocab: HashMap<String, u32> =
            serde_json::from_slice(&fs::read(root.join("vocab.json"))?)?;
        let inverse = vocab
            .iter()
            .map(|(piece, id)| (*id, piece.clone()))
            .collect();
        // Official M2M100 language order. Shared vocab ids differ from SPM ids.
        let size = vocab.len() as u32;
        Ok(Self {
            sp,
            vocab,
            inverse,
            ja: size + 42,
            ko: size + 48,
        })
    }
    fn encode(&self, text: &str) -> Result<Vec<u32>> {
        Ok(self
            .sp
            .encode(text)?
            .iter()
            .map(|p| self.vocab.get(p).copied().unwrap_or(3))
            .collect())
    }
    fn decode(&self, ids: &[u32]) -> Result<String> {
        let pieces = ids
            .iter()
            .filter(|&&id| id >= 4)
            .filter_map(|id| self.inverse.get(id))
            .map(String::as_str)
            .collect::<Vec<_>>();
        Ok(self.sp.decode(&pieces)?.trim().to_owned())
    }
    fn split(&self, text: &str) -> Result<Vec<String>> {
        if self.encode(text)?.len() <= 192 {
            return Ok(vec![text.to_owned()]);
        }
        let boundary =
            split_boundary(text).context("모델 입력 한도에 맞게 원문을 분할할 수 없습니다")?;
        let mut parts = self.split(&text[..boundary])?;
        parts.extend(self.split(&text[boundary..])?);
        Ok(parts)
    }
}
struct Runner {
    model: m2m::Model,
    config: m2m::Config,
    tokenizer: Arc<Tokenizer>,
    memory: Arc<Memory>,
    pool: Arc<rayon::ThreadPool>,
    cache: HashMap<String, String>,
    cache_order: VecDeque<String>,
    small: bool,
}
impl Runner {
    fn translate(&mut self, source: &str) -> Result<String> {
        let pool = Arc::clone(&self.pool);
        pool.install(|| self.translate_inner(source))
    }
    fn translate_inner(&mut self, source: &str) -> Result<String> {
        if let Some(text) = self.memory.lookup(source) {
            return Ok(text.to_owned());
        }
        if !japanese(source) {
            return Ok(source.to_owned());
        }
        if let Some(cached) = self.cache.get(source) {
            return Ok(cached.clone());
        }
        let mut lines = Vec::new();
        for line in source.split('\n') {
            if let Some(text) = self.memory.lookup(line) {
                lines.push(text.to_owned());
                continue;
            }
            if !japanese(line) {
                lines.push(line.to_owned());
                continue;
            }
            let mut translated = Vec::new();
            for part in self.tokenizer.split(line)? {
                if let Some(text) = self.memory.lookup(&part) {
                    translated.push(text.to_owned());
                    continue;
                }
                let korean = self.generate(&part)?;
                translated.push(self.memory.post_edit(&part, &korean)?);
            }
            lines.push(translated.join(" "));
        }
        let result = lines.join("\n");
        if source.len() <= 4096 && result.len() <= 8192 {
            if self.cache.len() >= 512 {
                if let Some(old) = self.cache_order.pop_front() {
                    self.cache.remove(&old);
                }
            }
            self.cache.insert(source.to_owned(), result.clone());
            self.cache_order.push_back(source.to_owned());
        }
        Ok(result)
    }
    fn generate(&mut self, text: &str) -> Result<String> {
        self.generate_at(text, 0)
    }
    fn generate_at(&mut self, text: &str, depth: usize) -> Result<String> {
        self.model.reset();
        let result = self.generate_inner(text);
        self.model.reset();
        if result.as_ref().is_err_and(|e| e.is::<GenerationLimit>())
            && depth < 3
            && text.chars().count() >= 16
        {
            if let Some(boundary) = split_boundary(text) {
                let first = self.generate_at(&text[..boundary], depth + 1)?;
                let last = self.generate_at(&text[boundary..], depth + 1)?;
                return Ok(format!("{} {}", first.trim(), last.trim()));
            }
        }
        result
    }
    fn generate_inner(&mut self, text: &str) -> Result<String> {
        let tokens = self.tokenizer.encode(text)?;
        let limit = (tokens.len() * 4 + 32)
            .clamp(64, 512)
            .min(self.config.max_position_embeddings - 2);
        let mut source = vec![if self.small {
            self.tokenizer.ko
        } else {
            self.tokenizer.ja
        }];
        source.extend(tokens);
        source.push(self.config.eos_token_id);
        let encoder = self.model.encode(&source)?;
        let mut input = vec![self.config.decoder_start_token_id];
        if !self.small {
            input.push(self.tokenizer.ko);
        }
        let mut output = Vec::new();
        let mut past = 0;
        for _ in 0..limit {
            let scores = self.model.decode(&input, &encoder, past)?;
            past += input.len();
            let blocked = repeated_trigram_tokens(&output);
            let next = scores
                .iter()
                .enumerate()
                .filter(|(id, score)| {
                    (*id >= 4 || *id == self.config.eos_token_id as usize)
                        && *id < self.tokenizer.vocab.len()
                        && score.is_finite()
                        && !blocked.contains(&(*id as u32))
                })
                .max_by(|a, b| a.1.total_cmp(b.1).then_with(|| b.0.cmp(&a.0)))
                .map(|(id, _)| id as u32)
                .context("모델이 유효한 출력 토큰을 생성하지 못했습니다")?;
            if next == self.config.eos_token_id {
                let decoded = self.tokenizer.decode(&output)?;
                if decoded.is_empty() && text.chars().any(char::is_alphabetic) {
                    bail!("모델이 빈 번역을 생성했습니다");
                }
                return Ok(decoded);
            }
            output.push(next);
            input = vec![next];
        }
        Err(GenerationLimit(limit).into())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParallelPolicy {
    Auto,
    Serial,
    Parallel,
}
impl std::str::FromStr for ParallelPolicy {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "auto" => Ok(Self::Auto),
            "serial" => Ok(Self::Serial),
            "parallel" => Ok(Self::Parallel),
            _ => bail!("병렬 모드는 auto|serial|parallel이어야 합니다"),
        }
    }
}
pub struct OfflineTranslator {
    runners: Vec<Runner>,
    single_pool: Arc<rayon::ThreadPool>,
    parallel: Option<bool>,
}
impl OfflineTranslator {
    pub fn open(root: &Path, memory: Memory, threads: usize) -> Result<Self> {
        Self::open_with_policy(root, memory, threads, ParallelPolicy::Auto)
    }
    pub fn open_with_policy(
        root: &Path,
        memory: Memory,
        threads: usize,
        policy: ParallelPolicy,
    ) -> Result<Self> {
        if usize::BITS < 64 {
            bail!("내부 모델은 64비트 실행 파일이 필요합니다");
        }
        eprintln!("직접 일→한 모델 체크섬 확인 중...");
        models::verify(root)?;
        let directory = root.join("ja-ko");
        let config: m2m::Config =
            serde_json::from_slice(&fs::read(directory.join("config.json"))?)?;
        let tokenizer = Arc::new(Tokenizer::open(&directory)?);
        let budget = threads.clamp(1, 8);
        let count = budget.min(2);
        let load_pool = Arc::new(
            rayon::ThreadPoolBuilder::new()
                .num_threads(budget)
                .build()?,
        );
        eprintln!("직접 일→한 모델 로딩 및 Q8 변환 중...");
        let model = load_pool.install(|| -> Result<_> {
            let vb = VarBuilder::from_buffered_safetensors(
                fs::read(directory.join("model.safetensors"))?,
                DType::F32,
                &Device::Cpu,
            )?;
            Ok(m2m::Model::new(&config, vb)?)
        })?;
        let small = config.decoder_layers == 3;
        let memory = Arc::new(memory);
        let mut runners = Vec::new();
        // Immutable quantized weights are shared; per-runner CPU pools sum to
        // the total budget. This avoids nested unlimited worker pools.
        for index in 0..count {
            let threads = budget / count + usize::from(index < budget % count);
            runners.push(Runner {
                model: model.clone(),
                config: config.clone(),
                tokenizer: Arc::clone(&tokenizer),
                memory: Arc::clone(&memory),
                pool: Arc::new(
                    rayon::ThreadPoolBuilder::new()
                        .num_threads(threads)
                        .build()?,
                ),
                cache: HashMap::new(),
                cache_order: VecDeque::new(),
                small,
            });
        }
        eprintln!("직접 일→한 모델 준비 완료 · 작업자 {count}개 / CPU {budget}개 · 교정 문장 {}개 / 용어 {}개",memory.sentences.len(),memory.terms.len());
        Ok(Self {
            runners,
            single_pool: if budget == 1 {
                load_pool
            } else {
                Arc::new(rayon::ThreadPoolBuilder::new().num_threads(1).build()?)
            },
            parallel: if budget == 1 || policy == ParallelPolicy::Serial {
                Some(false)
            } else if policy == ParallelPolicy::Parallel {
                Some(true)
            } else {
                None
            },
        })
    }
    pub fn translate(&mut self, text: &str) -> Result<String> {
        // The CPU budget is a cap. Short decode operations often run faster
        // with one CPU than with nested matrix parallelism.
        let pool = Arc::clone(&self.single_pool);
        pool.install(|| self.runners[0].translate_inner(text))
    }
    pub fn translate_lines(&mut self, lines: &[String]) -> Result<Vec<String>> {
        self.translate_lines_with_progress(lines, |_, _| {})
    }
    fn select_parallel(&mut self, lines: &[String]) -> Result<bool> {
        if let Some(selected) = self.parallel {
            return Ok(selected);
        }
        let candidates = lines
            .iter()
            .filter(|s| japanese(s) && self.runners[0].memory.lookup(s).is_none())
            .collect::<Vec<_>>();
        if candidates.len() < 16 {
            return Ok(false);
        }
        let mut seen = std::collections::HashSet::new();
        let samples = candidates
            .into_iter()
            .filter(|s| seen.insert(s.as_str()))
            .take(4)
            .cloned()
            .collect::<Vec<_>>();
        if samples.len() < 4 {
            return Ok(false);
        }
        eprintln!("처음 본문 묶음의 단일·병렬 처리 성능과 결과 일치를 확인합니다...");
        for runner in &mut self.runners {
            for source in &samples {
                runner.cache.remove(source);
            }
            runner.cache_order.retain(|key| !samples.contains(key));
        }
        let start = std::time::Instant::now();
        let expected = samples
            .iter()
            .map(|s| self.translate(s))
            .collect::<Result<Vec<_>>>()?;
        let serial_seconds = start.elapsed().as_secs_f64();
        for runner in &mut self.runners {
            for source in &samples {
                runner.cache.remove(source);
            }
            runner.cache_order.retain(|key| !samples.contains(key));
        }
        let start = std::time::Instant::now();
        let candidate = ordered::map(&mut self.runners, &samples, Runner::translate);
        let parallel_seconds = start.elapsed().as_secs_f64();
        let matches = candidate.as_ref().is_ok_and(|actual| actual == &expected);
        let selected = matches && parallel_seconds < serial_seconds * 0.9;
        if !selected {
            for runner in &mut self.runners {
                for source in &samples {
                    runner.cache.remove(source);
                }
                runner.cache_order.retain(|key| !samples.contains(key));
            }
            for (source, text) in samples.iter().zip(expected) {
                self.runners[0].cache.insert(source.clone(), text);
                self.runners[0].cache_order.push_back(source.clone());
            }
        }
        self.parallel = Some(selected);
        eprintln!(
            "처리 방식 선택: {} · 단일 {:.2}초 / 병렬 {:.2}초 · 결과 일치 {}",
            if selected { "병렬" } else { "단일" },
            serial_seconds,
            parallel_seconds,
            matches
        );
        Ok(selected)
    }
    pub fn translate_lines_with_progress(
        &mut self,
        lines: &[String],
        progress: impl Fn(usize, usize) + Sync,
    ) -> Result<Vec<String>> {
        let parallel = self.select_parallel(lines)?;
        let completed = std::sync::Mutex::new(0usize);
        progress(0, lines.len());
        if !parallel {
            let mut output = Vec::with_capacity(lines.len());
            for text in lines {
                output.push(
                    self.translate(text)
                        .with_context(|| format!("본문 {}번째 줄 번역 실패", output.len() + 1))?,
                );
                progress(output.len(), lines.len());
            }
            return Ok(output);
        }
        ordered::map(&mut self.runners, lines, |runner, text| {
            let output = runner.translate(text)?;
            // Serialize progress reporting so concurrent completions cannot
            // cause the displayed counter to move backwards.
            let mut done = completed
                .lock()
                .map_err(|_| anyhow::anyhow!("진행률 잠금 오류"))?;
            *done += 1;
            progress(*done, lines.len());
            Ok(output)
        })
    }
}
#[derive(Debug)]
struct GenerationLimit(usize);
impl std::fmt::Display for GenerationLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "모델 출력이 {}토큰 한도에 도달했습니다. 불완전한 번역은 저장하지 않습니다",
            self.0
        )
    }
}
impl std::error::Error for GenerationLimit {}
fn repeated_trigram_tokens(output: &[u32]) -> std::collections::HashSet<u32> {
    if output.len() < 2 {
        return std::collections::HashSet::new();
    }
    let prefix = &output[output.len() - 2..];
    output
        .windows(3)
        .filter(|triple| triple[..2] == *prefix)
        .map(|triple| triple[2])
        .collect()
}
fn japanese(text: &str) -> bool {
    text.chars()
        .any(|c| matches!(c,'\u{3040}'..='\u{30ff}'|'\u{3400}'..='\u{9fff}'))
}
fn split_boundary(text: &str) -> Option<usize> {
    let positions = text.char_indices().collect::<Vec<_>>();
    let mid = positions.len() / 2;
    if positions.len() < 2 {
        return None;
    }
    positions
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(i, _)| *i >= mid / 2 && *i <= mid + mid / 2)
        .min_by_key(|(i, (_, c))| {
            (
                if "。！？.!?\n".contains(*c) {
                    0
                } else if c.is_whitespace() || "、,;；".contains(*c) {
                    1
                } else {
                    2
                },
                i.abs_diff(mid),
            )
        })
        .map(|(_, (byte, c))| byte + c.len_utf8())
        .filter(|&byte| byte < text.len())
        .or_else(|| positions.get(mid.max(1)).map(|(byte, _)| *byte))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repetition_guard_blocks_only_previously_completed_trigrams() {
        assert!(repeated_trigram_tokens(&[1, 2]).is_empty());
        assert_eq!(repeated_trigram_tokens(&[1, 2, 3, 4, 1, 2]), [3].into());
        assert!(repeated_trigram_tokens(&[1, 2, 3, 4, 2, 1]).is_empty());
    }
    #[test]
    #[ignore = "requires pinned model assets; token ids are from official Transformers v4.46.3"]
    fn tokenization_matches_official_small100_language_prefix_and_bpe_ids() {
        let tokenizer = Tokenizer::open(&models::default_dir().join("ja-ko")).unwrap();
        assert_eq!(tokenizer.ko, 128052);
        assert_eq!(
            tokenizer.encode("私は学生です。").unwrap(),
            [77828, 13261, 2357, 37]
        );
        assert_eq!(
            tokenizer.encode("今日は雨が降っています。").unwrap(),
            [20511, 30299, 26062, 413, 36393, 23654, 37]
        );
        assert_eq!(
            tokenizer.encode("こんにちは。").unwrap(),
            [22, 13552, 8042, 474, 14309, 545, 37]
        );
    }
    #[test]
    fn unicode_splits_preserve_every_character() {
        for s in [
            "日本語の長い文です。次の文です。",
            "😀😀あいうえお",
            "abc def ghi",
            "長文",
        ] {
            let b = split_boundary(s).unwrap();
            assert!(b > 0 && b < s.len() && s.is_char_boundary(b));
            assert_eq!(format!("{}{}", &s[..b], &s[b..]), s);
        }
        assert_eq!(split_boundary("あ"), None);
    }
}
