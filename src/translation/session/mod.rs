mod protocol;
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
        for _ in 0..count.clamp(1, 16) {
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
}
