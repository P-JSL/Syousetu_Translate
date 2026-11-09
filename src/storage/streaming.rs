//! Translation output is visible at its final path while the job is running.
use anyhow::{Context, Result};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufWriter, Seek, Write},
    path::{Path, PathBuf},
};

pub struct StreamingFile {
    writer: BufWriter<File>,
    backup: Option<PathBuf>,
}
impl StreamingFile {
    pub fn create(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        let (file, backup) = if path.exists() {
            anyhow::ensure!(
                fs::symlink_metadata(path)?.file_type().is_file(),
                "출력 경로는 일반 파일이어야 합니다: {}",
                path.display()
            );
            let backup = path.with_extension("backup");
            let mut file = OpenOptions::new().read(true).write(true).open(path)?;
            let mut saved = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&backup)
                .with_context(|| format!("기존 백업을 먼저 확인해 주세요: {}", backup.display()))?;
            let snapshot = io::copy(&mut file, &mut saved).and_then(|_| saved.sync_all());
            if let Err(error) = snapshot {
                drop(saved);
                let _ = fs::remove_file(&backup);
                return Err(error).context("기존 출력 파일 백업 실패");
            }
            drop(saved);
            file.set_len(0)?;
            file.rewind()?;
            (file, Some(backup))
        } else {
            (
                OpenOptions::new().write(true).create_new(true).open(path)?,
                None,
            )
        };
        Ok(Self {
            writer: BufWriter::with_capacity(64 * 1024, file),
            backup,
        })
    }
    pub fn finish(mut self) -> Result<()> {
        self.writer.flush()?;
        self.writer.get_ref().sync_all()?;
        if let Some(backup) = self.backup.take() {
            fs::remove_file(backup)?;
        }
        Ok(())
    }
}
impl Write for StreamingFile {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.writer.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}
impl Drop for StreamingFile {
    fn drop(&mut self) {
        // Cancellation preserves both the visible translated prefix and any
        // original-file backup. No temporary translation file is created.
        let _ = self.writer.flush();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    fn path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "syousetu-live-{}-{}.txt",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }
    #[test]
    fn flushed_prefix_is_visible_before_finish_and_survives_cancellation() {
        let path = path();
        fs::write(&path, "original").unwrap();
        let mut output = StreamingFile::create(&path).unwrap();
        output.write_all("첫 번역\n".as_bytes()).unwrap();
        output.flush().unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "첫 번역\n");
        assert_eq!(
            fs::read_to_string(path.with_extension("backup")).unwrap(),
            "original"
        );
        drop(output);
        assert_eq!(fs::read_to_string(&path).unwrap(), "첫 번역\n");
        assert!(StreamingFile::create(&path).is_err());
        fs::remove_file(path.with_extension("backup")).unwrap();
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn successful_output_closes_the_backup_transaction() {
        let path = path();
        fs::write(&path, "old").unwrap();
        let mut output = StreamingFile::create(&path).unwrap();
        output.write_all(b"complete").unwrap();
        output.finish().unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "complete");
        assert!(!path.with_extension("backup").exists());
        fs::remove_file(path).unwrap();
    }
}
