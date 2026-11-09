mod events;
mod process;
use process::{Message, Running};
// Ordered desktop tasks replacing Java TranslateTask/ProgressController.
// Each task uses a configuration snapshot and the same Rust CLI as headless use.
use crate::config::Config;
use anyhow::{bail, Result};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, SyncSender},
};

const MAX_LOG: usize = 16 * 1024;
const MAX_OUTPUT: usize = 4 * 1024 * 1024;

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

    pub fn poll(&mut self) {
        // Consume the bounded pipe readers before checking process completion.
        let active = self
            .running
            .as_ref()
            .and_then(|running| self.jobs.iter().position(|job| job.id == running.id));
        while let Ok(message) = self.receiver.try_recv() {
            let index = active
                .filter(|&i| self.jobs[i].id == message.id)
                .or_else(|| self.jobs.iter().position(|job| job.id == message.id));
            if let Some(index) = index {
                self.jobs[index].apply_message(message);
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
