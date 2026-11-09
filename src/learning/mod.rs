//! Small, user-approved translation memory. This never trains model weights or
//! accepts generated translations as verified corrections.
mod bootstrap;
pub mod import;
use anyhow::{bail, Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
};

const MAX_BYTES: u64 = 5 * 1024 * 1024;
const MAX_ENTRIES: usize = 25_000;
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Term {
    pub source: String,
    pub previous: String,
    pub corrected: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Memory {
    version: u32,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    defaults_applied: bool,
    pub sentences: BTreeMap<String, String>,
    pub terms: Vec<Term>,
}
impl Default for Memory {
    fn default() -> Self {
        Self {
            version: 1,
            defaults_applied: false,
            sentences: BTreeMap::new(),
            terms: Vec::new(),
        }
    }
}
pub fn default_path() -> PathBuf {
    std::env::var_os("SYOUSETU_MEMORY_PATH")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let root = crate::models::data_dir();
            let compressed = root.join("corrections.json.zlib");
            let legacy = root.join("corrections.json");
            if !compressed.is_file() && legacy.is_file() {
                legacy
            } else {
                compressed
            }
        })
}
fn valid(text: &str) -> Result<()> {
    if text.trim().is_empty() || text.len() > 32_768 || text.contains('\0') {
        bail!("교정 문장은 비어 있지 않은 32KB 이내의 텍스트여야 합니다");
    }
    Ok(())
}
impl Memory {
    pub fn bundled() -> Result<Self> {
        bootstrap::bundled()
    }
    pub fn load(path: &Path) -> Result<Self> {
        let metadata = match fs::metadata(path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return bootstrap::bundled(),
            Err(e) => return Err(e).context("교정 기억 파일 확인 실패"),
        };
        if metadata.len() > MAX_BYTES {
            bail!("교정 기억 파일은 5MB 이하여야 합니다");
        }
        let bytes = fs::read(path)?;
        let bytes = if path.extension().is_some_and(|ext| ext == "zlib") {
            miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&bytes, MAX_BYTES as usize)
                .map_err(|e| anyhow::anyhow!("교정 기억 압축 해제 실패: {e:?}"))?
        } else {
            bytes
        };
        let memory: Self = serde_json::from_slice(&bytes)
            .with_context(|| format!("교정 기억 읽기 실패: {}", path.display()))?;
        if memory.version != 1 || memory.sentences.len() + memory.terms.len() > MAX_ENTRIES {
            bail!("지원하지 않는 교정 기억 버전 또는 항목 수 초과");
        }
        for (source, target) in &memory.sentences {
            valid(source)?;
            valid(target)?;
        }
        for term in &memory.terms {
            valid(&term.source)?;
            valid(&term.previous)?;
            valid(&term.corrected)?;
        }
        bootstrap::merge(memory)
    }
    pub fn lookup(&self, source: &str) -> Option<&str> {
        self.sentences.get(source.trim()).map(String::as_str)
    }
    /// Only terms present in the Japanese source qualify. Replacements run in
    /// one pass, preventing one correction from being fed into another rule.
    pub fn post_edit(&self, source: &str, translated: &str) -> Result<String> {
        let terms = self
            .terms
            .iter()
            .filter(|t| source.contains(&t.source) && translated.contains(&t.previous))
            .collect::<Vec<_>>();
        if terms.is_empty() {
            return Ok(translated.to_owned());
        }
        let matcher = aho_corasick::AhoCorasickBuilder::new()
            .match_kind(aho_corasick::MatchKind::LeftmostLongest)
            .build(terms.iter().map(|t| t.previous.as_str()))?;
        Ok(matcher.replace_all(
            translated,
            &terms
                .iter()
                .map(|t| t.corrected.as_str())
                .collect::<Vec<_>>(),
        ))
    }
    pub fn learn(&mut self, source: &str, corrected: &str) -> Result<()> {
        valid(source)?;
        valid(corrected)?;
        self.sentences
            .insert(source.trim().into(), corrected.trim().into());
        Ok(())
    }
    pub fn learn_term(&mut self, source: &str, previous: &str, corrected: &str) -> Result<()> {
        valid(source)?;
        valid(previous)?;
        valid(corrected)?;
        self.terms
            .retain(|t| t.source != source.trim() || t.previous != previous.trim());
        self.terms.push(Term {
            source: source.trim().into(),
            previous: previous.trim().into(),
            corrected: corrected.trim().into(),
        });
        Ok(())
    }
    /// Lock, re-read, merge and atomically save so concurrent GUI instances do
    /// not silently lose each other's corrections. A failed edit preserves data.
    pub fn edit(path: &Path, edit: impl FnOnce(&mut Self) -> Result<()>) -> Result<Self> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path.with_extension("lock"))?;
        lock.try_lock_exclusive()
            .context("다른 작업이 교정 기억을 저장 중입니다. 다시 시도해 주세요")?;
        let mut memory = Self::load(path)?;
        edit(&mut memory)?;
        let json = serde_json::to_string_pretty(&memory)?;
        if json.len() as u64 > MAX_BYTES
            || memory.sentences.len() + memory.terms.len() > MAX_ENTRIES
        {
            bail!("교정 기억 한도: 5MB / 25,000개");
        }
        if path.extension().is_some_and(|ext| ext == "zlib") {
            use std::io::Write;
            let bytes = miniz_oxide::deflate::compress_to_vec_zlib(json.as_bytes(), 9);
            let mut file = crate::storage::AtomicFile::new(path)?;
            file.write_all(&bytes)?;
            file.commit()?;
        } else {
            crate::storage::atomic_write(path, &json)?;
        }
        Ok(memory)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_corrections_and_source_gated_terms() {
        let mut m = Memory::default();
        m.learn(" 今日は晴れです。 ", "오늘은 맑습니다.").unwrap();
        assert_eq!(m.lookup("今日は晴れです。"), Some("오늘은 맑습니다."));
        assert_eq!(m.lookup("今日は雨です。"), None);
        m.learn_term("アリス", "앨리스", "아리스").unwrap();
        m.learn_term("アリス", "아리스", "SHOULD_NOT_CHAIN")
            .unwrap();
        assert_eq!(
            m.post_edit("アリスが来た", "앨리스가 왔다").unwrap(),
            "아리스가 왔다"
        );
        assert_eq!(
            m.post_edit("別人", "앨리스가 왔다").unwrap(),
            "앨리스가 왔다"
        );
    }
    #[test]
    fn edits_persist_merge_and_reject_corruption() {
        let dir = std::env::temp_dir().join(format!("syousetu-memory-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("memory.json");
        Memory::edit(&p, |m| m.learn("猫", "고양이")).unwrap();
        Memory::edit(&p, |m| m.learn("犬", "개")).unwrap();
        assert_eq!(
            Memory::load(&p).unwrap().sentences.len(),
            Memory::bundled().unwrap().sentences.len() + 2
        );
        assert!(Memory::edit(&p, |m| m.learn("", "bad")).is_err());
        assert_eq!(Memory::load(&p).unwrap().lookup("猫"), Some("고양이"));
        fs::write(&p, "{broken").unwrap();
        assert!(Memory::edit(&p, |m| m.learn("猫", "덮어쓰기")).is_err());
        assert_eq!(fs::read_to_string(&p).unwrap(), "{broken");
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn compressed_edits_preserve_overrides_and_deleted_defaults_after_reload() {
        let dir = std::env::temp_dir().join(format!("syousetu-memory-zlib-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("memory.json.zlib");
        Memory::edit(&path, |m| {
            m.learn("ありがとうございます。", "고맙습니다.")?;
            m.sentences.remove("気にしないでください。");
            Ok(())
        })
        .unwrap();
        let memory = Memory::load(&path).unwrap();
        assert_eq!(memory.lookup("ありがとうございます。"), Some("고맙습니다."));
        assert!(memory.lookup("気にしないでください。").is_none());
        assert!(
            fs::metadata(&path).unwrap().len() < serde_json::to_vec(&memory).unwrap().len() as u64
        );
        fs::write(&path, b"corrupt compressed bytes").unwrap();
        assert!(Memory::edit(&path, |_| Ok(())).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"corrupt compressed bytes");
        fs::remove_dir_all(dir).unwrap();
    }
}
