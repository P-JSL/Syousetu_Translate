//! Shared atomic output for translated text and edited settings.
use anyhow::{Context, Result};
use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    path::Path,
};

pub fn atomic_write(path: &Path, body: &str) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let name = path
        .file_name()
        .context("출력 파일명이 없습니다")?
        .to_string_lossy();
    let tmp = parent.join(format!(".{name}.{}.tmp", std::process::id()));
    let result = (|| -> Result<()> {
        let mut writer = BufWriter::new(File::create(&tmp)?);
        writer.write_all(body.as_bytes())?;
        writer.flush()?;
        writer.get_ref().sync_all()?;
        if path.exists() {
            let backup = path.with_extension("backup");
            if backup.exists() {
                anyhow::bail!("백업 파일이 이미 존재합니다: {}", backup.display());
            }
            fs::rename(path, &backup).context("백업 생성 실패")?;
            match fs::rename(&tmp, path) {
                Ok(()) => {
                    fs::remove_file(&backup).ok();
                    Ok(())
                }
                Err(e) => {
                    let _ = fs::rename(&backup, path);
                    Err(e.into())
                }
            }
        } else {
            fs::rename(&tmp, path)?;
            Ok(())
        }
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}
