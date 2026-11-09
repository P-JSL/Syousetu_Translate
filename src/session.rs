//! Ordered subprocess workers, isolating one legacy DLL per process.
use crate::{engine::Translator, filter::Pipeline};
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, BufWriter, Read, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};
const MAX_MESSAGE: usize = 16 * 1024 * 1024;
const WORKER_TIMEOUT: Duration = Duration::from_secs(30);
pub enum Session {
    Local {
        pipeline: Option<Pipeline>,
        engine: Box<dyn Translator>,
    },
    Pool(Vec<Worker>),
}
impl Session {
    pub fn local(pipeline: Pipeline, engine: Box<dyn Translator>) -> Self {
        Self::Local { pipeline: Some(pipeline), engine }
    }
    pub fn direct(engine: Box<dyn Translator>) -> Self {Self::Local {pipeline: None, engine}}
    pub fn parallel(args: &[String], count: usize) -> Result<Self> {
        let mut workers = Vec::new();
        for _ in 0..count.clamp(1, 16) {
            workers.push(Worker::spawn(args)?);
        }
        Ok(Self::Pool(workers))
    }
    pub fn translate(&mut self, text: &str) -> Result<String> {
        match self {
            Self::Local { pipeline, engine } => match pipeline {
                Some(pipeline) => pipeline.translate(&mut **engine, text),
                None => engine.translate(text),
            },
            Self::Pool(workers) => {
                workers[0].send(text)?;
                workers[0].translation()
            }
        }
    }
    pub fn translate_lines(&mut self, lines: &[String]) -> Result<Vec<String>> {
        match self {
            Self::Local { pipeline, engine } => match pipeline {
                Some(pipeline) => lines.iter().map(|text|pipeline.translate(&mut **engine,text)).collect(),
                None => engine.translate_lines(lines),
            },
            Self::Pool(workers) => {
                let mut result = Vec::with_capacity(lines.len());
                for chunk in lines.chunks(workers.len()) {
                    let mut failure = None;
                    let mut sent = 0;
                    for (worker, text) in workers.iter_mut().zip(chunk) {
                        match worker.send(text) {
                            Ok(()) => sent += 1,
                            Err(error) => { failure = Some(error); break; }
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
                    if let Some(error) = failure { return Err(error); }
                }
                Ok(result)
            }
        }
    }
}
pub struct Worker {
    process: Child,
    input: BufWriter<ChildStdin>,
    responses: Receiver<std::result::Result<Value, String>>,
}
impl Worker {
    fn spawn(args: &[String]) -> Result<Self> {
        let mut command = Command::new(std::env::current_exe()?);
        command.arg("worker")
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        #[cfg(windows)] {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let mut process = command.spawn()
            .context("번역 worker 프로세스 시작 실패")?;
        let input = BufWriter::new(process.stdin.take().context("worker stdin 없음")?);
        let output = process.stdout.take().context("worker stdout 없음")?;
        let (sender, responses) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let mut reader = BufReader::new(output);
            loop {
                let mut message = String::new();
                let response = match reader
                    .by_ref()
                    .take(MAX_MESSAGE as u64 + 1)
                    .read_line(&mut message)
                {
                    Ok(0) => break,
                    Ok(_) if message.len() > MAX_MESSAGE || !message.ends_with('\n') => {
                        Err("worker 응답 용량 초과 또는 불완전한 응답".into())
                    }
                    Ok(_) => serde_json::from_str(&message)
                        .map_err(|e| format!("worker 응답 해석 실패: {e}")),
                    Err(e) => Err(format!("worker 읽기 실패: {e}")),
                };
                let failed = response.is_err();
                if sender.send(response).is_err() || failed {
                    break;
                }
            }
        });
        let mut worker = Self {
            process,
            input,
            responses,
        };
        let ready = worker.receive()?;
        if ready.get("ready").and_then(Value::as_bool) != Some(true) {
            bail!(
                "번역 worker 초기화 실패: {}",
                ready
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("준비 응답 없음")
            );
        }
        Ok(worker)
    }
    fn send(&mut self, text: &str) -> Result<()> {
        let message = serde_json::to_string(text)?;
        if message.len() >= MAX_MESSAGE {
            bail!("worker 입력 용량 초과");
        }
        writeln!(self.input, "{message}").context("worker 입력 실패")?;
        self.input.flush().context("worker 입력 전송 실패")
    }
    fn receive(&mut self) -> Result<Value> {
        match self.responses.recv_timeout(WORKER_TIMEOUT) {
            Ok(response) => response.map_err(anyhow::Error::msg),
            Err(mpsc::RecvTimeoutError::Timeout) => bail!("번역 worker 응답 대기 시간이 30초를 초과했습니다"),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let status = self.process.try_wait()?.map(|s| s.to_string())
                    .unwrap_or_else(|| "출력 연결 종료 (종료 코드 미확인)".into());
                bail!("번역 worker가 응답 전에 종료되었습니다: {status}. 바로 앞의 엔진 초기화 오류를 확인해 주세요");
            }
        }
    }
    fn translation(&mut self) -> Result<String> {
        let response = self.receive()?;
        if let Some(error) = response.get("error").and_then(Value::as_str) {
            bail!("worker 번역 실패: {error}");
        }
        response
            .get("output")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .context("worker 번역 결과가 없습니다")
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}
pub fn serve(session: Result<Session>) -> Result<()> {
    let mut output = BufWriter::new(std::io::stdout().lock());
    let mut session = match session {
        Ok(session) => session,
        Err(error) => {
            writeln!(output, "{}", serde_json::json!({"ready":false,"error":format!("{error:#}")}))?;
            output.flush()?;
            return Err(error);
        }
    };
    writeln!(output, "{{\"ready\":true}}")?;
    output.flush()?;
    let mut input = BufReader::new(std::io::stdin().lock());
    loop {
        let mut message = String::new();
        if input
            .by_ref()
            .take(MAX_MESSAGE as u64 + 1)
            .read_line(&mut message)?
            == 0
        {
            break;
        }
        if message.len() > MAX_MESSAGE || !message.ends_with('\n') {
            bail!("worker 입력 용량 초과 또는 불완전한 입력");
        }
        let translated = match serde_json::from_str::<String>(&message) {
            Ok(text) => match session.translate(&text) {
                Ok(text) => serde_json::json!({"output":text}),
                Err(error) => serde_json::json!({"error":format!("{error:#}")}),
            },
            Err(error) => serde_json::json!({"error":format!("worker JSON 입력 오류: {error}")}),
        };
        let message = serde_json::to_string(&translated)?;
        if message.len() >= MAX_MESSAGE {
            bail!("worker 결과 용량 초과");
        }
        writeln!(output, "{message}")?;
        output.flush()?;
    }
    Ok(())
}
