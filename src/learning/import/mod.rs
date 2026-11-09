//! Import only independently meaningful dictionary pairs. Engine directives and
//! synthetic morphological tokens cannot be interpreted by neural inference.
pub mod dictionary;
mod ehnd;
mod review;
pub use ehnd::{import_ehnd, ImportReport};
pub fn import_dictionary(
    path: &std::path::Path,
    memory: &mut super::Memory,
) -> anyhow::Result<ImportReport> {
    anyhow::ensure!(
        std::fs::metadata(path)?.len() <= 8 * 1024 * 1024,
        "가져올 사용자 사전은 8MB 이하여야 합니다"
    );
    let digest = crate::storage::sha256_file(path)?;
    let review = review::find(&digest);
    let entries = dictionary::read(path)?;
    let mut report = ImportReport {
        dictionary_files: 1,
        dictionary_records: entries.len(),
        ..ImportReport::default()
    };
    let mut candidates = std::collections::BTreeMap::<String, Option<String>>::new();
    for (source, target) in entries {
        if review.is_some_and(|policy| !policy.allows(&source, &target)) {
            report.skipped_reviewed_invalid += 1;
            continue;
        }
        if target.contains(['#', '∬', '♞']) || !target.chars().any(|c| ('가'..='힣').contains(&c))
        {
            report.skipped_synthetic_or_ambiguous += 1;
            continue;
        }
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
    for (source, target) in candidates {
        let Some(target) = target else {
            continue;
        };
        report.accepted_literals += 1;
        if memory.sentences.contains_key(&source) {
            report.retained_existing += 1;
        } else if memory.sentences.len() + memory.terms.len() >= super::MAX_ENTRIES {
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
    fn conflicting_labels_are_excluded_without_overwriting_user_data() {
        let p = std::env::temp_dir().join(format!(
            "syousetu-import-conflict-{}.tsv",
            std::process::id()
        ));
        std::fs::write(&p, "ありがとう\t고마워\nありがとう\t잘 가\n日本\t일본\n").unwrap();
        let mut memory = super::super::Memory::default();
        let report = import_dictionary(&p, &mut memory).unwrap();
        assert_eq!(report.dictionary_records, 3);
        assert_eq!(report.accepted_literals, 1);
        assert_eq!(report.conflicting_sources, 1);
        assert_eq!(memory.lookup("日本"), Some("일본"));
        assert!(memory.lookup("ありがとう").is_none());
        memory.learn("日本", "사용자 표현").unwrap();
        import_dictionary(&p, &mut memory).unwrap();
        assert_eq!(memory.lookup("日本"), Some("사용자 표현"));
        std::fs::remove_file(p).unwrap();
    }
}
