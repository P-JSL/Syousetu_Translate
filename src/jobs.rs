//! Ordered desktop tasks replacing Java TranslateTask/ProgressController.
//! Each task uses a configuration snapshot and the same Rust CLI as headless use.
use crate::{config::Config, storage::atomic_write};
use anyhow::{bail, Context, Result};
use std::{
    ffi::OsString,
    fs,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread,
};

const MAX_LOG: usize = 16 * 1024;
const MAX_OUTPUT: usize = 4 * 1024 * 1024;
static TEMP_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    Waiting,
    Running,
    Done,
    Error,
}

pub struct Job {
    pub id: u64,
    pub title: String,
    pub status: Status,
    pub progress: Option<(usize, usize, String)>,
    pub log: String,
    pub output: String,
    pub output_truncated: bool,
    args: Vec<OsString>,
    config: Config,
    readers_done: u8,
}

struct Message {
    id: u64,
    line: Option<String>,
    stdout: bool,
}
struct Running {
    id: u64,
    process: Child,
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

pub struct Queue {
    pub jobs: Vec<Job>,
    cli: PathBuf,
    directory: PathBuf,
    next_id: u64,
    running: Option<Running>,
    sender: SyncSender<Message>,
    receiver: Receiver<Message>,
}

impl Queue {
    pub fn new(cli: impl Into<PathBuf>, directory: impl Into<PathBuf>) -> Self {
        let (sender, receiver) = mpsc::sync_channel(128);
        Self {
            jobs: Vec::new(),
            cli: cli.into(),
            directory: directory.into(),
            next_id: 1,
            running: None,
            sender,
            receiver,
        }
    }

    pub fn enqueue(&mut self, title: String, args: Vec<OsString>, config: &Config) -> Result<u64> {
        config.to_properties()?;
        if args.is_empty() || args.iter().any(|arg| arg == "--config") {
            bail!("작업 명령이 없거나 설정 경로가 중복되었습니다");
        }
        let id = self.next_id;
        self.next_id += 1;
        self.jobs.push(Job {
            id,
            title,
            args,
            config: config.clone(),
            status: Status::Waiting,
            progress: None,
            log: String::new(),
            output: String::new(),
            output_truncated: false,
            readers_done: 0,
        });
        Ok(id)
    }

    pub fn get(&self, id: u64) -> Option<&Job> {
        self.jobs.iter().find(|job| job.id == id)
    }
    pub fn busy(&self) -> bool {
        self.jobs
            .iter()
            .any(|job| matches!(job.status, Status::Waiting | Status::Running))
    }

    pub fn move_waiting(&mut self, id: u64, direction: isize) {
        let Some(index) = self
            .jobs
            .iter()
            .position(|job| job.id == id && job.status == Status::Waiting)
        else {
            return;
        };
        if direction != -1 && direction != 1 {
            return;
        }
        if let Some(other) = index.checked_add_signed(direction) {
            if self
                .jobs
                .get(other)
                .is_some_and(|job| job.status == Status::Waiting)
            {
                self.jobs.swap(index, other);
            }
        }
    }

    pub fn remove(&mut self, id: u64) {
        self.jobs
            .retain(|job| job.id != id || job.status == Status::Running);
    }

    fn start(&self, job: &Job) -> Result<Running> {
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

    pub fn poll(&mut self) {
        // Consume the bounded pipe readers before checking process completion.
        while let Ok(message) = self.receiver.try_recv() {
            let Some(job) = self.jobs.iter_mut().find(|job| job.id == message.id) else {
                continue;
            };
            let Some(line) = message.line else {
                job.readers_done += 1;
                continue;
            };
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                if value["event"] == "progress" {
                    if let (Some(done), Some(total), Some(unit)) = (
                        value["done"].as_u64(),
                        value["total"].as_u64(),
                        value["unit"].as_str(),
                    ) {
                        if done <= total {
                            job.progress = Some((done as usize, total as usize, unit.into()));
                        }
                    }
                    continue;
                }
            }
            if message.stdout && !job.output_truncated {
                if job.output.len() + line.len() < MAX_OUTPUT {
                    job.output.push_str(&line);
                    job.output.push('\n');
                } else {
                    job.output_truncated = true;
                }
            }
            job.log.push_str(&line);
            job.log.push('\n');
            if job.log.len() > MAX_LOG {
                let mut start = job.log.len() - MAX_LOG;
                while !job.log.is_char_boundary(start) {
                    start += 1;
                }
                job.log.drain(..start);
            }
        }
        let completed =
            self.running
                .as_mut()
                .and_then(|running| match running.process.try_wait() {
                    Ok(Some(status))
                        if self
                            .jobs
                            .iter()
                            .any(|job| job.id == running.id && job.readers_done == 2) =>
                    {
                        Some((running.id, status.success(), format!("{status}")))
                    }
                    Ok(Some(_)) => None,
                    Err(e) => Some((running.id, false, e.to_string())),
                    Ok(None) => None,
                });
        if let Some((id, success, status)) = completed {
            self.running.take();
            if let Some(job) = self.jobs.iter_mut().find(|job| job.id == id) {
                job.status = if success { Status::Done } else { Status::Error };
                if !success {
                    job.log.push_str(&format!("\n작업 실패: {status}\n"));
                }
            }
        }
        if self.running.is_none() {
            if let Some(index) = self
                .jobs
                .iter()
                .position(|job| job.status == Status::Waiting)
            {
                match self.start(&self.jobs[index]) {
                    Ok(running) => {
                        self.jobs[index].status = Status::Running;
                        self.running = Some(running);
                    }
                    Err(e) => {
                        self.jobs[index].status = Status::Error;
                        self.jobs[index].log = format!("{e:#}");
                    }
                }
            }
        }
    }
}

pub fn sibling_cli(executable: &Path) -> PathBuf {
    executable.with_file_name(if cfg!(windows) {
        "syousetu-translate.exe"
    } else {
        "syousetu-translate"
    })
}
