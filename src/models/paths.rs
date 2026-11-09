use super::status;
use std::path::{Path, PathBuf};
pub fn data_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("LOCALAPPDATA").filter(|p| !p.is_empty()) {
        return PathBuf::from(p).join("SyousetuTranslate");
    }
    if let Some(p) = std::env::var_os("XDG_DATA_HOME").filter(|p| !p.is_empty()) {
        return PathBuf::from(p).join("SyousetuTranslate");
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".local/share/SyousetuTranslate")
}
pub fn default_dir() -> PathBuf {
    std::env::var_os("SYOUSETU_MODEL_DIR")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|exe| bundled_dir(&exe))
        })
        .unwrap_or_else(|| data_dir().join("models"))
}
pub(super) fn bundled_dir(executable: &Path) -> Option<PathBuf> {
    let candidate = executable.parent()?.join("models");
    status(&candidate).ready.then_some(candidate)
}
