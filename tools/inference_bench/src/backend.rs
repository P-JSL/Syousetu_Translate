use anyhow::{ensure, Context, Result};
use ct2rs::sys::{ComputeType, Config, Device, TranslationOptions, Translator};
use sentencepiece_rs::SentencePieceProcessor;
use std::{
    collections::{HashMap, VecDeque},
    fs,
    path::Path,
};
use syousetu_translate::{
    learning::Memory,
    translation::{engine, prose},
};

#[path = "../../../src/offline/text.rs"]
mod text;

pub struct Backend {
    translator: Translator,
    tokenizer: SentencePieceProcessor,
    memory: Memory,
    options: TranslationOptions<String, String>,
    window_rows: usize,
    cache: HashMap<String, String>,
    order: VecDeque<String>,
}

enum Piece {
    Literal(String),
    Generate(usize),
}
struct Part {
    source: String,
    tokens: Vec<String>,
}

impl Backend {
    pub fn open(root: &Path, threads: usize, batch: usize) -> Result<Self> {
        Self::configured(root, threads, batch, 1, batch)
    }
    pub fn configured(
        root: &Path,
        threads: usize,
        batch: usize,
        replicas: usize,
        window_rows: usize,
    ) -> Result<Self> {
        ensure!((1..=64).contains(&threads), "threads must be 1..64");
        ensure!((1..=256).contains(&batch), "batch must be 1..256");
        ensure!((1..=64).contains(&replicas), "replicas must be 1..64");
        ensure!(threads * replicas <= 64, "invalid CPU budget");
        ensure!(
            (1..=8192).contains(&window_rows),
            "window rows must be 1..8192"
        );
        let vocab: Vec<String> =
            serde_json::from_slice(&fs::read(root.join("shared_vocabulary.json"))?)?;
        ensure!(vocab.len() == 128112, "unexpected vocabulary");
        let suppressed = vocab
            .iter()
            .enumerate()
            .filter(|(id, _)| (*id < 4 && *id != 2) || *id >= 128004)
            .map(|(_, token)| vec![token.clone()])
            .collect();
        Ok(Self {
            translator: Translator::new(
                root,
                &Config {
                    device: Device::CPU,
                    compute_type: ComputeType::INT8_FLOAT32,
                    num_threads_per_replica: threads,
                    device_indices: vec![0; replicas],
                    ..Default::default()
                },
            )?,
            tokenizer: SentencePieceProcessor::open(root.join("sentencepiece.bpe.model"))?,
            memory: Memory::default(),
            options: TranslationOptions {
                beam_size: 1,
                min_decoding_length: 0,
                max_decoding_length: 512,
                max_input_length: 0,
                no_repeat_ngram_size: 3,
                disable_unk: true,
                max_batch_size: batch,
                suppress_sequences: suppressed,
                ..Default::default()
            },
            window_rows,
            cache: HashMap::new(),
            order: VecDeque::new(),
        })
    }

    fn split(&self, source: &str) -> Result<Vec<Part>> {
        let tokens = self.tokenizer.encode(&prose::japanese::expand(source))?;
        if tokens.len() <= 192 {
            return Ok(vec![Part {
                source: source.to_owned(),
                tokens,
            }]);
        }
        let boundary = text::split_boundary(source).context("cannot split long source")?;
        let mut parts = self.split(&source[..boundary])?;
        parts.extend(self.split(&source[boundary..])?);
        Ok(parts)
    }

    fn translate_window(&mut self, lines: &[String]) -> Result<Vec<String>> {
        let mut parts = Vec::<Part>::new();
        let mut unique = HashMap::<String, usize>::new();
        let mut rows = Vec::with_capacity(lines.len());
        for source in lines {
            let mut pieces = Vec::new();
            if let Some(cached) = self.cache.get(source) {
                pieces.push(Piece::Literal(cached.clone()));
            } else if let Some(corrected) = self.memory.resolve(source) {
                pieces.push(Piece::Literal(corrected.into_owned()));
            } else {
                for (line_index, line) in source.split('\n').enumerate() {
                    if line_index > 0 {
                        pieces.push(Piece::Literal("\n".into()));
                    }
                    if let Some(corrected) = self.memory.resolve(line) {
                        pieces.push(Piece::Literal(corrected.into_owned()));
                    } else if !text::japanese(line) {
                        pieces.push(Piece::Literal(line.to_owned()));
                    } else {
                        for (part_index, part) in self.split(line)?.into_iter().enumerate() {
                            if part_index > 0 {
                                pieces.push(Piece::Literal(" ".into()));
                            }
                            if let Some(corrected) = self.memory.resolve(&part.source) {
                                pieces.push(Piece::Literal(corrected.into_owned()));
                            } else {
                                let id = *unique.entry(part.source.clone()).or_insert_with(|| {
                                    let id = parts.len();
                                    parts.push(part);
                                    id
                                });
                                pieces.push(Piece::Generate(id));
                            }
                        }
                    }
                }
            }
            rows.push(pieces);
        }
        let sources = parts
            .iter()
            .map(|part| {
                let mut source = Vec::with_capacity(part.tokens.len() + 2);
                source.push("__ko__".to_owned());
                source.extend(part.tokens.iter().cloned());
                source.push("</s>".to_owned());
                source
            })
            .collect::<Vec<_>>();
        let generated = if sources.is_empty() {
            Vec::new()
        } else {
            self.translator
                .translate_batch(&sources, &self.options, None)?
        };
        ensure!(generated.len() == parts.len(), "missing generated rows");
        let mut translated = Vec::with_capacity(parts.len());
        for (part, result) in parts.iter().zip(generated) {
            let tokens = result.output().context("no hypothesis")?;
            let limit = (part.tokens.len() * 4 + 32).clamp(64, 512);
            ensure!(
                tokens.len() < limit,
                "generation limit reached; refusing partial translation"
            );
            let decoded = self.tokenizer.decode(tokens)?.trim().to_owned();
            ensure!(
                !decoded.is_empty() || !part.source.chars().any(char::is_alphabetic),
                "empty translation"
            );
            translated.push(
                self.memory
                    .post_edit(&part.source, &prose::finish(&part.source, &decoded))?,
            );
        }
        let results = rows
            .into_iter()
            .map(|pieces| {
                let mut output = String::new();
                for piece in pieces {
                    match piece {
                        Piece::Literal(value) => output.push_str(&value),
                        Piece::Generate(index) => output.push_str(&translated[index]),
                    }
                }
                output
            })
            .collect::<Vec<_>>();
        for (source, output) in lines.iter().zip(&results) {
            if source.len() > 4096 || output.len() > 8192 {
                continue;
            }
            if self.cache.contains_key(source) {
                continue;
            }
            if self.cache.len() >= 1024 {
                if let Some(old) = self.order.pop_front() {
                    self.cache.remove(&old);
                }
            }
            self.cache.insert(source.clone(), output.clone());
            self.order.push_back(source.clone());
        }
        Ok(results)
    }
}

impl engine::Translator for Backend {
    fn translate(&mut self, source: &str) -> Result<String> {
        self.translate_window(&[source.to_owned()])?
            .pop()
            .context("missing translation")
    }
    fn translate_lines_into(
        &mut self,
        lines: &[String],
        emit: &mut dyn FnMut(usize, &str) -> Result<()>,
    ) -> Result<()> {
        for (window_index, window) in lines.chunks(self.window_rows).enumerate() {
            for (index, translated) in self.translate_window(window)?.iter().enumerate() {
                emit(window_index * self.window_rows + index, translated)?;
            }
        }
        Ok(())
    }
    fn translate_lines(&mut self, lines: &[String]) -> Result<Vec<String>> {
        let mut results = Vec::with_capacity(lines.len());
        self.translate_lines_into(lines, &mut |_, value| {
            results.push(value.to_owned());
            Ok(())
        })?;
        Ok(results)
    }
}
