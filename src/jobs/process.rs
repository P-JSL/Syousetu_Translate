//! Ordered desktop tasks replacing Java TranslateTask/ProgressController.
//! Each task uses a configuration snapshot and the same Rust CLI as headless use.
use crate::storage::atomic_write;
use anyhow::{Context, Result};
use std::{
    fs,
    io::{BufRead, BufReader, Read},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::SyncSender,
    },
    thread,
};

use super::{Job, Queue};
static TEMP_ID: AtomicU64 = AtomicU64::new(0);
pub(super) struct Message {
    pub(super) id: u64,
    pub(super) line: Option<String>,
    pub(super) stdout: bool,
}
pub(super) struct Running {
    pub(super) id: u64,
    pub(super) process: Child,
    config_dir: PathBuf,
}
impl Drop for Running {
    fn drop(&mut self) {
        if !matches!(self.process.try_wait(), Ok(Some(_))) {
            let _ = self.process.kill();
            let _ = self.process.wait();
        }
        let _ = fs::remove_file(self.config_dir.join("config.properties"));
        let _ = fs::remove_dir(&self.config_dir);
    }
}

fn read_output(
    id: u64,
    stdout: bool,
    input: impl Read + Send + 'static,
    sender: SyncSender<Message>,
) {
    thread::spawn(move || {
        let mut reader = BufReader::new(input);
        loop {
            let mut line = Vec::new();
            match reader.by_ref().take(32 * 1024).read_until(b'\n', &mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let line = String::from_utf8_lossy(&line).trim_end().to_owned();
                    if sender
                        .send(Message {
                            id,
                            line: Some(line),
                            stdout,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
        let _ = sender.send(Message {
            id,
            line: None,
            stdout,
        });
    });
}

impl Queue {
    pub(super) fn start(&self, job: &Job) -> Result<Running> {
        let mut directory = None;
        for _ in 0..100 {
            let serial = TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let candidate =
                std::env::temp_dir().join(format!("syousetu-job-{}-{serial}", std::process::id()));
            match fs::create_dir(&candidate) {
                Ok(()) => {
                    directory = Some(candidate);
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.into()),
            }
        }
        let config_dir = directory.context("작업용 임시 폴더 생성 실패")?;
        let config_path = config_dir.join("config.properties");
        let result = (|| -> Result<Child> {
            atomic_write(&config_path, &job.config.to_properties()?)?;
            let mut command = Command::new(&self.cli);
            command
                .args(&job.args)
                .arg("--config")
                .arg(&config_path)
                .current_dir(&self.directory)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW, like javaw.
            }
            command
                .spawn()
                .with_context(|| format!("Rust CLI 시작 실패: {}", self.cli.display()))
        })();
        let mut process = match result {
            Ok(process) => process,
            Err(e) => {
                let _ = fs::remove_file(&config_path);
                let _ = fs::remove_dir(&config_dir);
                return Err(e);
            }
        };
        read_output(
            job.id,
            true,
            process.stdout.take().context("CLI stdout 없음")?,
            self.sender.clone(),
        );
        read_output(
            job.id,
            false,
            process.stderr.take().context("CLI stderr 없음")?,
            self.sender.clone(),
        );
        Ok(Running {
            id: job.id,
            process,
            config_dir,
        })
    }
}
