use anyhow::{Context, Result};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);

/// A transaction for a file. `commit(self)` consumes the guard; every failure
/// path closes and removes only this transaction's uniquely named temporary file.
pub struct AtomicFile {
    destination: PathBuf,
    temporary: PathBuf,
    writer: Option<BufWriter<File>>,
}
impl AtomicFile {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent)?;
        let name = path
            .file_name()
            .context("출력 파일명이 없습니다")?
            .to_string_lossy();
        for _ in 0..100 {
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let temporary = parent.join(format!(".{name}.{}.{id}.tmp", std::process::id()));
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
            {
                Ok(file) => {
                    return Ok(Self {
                        destination: path.to_owned(),
                        temporary,
                        writer: Some(BufWriter::with_capacity(64 * 1024, file)),
                    })
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.into()),
            }
        }
        anyhow::bail!("출력 임시 파일 생성 실패")
    }
    pub fn commit(mut self) -> Result<()> {
        let mut writer = self.writer.take().expect("open transaction owns a writer");
        writer.flush()?;
        writer.get_ref().sync_all()?;
        drop(writer);
        if self.destination.exists() {
            let backup = self.destination.with_extension("backup");
            anyhow::ensure!(
                !backup.exists(),
                "백업 파일이 이미 존재합니다: {}",
                backup.display()
            );
            fs::rename(&self.destination, &backup).context("백업 생성 실패")?;
            if let Err(error) = fs::rename(&self.temporary, &self.destination) {
                fs::rename(&backup, &self.destination)
                    .context("출력 교체와 원본 복구가 실패했습니다. backup 파일을 확인해 주세요")?;
                return Err(error.into());
            }
            let _ = fs::remove_file(backup);
        } else {
            fs::rename(&self.temporary, &self.destination)?;
        }
        Ok(())
    }
}
impl Write for AtomicFile {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.writer.as_mut().expect("open writer").write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.writer.as_mut().expect("open writer").flush()
    }
}
impl Drop for AtomicFile {
    fn drop(&mut self) {
        self.writer.take();
        let _ = fs::remove_file(&self.temporary);
    }
}
pub fn atomic_write(path: &Path, body: &str) -> Result<()> {
    let mut transaction = AtomicFile::new(path)?;
    transaction.write_all(body.as_bytes())?;
    transaction.commit()
}
#[cfg(test)]
mod tests {
    use super::*;
    fn directory() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "syousetu-atomic-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }
    #[test]
    fn aborted_stream_preserves_destination_and_cleans_temporary_file() {
        let dir = directory();
        let target = dir.join("novel.txt");
        fs::write(&target, "original").unwrap();
        {
            let mut file = AtomicFile::new(&target).unwrap();
            file.write_all(b"partial").unwrap();
        }
        assert_eq!(fs::read_to_string(&target).unwrap(), "original");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn commit_is_complete_and_an_existing_backup_is_never_overwritten() {
        let dir = directory();
        let target = dir.join("novel.txt");
        atomic_write(&target, "original").unwrap();
        atomic_write(&target, "complete\n한국어").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "complete\n한국어");
        let backup = target.with_extension("backup");
        fs::write(&backup, "user backup").unwrap();
        assert!(atomic_write(&target, "rejected").is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "complete\n한국어");
        assert_eq!(fs::read_to_string(backup).unwrap(), "user backup");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn overlapping_guards_do_not_share_temporary_files() {
        let dir = directory();
        let target = dir.join("novel.txt");
        let a = AtomicFile::new(&target).unwrap();
        let mut b = AtomicFile::new(&target).unwrap();
        assert_ne!(a.temporary, b.temporary);
        drop(a);
        b.write_all(b"retained").unwrap();
        b.commit().unwrap();
        assert_eq!(fs::read_to_string(target).unwrap(), "retained");
        fs::remove_dir_all(dir).unwrap();
    }
}
