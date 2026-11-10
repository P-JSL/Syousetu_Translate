use super::{runner::batched, text::japanese, OfflineTranslator};
use anyhow::{Context, Result};
use std::sync::Arc;
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
            .take(self.runners.len().clamp(4, 16))
            .cloned()
            .collect::<Vec<_>>();
        if samples.len() < 4 {
            return Ok(false);
        }
        eprintln!("처음 본문 묶음의 단일·배치 처리 성능과 결과 일치를 확인합니다...");
        self.cache.evict(&samples);
        let start = std::time::Instant::now();
        let expected = samples
            .iter()
            .map(|s| self.translate_prepared(s))
            .collect::<Result<Vec<_>>>()?;
        let serial_seconds = start.elapsed().as_secs_f64();
        self.cache.evict(&samples);
        let start = std::time::Instant::now();
        let mut candidate = vec![None; samples.len()];
        let pool = Arc::clone(&self.compute_pool);
        let result = batched::translate(&mut self.runners, &samples, &pool, |index, text| {
            candidate[index] = Some(text);
            Ok(())
        });
        let parallel_seconds = start.elapsed().as_secs_f64();
        let matches = result.is_ok()
            && candidate
                .iter()
                .zip(&expected)
                .all(|(actual, expected)| actual.as_ref() == Some(expected));
        let selected = matches && parallel_seconds < serial_seconds * 0.9;
        for (source, text) in samples.iter().zip(expected) {
            self.cache.insert(source, text);
        }
        self.parallel = Some(selected);
        eprintln!(
            "처리 방식 선택: {} · 단일 {:.2}초 / 배치 {:.2}초 · 결과 일치 {}",
            if selected { "배치 병렬" } else { "단일" },
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
        let mut output = Vec::with_capacity(lines.len());
        self.translate_lines_to(lines, progress, &mut |_, text| {
            output.push(text.to_owned());
            Ok(())
        })?;
        Ok(output)
    }
    pub fn translate_lines_to(
        &mut self,
        lines: &[String],
        progress: impl Fn(usize, usize) + Sync,
        emit: &mut dyn FnMut(usize, &str) -> Result<()>,
    ) -> Result<()> {
        let parallel = self.select_parallel(lines)?;
        progress(0, lines.len());
        if !parallel {
            for (index, text) in lines.iter().enumerate() {
                let translated = self
                    .translate_prepared(text)
                    .with_context(|| format!("본문 {}번째 줄 번역 실패", index + 1))?;
                emit(index, &translated)?;
                progress(index + 1, lines.len());
            }
            return Ok(());
        }
        let batch = super::batch::Batch::new(lines, |source| {
            self.runners[0]
                .correction(source)
                .map(std::borrow::Cow::into_owned)
                .or_else(|| {
                    if !japanese(source) {
                        Some(source.to_owned())
                    } else {
                        self.cache.get(source)
                    }
                })
        });
        progress(batch.ready, lines.len());
        let mut completed = batch.ready;
        let mut values = vec![None; batch.unique.len()];
        let mut next = 0;
        batch.emit_ready(&values, &mut next, emit)?;
        let pool = Arc::clone(&self.compute_pool);
        let cache = &mut self.cache;
        batched::translate(&mut self.runners, &batch.unique, &pool, |index, text| {
            cache.insert(&batch.unique[index], text.clone());
            completed += batch.count(&batch.unique[index]);
            values[index] = Some(text);
            batch.emit_ready(&values, &mut next, emit)?;
            progress(completed, lines.len());
            Ok(())
        })?;
        anyhow::ensure!(
            next == lines.len(),
            "번역 결과가 누락되었습니다. 완료된 줄은 출력 파일에 보존됩니다"
        );
        Ok(())
    }
}
