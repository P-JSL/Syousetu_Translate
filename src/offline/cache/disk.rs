use super::{cost, valid, MAX_BYTES, MAX_ENTRIES};
use anyhow::{ensure, Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_COMPRESSED: u64 = 32 * 1024 * 1024;
const MAX_JSON: usize = 64 * 1024 * 1024;
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Entry {
    pub source: String,
    pub target: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    version: u32,
    identity: String,
    entries: Vec<Entry>,
}
pub(super) struct Store {
    path: PathBuf,
    identity: String,
}
impl Store {
    pub(super) fn new(directory: &Path, identity: String) -> Result<Self> {
        ensure!(
            identity.len() == 64 && identity.bytes().all(|b| b.is_ascii_hexdigit()),
            "잘못된 번역 결과 식별자"
        );
        fs::create_dir_all(directory)?;
        Ok(Self {
            path: directory.join(format!("{identity}.json.zlib")),
            identity,
        })
    }
    pub(super) fn read(&self) -> Result<Vec<Entry>> {
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                match File::open(self.path.with_extension("backup")) {
                    Ok(file) => file,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
                    Err(e) => return Err(e.into()),
                }
            }
            Err(e) => return Err(e.into()),
        };
        ensure!(
            file.metadata()?.len() <= MAX_COMPRESSED,
            "번역 결과 파일 한도 초과"
        );
        let mut packed = Vec::new();
        file.take(MAX_COMPRESSED + 1).read_to_end(&mut packed)?;
        ensure!(
            packed.len() as u64 <= MAX_COMPRESSED,
            "번역 결과 파일 한도 초과"
        );
        let json = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&packed, MAX_JSON)
            .map_err(|e| anyhow::anyhow!("번역 결과 압축 데이터 오류: {e:?}"))?;
        let snapshot: Snapshot = serde_json::from_slice(&json)?;
        ensure!(
            snapshot.version == 1
                && snapshot.identity == self.identity
                && snapshot.entries.len() <= MAX_ENTRIES,
            "번역 결과 출처 또는 형식 불일치"
        );
        let mut keys = HashSet::new();
        let mut bytes = 0usize;
        for entry in &snapshot.entries {
            ensure!(
                valid(&entry.source, &entry.target) && keys.insert(&entry.source),
                "번역 결과 항목 오류"
            );
            bytes += cost(&entry.source, &entry.target);
            ensure!(bytes <= MAX_BYTES, "번역 결과 용량 한도 초과");
        }
        Ok(snapshot.entries)
    }
    pub(super) fn merge(&self, delta: Vec<Entry>) -> Result<()> {
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.path.with_extension("lock"))?;
        lock.try_lock_exclusive()
            .context("다른 작업이 번역 결과를 저장 중입니다")?;
        ensure!(
            !self.path.with_extension("backup").exists(),
            "이전 번역 결과 백업을 보존합니다"
        );
        let mut entries = HashMap::new();
        let mut order = VecDeque::new();
        let mut bytes = 0;
        for entry in self.read()?.into_iter().chain(delta) {
            ensure!(valid(&entry.source, &entry.target), "번역 결과 항목 오류");
            if let Some(old) = entries.insert(entry.source.clone(), entry.target.clone()) {
                bytes -= cost(&entry.source, &old);
            } else {
                order.push_back(entry.source.clone());
            }
            bytes += cost(&entry.source, &entry.target);
            while entries.len() > MAX_ENTRIES || bytes > MAX_BYTES {
                let old = order.pop_front().context("번역 결과 순서 오류")?;
                if let Some(target) = entries.remove(&old) {
                    bytes -= cost(&old, &target);
                }
            }
        }
        let entries = order
            .into_iter()
            .filter_map(|source| {
                entries
                    .remove(&source)
                    .map(|target| Entry { source, target })
            })
            .collect();
        let json = serde_json::to_vec(&Snapshot {
            version: 1,
            identity: self.identity.clone(),
            entries,
        })?;
        ensure!(json.len() <= MAX_JSON, "번역 결과 용량 한도 초과");
        let packed = miniz_oxide::deflate::compress_to_vec_zlib(&json, 1);
        ensure!(
            packed.len() as u64 <= MAX_COMPRESSED,
            "번역 결과 용량 한도 초과"
        );
        let mut output = crate::storage::AtomicFile::new(&self.path)?;
        output.write_all(&packed)?;
        output.commit()
    }
}
