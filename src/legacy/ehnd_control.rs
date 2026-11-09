//! EHND control-flow audit. Extracts directive structure without
//! inventing meanings for the proprietary conditional state machine.
use crate::ehnd::{Ehnd, Phase, RuleType};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Serialize)]
pub struct DirectiveRecord {
    pub file: String,
    pub line: usize,
    pub phase: &'static str,
    pub directive: String,
    pub family: Option<String>,
    pub key: Option<String>,
    pub pattern: Option<String>,
    pub replacement: Option<String>,
    pub priority: Option<u64>,
    pub is_regex: Option<bool>,
}
#[derive(Debug, Serialize)]
pub struct ControlAudit {
    pub total_directives: usize,
    pub by_type: BTreeMap<String, usize>,
    pub tgl_by_family: BTreeMap<String, usize>,
    pub control_ids: usize,
    pub ids_linked_on_and_conn: usize,
    pub ids_conn_without_on: usize,
    pub malformed_or_unknown: usize,
    pub examples: Vec<DirectiveRecord>,
    pub all_directives_executable: bool,
}
pub fn audit(ehnd: &Ehnd) -> ControlAudit {
    let mut by_type = BTreeMap::new();
    let mut tgl_by_family = BTreeMap::new();
    let mut ids: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut examples = Vec::new();
    let mut malformed = 0usize;
    let mut total = 0usize;
    for rule in &ehnd.rules {
        if rule.kind != RuleType::Directive {
            continue;
        }
        total += 1;
        let kind = rule.fields.first().map(String::as_str).unwrap_or("");
        *by_type.entry(kind.to_owned()).or_insert(0) += 1;
        let mut record = DirectiveRecord {
            file: rule.origin.clone(),
            line: rule.line,
            phase: if rule.phase == Phase::Pre {
                "PRE"
            } else {
                "POST"
            },
            directive: kind.to_owned(),
            family: None,
            key: None,
            pattern: None,
            replacement: None,
            priority: None,
            is_regex: None,
        };
        match kind {
            "_TGL" => {
                let identifier = rule.fields.get(1).map(String::as_str).unwrap_or("");
                if let Some((family, key)) = identifier.split_once(':') {
                    record.family = Some(family.to_owned());
                    record.key = Some(key.to_owned());
                    *tgl_by_family.entry(family.to_owned()).or_insert(0) += 1;
                    ids.entry(key.to_owned())
                        .or_default()
                        .insert(family.to_owned());
                } else {
                    malformed += 1;
                }
                if rule.fields.len() >= 6 {
                    record.pattern = rule.fields.get(2).cloned();
                    record.replacement = rule.fields.get(3).cloned();
                    record.priority = rule.fields[4].parse().ok();
                    record.is_regex = match rule.fields[5].as_str() {
                        "1" => Some(true),
                        "0" => Some(false),
                        _ => None,
                    };
                }
            }
            "_skip_sub" | "_SkipLayer" if rule.fields.len() >= 5 => {
                record.pattern = rule.fields.get(1).cloned();
                record.replacement = rule.fields.get(2).cloned();
                record.priority = rule.fields[3].parse().ok();
                record.is_regex = match rule.fields[4].as_str() {
                    "1" => Some(true),
                    "0" => Some(false),
                    _ => None,
                };
                if record.priority.is_none() || record.is_regex.is_none() {
                    malformed += 1;
                }
            }
            _ => {
                malformed += 1;
            }
        }
        if examples.len() < 50 {
            examples.push(record);
        }
    }
    ControlAudit {
        total_directives: total,
        by_type,
        tgl_by_family,
        control_ids: ids.len(),
        ids_linked_on_and_conn: ids
            .values()
            .filter(|families| families.contains("_ON") && families.contains("_CONN"))
            .count(),
        ids_conn_without_on: ids
            .values()
            .filter(|families| families.contains("_CONN") && !families.contains("_ON"))
            .count(),
        malformed_or_unknown: malformed,
        examples,
        all_directives_executable: false,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn structural_control_links() {
        let dir = std::env::temp_dir().join(format!("ehnd-control-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        for (name,text) in [
            ("ehnd_conf.ini","PREFILTER_SWITCH=ON\n"),
            ("PreFilter_@Hdor.txt",
                "_TGL\t_ON:ABC\tpattern\tx\t120\t1\n_TGL\t_CONN:ABC\tpattern\tx\t120\t1\n_skip_sub\tbar\tx\t100\t0\n"),
            ("PreFilter_@Hdor_C.txt",""),
            ("PostFilter_@Hdor.txt","_SkipLayer\tfoo\ty\t200\t1\n"),
            ("PostFilter_@Hdor_C.txt",""),
        ]{fs::write(dir.join(name),text).unwrap();}
        let ehnd = Ehnd::load(&dir).unwrap();
        let report = audit(&ehnd);
        assert_eq!(report.total_directives, 4);
        assert_eq!(report.control_ids, 1);
        assert_eq!(report.ids_linked_on_and_conn, 1);
        assert_eq!(report.ids_conn_without_on, 0);
        assert_eq!(report.malformed_or_unknown, 0);
        for name in [
            "ehnd_conf.ini",
            "PreFilter_@Hdor.txt",
            "PreFilter_@Hdor_C.txt",
            "PostFilter_@Hdor.txt",
            "PostFilter_@Hdor_C.txt",
        ] {
            fs::remove_file(dir.join(name)).unwrap();
        }
        fs::remove_dir(dir).unwrap();
    }
}
