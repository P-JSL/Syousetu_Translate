use super::{ordered, runner::Runner, text::japanese, OfflineTranslator};
use anyhow::{Context, Result};
impl OfflineTranslator {
    fn select_parallel(&mut self, lines: &[String]) -> Result<bool> {
        if let Some(selected) = self.parallel {
            return Ok(selected);
        }
        let candidates = lines
            .iter()
            .filter(|s| japanese(s) && !self.runners[0].has_correction(s))
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
        self.cache.evict(&samples);
        let start = std::time::Instant::now();
        let expected = samples
            .iter()
            .map(|s| self.translate(s))
            .collect::<Result<Vec<_>>>()?;
        let serial_seconds = start.elapsed().as_secs_f64();
        self.cache.evict(&samples);
        let start = std::time::Instant::now();
        let candidate = ordered::map(&mut self.runners, &samples, Runner::translate);
        let parallel_seconds = start.elapsed().as_secs_f64();
        let matches = candidate.as_ref().is_ok_and(|actual| actual == &expected);
        let selected = matches && parallel_seconds < serial_seconds * 0.9;
        for (source, text) in samples.iter().zip(expected) {
            self.cache.insert(source, text);
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
        let batch = super::batch::Batch::new(lines, |source| {
            self.runners[0]
                .correction(source)
                .map(str::to_owned)
                .or_else(|| {
                    if !japanese(source) {
                        Some(source.to_owned())
                    } else {
                        self.cache.get(source)
                    }
                })
        });
        progress(batch.ready, lines.len());
        let completed = std::sync::Mutex::new(batch.ready);
        let translated = ordered::map(&mut self.runners, &batch.unique, |runner, text| {
            let output = runner.translate(text)?;
            // Serialize progress reporting so concurrent completions cannot
            // cause the displayed counter to move backwards.
            let mut done = completed
                .lock()
                .map_err(|_| anyhow::anyhow!("진행률 잠금 오류"))?;
            *done += batch.count(text);
            progress(*done, lines.len());
            Ok(output)
        })?;
        for (source, text) in batch.unique.iter().zip(&translated) {
            self.cache.insert(source, text.clone());
        }
        batch.restore(translated)
    }
}
