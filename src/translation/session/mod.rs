mod protocol;
#[cfg(test)]
mod tests;
mod worker;
pub use protocol::serve;
use worker::Worker;
// Ordered subprocess workers with private session state.
use super::{engine::Translator, filter::Pipeline};
use anyhow::Result;
const MAX_MESSAGE: usize = 16 * 1024 * 1024;
pub struct Session {
    inner: SessionKind,
}
enum SessionKind {
    Local {
        pipeline: Option<Pipeline>,
        engine: Box<dyn Translator>,
    },
    Pool(Vec<Worker>),
}
impl Session {
    pub fn independent_rows(&self) -> bool {
        match &self.inner {
            SessionKind::Local { engine, .. } => engine.independent_rows(),
            SessionKind::Pool(_) => false,
        }
    }
    pub fn local(pipeline: Pipeline, engine: Box<dyn Translator>) -> Self {
        Self {
            inner: SessionKind::Local {
                pipeline: Some(pipeline),
                engine,
            },
        }
    }
    pub fn direct(engine: Box<dyn Translator>) -> Self {
        Self {
            inner: SessionKind::Local {
                pipeline: None,
                engine,
            },
        }
    }
    pub fn parallel(args: &[String], count: usize) -> Result<Self> {
        let mut workers = Vec::new();
        for _ in 0..count.clamp(1, 64) {
            workers.push(Worker::spawn(args)?);
        }
        Ok(Self {
            inner: SessionKind::Pool(workers),
        })
    }
    pub fn translate(&mut self, text: &str) -> Result<String> {
        match &mut self.inner {
            SessionKind::Local { pipeline, engine } => match pipeline {
                Some(pipeline) => pipeline.translate(&mut **engine, text),
                None => engine.translate(text),
            },
            SessionKind::Pool(workers) => {
                workers[0].send(text)?;
                workers[0].translation()
            }
        }
    }
    pub fn translate_lines(&mut self, lines: &[String]) -> Result<Vec<String>> {
        match &mut self.inner {
            SessionKind::Local { pipeline, engine } => match pipeline {
                Some(pipeline) => lines
                    .iter()
                    .map(|text| pipeline.translate(&mut **engine, text))
                    .collect(),
                None => engine.translate_lines(lines),
            },
            SessionKind::Pool(workers) => {
                let mut result = Vec::with_capacity(lines.len());
                for chunk in lines.chunks(workers.len()) {
                    let mut failure = None;
                    let mut sent = 0;
                    for (worker, text) in workers.iter_mut().zip(chunk) {
                        match worker.send(text) {
                            Ok(()) => sent += 1,
                            Err(error) => {
                                failure = Some(error);
                                break;
                            }
                        }
                    }
                    // Drain every submitted response, including after an error,
                    // so a later task cannot consume the previous task's result.
                    for worker in workers.iter_mut().take(sent) {
                        match worker.translation() {
                            Ok(text) => result.push(text),
                            Err(error) if failure.is_none() => failure = Some(error),
                            Err(_) => {}
                        }
                    }
                    if let Some(error) = failure {
                        return Err(error);
                    }
                }
                Ok(result)
            }
        }
    }
    pub fn translate_lines_into(
        &mut self,
        lines: &[String],
        emit: &mut dyn FnMut(usize, &str) -> Result<()>,
    ) -> Result<()> {
        let mut next = 0;
        self.translate_lines_unchecked(lines, &mut |index, text| {
            anyhow::ensure!(
                index == next && index < lines.len(),
                "번역 결과 순서가 잘못되었습니다"
            );
            emit(index, text)?;
            next += 1;
            Ok(())
        })?;
        anyhow::ensure!(next == lines.len(), "번역 결과가 누락되었습니다");
        Ok(())
    }
    fn translate_lines_unchecked(
        &mut self,
        lines: &[String],
        emit: &mut dyn FnMut(usize, &str) -> Result<()>,
    ) -> Result<()> {
        match &mut self.inner {
            SessionKind::Local {
                pipeline: None,
                engine,
            } => engine.translate_lines_into(lines, emit),
            SessionKind::Local {
                pipeline: Some(pipeline),
                engine,
            } => {
                for (index, source) in lines.iter().enumerate() {
                    emit(index, &pipeline.translate(&mut **engine, source)?)?;
                }
                Ok(())
            }
            SessionKind::Pool(workers) => {
                let count = workers.len();
                for (chunk_index, chunk) in lines.chunks(count).enumerate() {
                    let mut sent = 0;
                    let mut failure = None;
                    for (worker, source) in workers.iter_mut().zip(chunk) {
                        match worker.send(source) {
                            Ok(()) => sent += 1,
                            Err(error) => {
                                failure = Some(error);
                                break;
                            }
                        }
                    }
                    let mut prefix_valid = true;
                    for (index, worker) in workers.iter_mut().take(sent).enumerate() {
                        let result = worker.translation().and_then(|text| {
                            if prefix_valid {
                                emit(chunk_index * count + index, &text)?;
                            }
                            Ok(())
                        });
                        if let Err(error) = result {
                            prefix_valid = false;
                            if failure.is_none() {
                                failure = Some(error);
                            }
                        }
                    }
                    if let Some(error) = failure {
                        return Err(error);
                    }
                }
                Ok(())
            }
        }
    }
}
