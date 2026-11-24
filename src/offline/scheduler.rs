use super::{runner::batched, text::japanese, OfflineTranslator};
use anyhow::{Context, Result};
use std::sync::Arc;
impl OfflineTranslator {
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
        progress(0, lines.len());
        if self.parallel == Some(false) {
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
        // Single-line work uses one runner in auto mode. Multiple unique rows
        // go straight to shared matrix batches without translating a pilot twice.
        let single = match (self.parallel, batch.unique.as_slice()) {
            (None, [source]) => Some(
                self.single_pool
                    .install(|| self.runners[0].translate_inner(source))?,
            ),
            _ => None,
        };
        let pool = Arc::clone(&self.compute_pool);
        let cache = &mut self.cache;
        let mut complete = |index: usize, text: String| {
            cache.insert(&batch.unique[index], text.clone());
            completed += batch.count(&batch.unique[index]);
            values[index] = Some(text);
            batch.emit_ready(&values, &mut next, emit)?;
            progress(completed, lines.len());
            Ok(())
        };
        if let Some(text) = single {
            complete(0, text)?;
        } else {
            batched::translate(
                &mut self.runners,
                &batch.unique,
                &pool,
                self.prefill_budget,
                &mut complete,
            )?;
        }
        anyhow::ensure!(
            next == lines.len(),
            "번역 결과가 누락되었습니다. 완료된 줄은 출력 파일에 보존됩니다"
        );
        Ok(())
    }
}
