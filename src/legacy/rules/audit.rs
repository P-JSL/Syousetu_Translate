use super::{complement, parse, Element, Operator, Predicate, Value};
use crate::legacy::legacy_dat::{CRuleKj, RuleKj};
use anyhow::{ensure, Context, Result};
use encoding_rs::{EUC_KR, SHIFT_JIS};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs::File, io::Read, path::Path};

const MAX_FILE: u64 = 64 * 1024 * 1024;
const MAX_RECORD: usize = 4096;
#[derive(Default, Debug, Serialize)]
pub struct EncodingCounts {
    pub literals: usize,
    pub ascii: usize,
    pub valid_cp949: usize,
    pub valid_shift_jis: usize,
    pub valid_utf8: usize,
}
#[derive(Debug, Serialize)]
pub struct Failure {
    pub record: usize,
    pub byte_offset: usize,
    pub reason: &'static str,
}
#[derive(Default, Debug, Serialize)]
pub struct TableReport {
    pub filename: &'static str,
    pub bytes: usize,
    pub sha256: String,
    pub records: usize,
    pub parsed_records: usize,
    pub empty_records: usize,
    pub failed_records: usize,
    pub predicates: usize,
    pub fields: BTreeMap<String, usize>,
    pub field_references: BTreeMap<String, usize>,
    pub operators: BTreeMap<&'static str, usize>,
    pub separator_runs: BTreeMap<usize, usize>,
    pub literal_encodings_by_field: BTreeMap<String, EncodingCounts>,
    pub failure_examples: Vec<Failure>,
}
impl TableReport {
    fn literal(&mut self, field: u8, value: &[u8]) {
        let counts = self
            .literal_encodings_by_field
            .entry(char::from(field).to_string())
            .or_default();
        counts.literals += 1;
        counts.ascii += usize::from(value.is_ascii());
        counts.valid_cp949 += usize::from(
            EUC_KR
                .decode_without_bom_handling_and_without_replacement(value)
                .is_some(),
        );
        counts.valid_shift_jis += usize::from(
            SHIFT_JIS
                .decode_without_bom_handling_and_without_replacement(value)
                .is_some(),
        );
        counts.valid_utf8 += usize::from(std::str::from_utf8(value).is_ok());
    }
    fn predicate(&mut self, predicate: &Predicate<'_>) {
        self.predicates += 1;
        *self
            .fields
            .entry(format!(
                "{}{}",
                char::from(predicate.field.tag),
                predicate.field.offset.get()
            ))
            .or_default() += 1;
        let operator = match predicate.operator {
            Operator::Assign => "=",
            Operator::Equal => "==",
            Operator::NotEqual => "!=",
        };
        *self.operators.entry(operator).or_default() += 1;
        match &predicate.value {
            Value::Field(field) => {
                *self
                    .field_references
                    .entry(format!("{}{}", char::from(field.tag), field.offset.get()))
                    .or_default() += 1;
            }
            Value::Literal(bytes) => self.literal(predicate.field.tag, bytes),
            Value::Set { first, rest } => {
                self.literal(predicate.field.tag, first);
                for (_, bytes) in rest {
                    self.literal(predicate.field.tag, bytes);
                }
            }
        }
    }
    fn record(&mut self, index: usize, encoded: &[u8]) {
        if encoded.is_empty() {
            self.empty_records += 1;
            return;
        }
        let failure = if encoded.len() > MAX_RECORD {
            Some((0, "record exceeds inspection bound"))
        } else {
            let decoded = complement(encoded);
            match parse(&decoded) {
                Ok(rule) => {
                    self.parsed_records += 1;
                    for element in &rule.elements {
                        match element {
                            Element::Predicate(predicate) => self.predicate(predicate),
                            Element::Separator(count) => {
                                *self.separator_runs.entry(*count).or_default() += 1;
                            }
                        }
                    }
                    None
                }
                Err(error) => Some((error.offset, error.reason)),
            }
        };
        if let Some((byte_offset, reason)) = failure {
            self.failed_records += 1;
            if self.failure_examples.len() < 8 {
                self.failure_examples.push(Failure {
                    record: index,
                    byte_offset,
                    reason,
                });
            }
        }
    }
}
#[derive(Debug, Serialize)]
pub struct Inspection {
    pub transform: &'static str,
    pub container_layout_verified: bool,
    pub all_nonempty_syntax_parsed: bool,
    pub original_runtime_equivalence_verified: bool,
    pub translation_semantics_verified: bool,
    pub eligible_for_default_ja_ko: bool,
    pub tables: Vec<TableReport>,
    pub limits: Vec<&'static str>,
}

fn read(path: &Path) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)
        .with_context(|| format!("open {}", path.display()))?
        .take(MAX_FILE + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_FILE,
        "rule table exceeds 64 MiB inspection limit"
    );
    Ok(bytes)
}
pub fn inspect(directory: &Path) -> Result<Inspection> {
    let data_dir = if directory.join("Dat").is_dir() {
        directory.join("Dat")
    } else {
        directory.to_path_buf()
    };
    let mut tables = Vec::new();
    for filename in ["RuleKJ.Dat", "CRuleKJ.Dat"] {
        let bytes = read(&data_dir.join(filename))?;
        let mut report = TableReport {
            filename,
            bytes: bytes.len(),
            sha256: format!("{:x}", Sha256::digest(&bytes)),
            ..Default::default()
        };
        if filename == "RuleKJ.Dat" {
            let table = RuleKj::parse(&bytes)?;
            report.records = table.count();
            for index in 0..table.count() {
                let page = table.page(index).context("missing fixed rule page")?;
                // Framing retains the original page. Only validated *trailing*
                // raw zero padding is excluded from this separate syntax view.
                let length = page
                    .iter()
                    .rposition(|byte| *byte != 0)
                    .map_or(0, |at| at + 1);
                report.record(index, &page[..length]);
            }
        } else {
            let table = CRuleKj::parse(&bytes)?;
            report.records = table.count();
            for index in 0..table.count() {
                report.record(
                    index,
                    table.record(index).context("missing compact rule record")?,
                );
            }
        }
        tables.push(report);
    }
    Ok(Inspection {
        transform: "candidate bytewise complement (xor 0xff)",
        container_layout_verified: true,
        all_nonempty_syntax_parsed: tables.iter().all(|table| table.failed_records == 0),
        original_runtime_equivalence_verified: false,
        translation_semantics_verified: false,
        eligible_for_default_ja_ko: false,
        tables,
        limits: vec![
            "Syntax does not establish the meaning of field tags, operators, separator runs or rule priority",
            "Literal bytes retain mixed encodings; independent decoder validity is not proof of a field encoding",
            "KJ source/destination examples suggest Korean-to-Japanese; do not invert these rules into Japanese-to-Korean",
            "Different record counts do not establish a positional mapping between the two tables",
            "No proprietary literal payload is included in this report or applied to translation memory",
        ],
    })
}
