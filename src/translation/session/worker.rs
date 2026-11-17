use super::MAX_MESSAGE;
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, BufWriter, Read, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};
const WORKER_TIMEOUT: Duration = Duration::from_secs(30);
pub(super) struct Worker {
    process: Child,
    input: BufWriter<ChildStdin>,
    responses: Receiver<std::result::Result<Value, String>>,
}
impl Worker {
    pub(super) fn spawn(args: &[String]) -> Result<Self> {
        let mut command = Command::new(std::env::current_exe()?);
        command
            .arg("worker")
            .args(worker_arguments(args)?)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let mut process = command.spawn().context("번역 worker 프로세스 시작 실패")?;
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
    pub(super) fn send(&mut self, text: &str) -> Result<()> {
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
            Err(mpsc::RecvTimeoutError::Timeout) => {
                bail!("번역 worker 응답 대기 시간이 30초를 초과했습니다")
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let status = self
                    .process
                    .try_wait()?
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "출력 연결 종료 (종료 코드 미확인)".into());
                bail!("번역 worker가 응답 전에 종료되었습니다: {status}. 바로 앞의 엔진 초기화 오류를 확인해 주세요");
            }
        }
    }
    pub(super) fn translation(&mut self) -> Result<String> {
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

// Only the parent reads command input and writes the output file. Workers get
// each source string through stdin after they have initialized their engine.
fn worker_arguments(args: &[String]) -> Result<Vec<String>> {
    let mut forwarded = Vec::with_capacity(args.len());
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if matches!(
            arg.as_str(),
            "--text" | "--input" | "--input-encoding" | "--out"
        ) {
            args.next()
                .filter(|value| !value.starts_with("--"))
                .with_context(|| format!("옵션 {arg}의 값이 없습니다"))?;
        } else {
            forwarded.push(arg.clone());
        }
    }
    Ok(forwarded)
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::worker_arguments;

    #[test]
    fn engine_options_survive_removal_of_parent_input_and_output() {
        let args = [
            "--engine",
            "native",
            "--text",
            "private source",
            "--dictionary",
            "dictionary.tsv",
            "--input",
            "source.txt",
            "--input-encoding",
            "utf-8",
            "--out",
            "novel.txt",
            "--jobs",
            "32",
            "--device",
            "cuda",
            "--cuda-device",
            "1",
            "--no-learning",
        ]
        .map(str::to_owned);
        assert_eq!(
            worker_arguments(&args).unwrap(),
            [
                "--engine",
                "native",
                "--dictionary",
                "dictionary.tsv",
                "--jobs",
                "32",
                "--device",
                "cuda",
                "--cuda-device",
                "1",
                "--no-learning",
            ]
        );
    }

    #[test]
    fn incomplete_parent_option_is_rejected_before_spawn() {
        for args in [vec!["--text"], vec!["--out", "--jobs", "32"]] {
            let args = args.into_iter().map(str::to_owned).collect::<Vec<_>>();
            assert!(worker_arguments(&args).is_err());
        }
    }
}
