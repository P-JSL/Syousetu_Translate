use super::super::{Memory, MAX_ENTRIES};
use anyhow::{Context, Result};
use serde::Serialize;
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Debug, Default, Serialize)]
pub struct ImportReport {
    pub skipped_reviewed_invalid: usize,
    pub dictionary_files: usize,
    pub dictionary_records: usize,
    pub accepted_literals: usize,
    pub skipped_synthetic_or_ambiguous: usize,
    pub skipped_engine_filter_records: usize,
    pub conflicting_sources: usize,
    pub inserted: usize,
    pub retained_existing: usize,
    pub skipped_capacity: usize,
}
fn decode(bytes: Vec<u8>) -> Result<String> {
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        let encoding = if bytes[0] == 0xff {
            encoding_rs::UTF_16LE
        } else {
            encoding_rs::UTF_16BE
        };
        let (text, _, bad) = encoding.decode(&bytes[2..]);
        anyhow::ensure!(!bad, "사전 UTF-16 디코딩 실패");
        Ok(text.into_owned())
    } else {
        Ok(String::from_utf8(bytes).context("사전은 UTF-8 또는 UTF-16 BOM이어야 합니다")?)
    }
}
fn pair(line: &str) -> Option<(String, String)> {
    let fields = line.split('\t').collect::<Vec<_>>();
    if fields.len() < 3 || !matches!(fields[2].trim(), "0" | "1") {
        return None;
    }
    let source = fields[0].trim();
    let mut target = fields[1].trim();
    if fields.get(3).is_some_and(|v| v.trim() == "VHn") {
        target = target.strip_suffix("#N").unwrap_or(target);
    }
    if !(2..=128).contains(&source.chars().count()) || target.is_empty() || target.len() > 768 {
        return None;
    }
    if source.contains([
        '#', '\\', '[', ']', '(', ')', '{', '}', '^', '$', '*', '+', '?', '|', '_', '乘', '梦',
        'ゎ', '♞', '∬',
    ]) || target.contains([
        '#', '\\', '[', ']', '{', '}', '^', '$', '*', '|', '∬', '♞', 'Ⓐ', 'Ⓢ',
    ]) {
        return None;
    }
    let japanese = |s: &str| {
        s.chars()
            .any(|c| matches!(c,'\u{3040}'..='\u{30ff}'|'\u{3400}'..='\u{9fff}'))
    };
    if !japanese(source) || japanese(target) || !target.chars().any(|c| ('가'..='힣').contains(&c))
    {
        return None;
    }
    Some((source.to_owned(), target.to_owned()))
}
pub fn import_ehnd(directory: &Path, memory: &mut Memory) -> Result<ImportReport> {
    let root = if directory.join("Ehnd").is_dir() {
        directory.join("Ehnd")
    } else {
        directory.to_owned()
    };
    let mut paths = fs::read_dir(&root)
        .with_context(|| format!("EHND 사전 폴더 읽기 실패: {}", root.display()))?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort();
    let mut report = ImportReport::default();
    let mut candidates: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut bytes = 0u64;
    for path in paths {
        let meta = fs::symlink_metadata(&path)?;
        if !meta.is_file() {
            continue;
        }
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if !name.ends_with(".txt")
            || !(["UserDict_", "PreFilter_", "PostFilter_", "SkipLayer_"]
                .iter()
                .any(|prefix| name.starts_with(prefix)))
        {
            continue;
        }
        bytes += meta.len();
        anyhow::ensure!(
            meta.len() <= 8 * 1024 * 1024 && bytes <= 32 * 1024 * 1024,
            "EHND 텍스트 사전 크기 한도 초과"
        );
        let text = decode(fs::read(&path)?)?;
        let records = text
            .lines()
            .map(|l| l.trim().trim_start_matches('\u{feff}'))
            .filter(|l| !l.is_empty() && !l.starts_with("//"));
        if !name.starts_with("UserDict_") {
            report.skipped_engine_filter_records += records.count();
            continue;
        }
        report.dictionary_files += 1;
        for line in records {
            report.dictionary_records += 1;
            if name.contains("#0#") {
                report.skipped_synthetic_or_ambiguous += 1;
                continue;
            }
            let Some((source, target)) = pair(line) else {
                report.skipped_synthetic_or_ambiguous += 1;
                continue;
            };
            match candidates.entry(source) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(Some(target));
                }
                std::collections::btree_map::Entry::Occupied(mut entry) => {
                    if entry.get().as_ref().is_some_and(|old| old != &target) {
                        entry.insert(None);
                        report.conflicting_sources += 1;
                    }
                }
            }
        }
    }
    for (source, target) in candidates {
        let Some(target) = target else {
            continue;
        };
        report.accepted_literals += 1;
        if memory.sentences.contains_key(&source) {
            report.retained_existing += 1;
        } else if memory.sentences.len() + memory.terms.len() >= MAX_ENTRIES {
            report.skipped_capacity += 1;
        } else {
            memory.learn(&source, &target)?;
            report.inserted += 1;
        }
    }
    Ok(report)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_real_literal_pairs_and_known_name_suffixes_are_accepted() {
        assert_eq!(
            pair("クロムウェル\t크롬웰#N\t1\tVHn"),
            Some(("クロムウェル".into(), "크롬웰".into()))
        );
        assert!(pair("乘名\t名:\t1").is_none());
        assert!(pair("おいしゎ形\t맛있어\t0").is_none());
        assert!(pair("猫.*\t고양이\t1").is_none());
        assert!(pair("_SkipLayer\t꺼짐\t1").is_none());
        assert!(pair("猫\t고양이#M\t1").is_none());
    }
}
