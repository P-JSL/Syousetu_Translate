use super::*;
use encoding_rs::{EUC_KR, SHIFT_JIS};
use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
};

#[test]
fn mixed_encodings_and_shift_jis_backslash_are_kept_as_literal_bytes() {
    let (source, _, source_errors) = EUC_KR.encode("학생");
    let (target, _, target_errors) = SHIFT_JIS.encode("表");
    assert!(!source_errors && !target_errors);
    assert!(target.contains(&b'\\'));
    let mut bytes = b"S-2==\"".to_vec();
    bytes.extend_from_slice(&source);
    bytes.extend_from_slice(b"\";D0=\"");
    bytes.extend_from_slice(&target);
    bytes.extend_from_slice(b"\";;");
    let encoded = complement(&bytes);
    assert_eq!(complement(&encoded), bytes);
    let rule = parse(&bytes).unwrap();
    assert_eq!(rule.raw, bytes);
    let Element::Predicate(first) = &rule.elements[0] else {
        panic!("predicate")
    };
    assert_eq!(first.field.tag, b'S');
    assert_eq!(first.field.offset.get(), -2);
    assert_eq!(first.operator, Operator::Equal);
    assert_eq!(first.value, Value::Literal(&source));
    let Element::Predicate(last) = &rule.elements[2] else {
        panic!("predicate")
    };
    assert_eq!(last.value, Value::Literal(&target));
    assert_eq!(last.operator, Operator::Assign);
    assert_eq!(rule.elements[3], Element::Separator(2));
    // A borrowed literal must reference the original byte buffer, rather than
    // lossy text decoded under a guessed common encoding.
    let Value::Literal(literal) = &first.value else {
        panic!("literal")
    };
    assert_eq!(literal.as_ptr(), bytes[6..].as_ptr());
}

#[test]
fn set_operators_and_quoted_syntax_do_not_change_clause_boundaries() {
    let input = br#" S1!={";|&{}"|"second"&"third"};;Z-32768=""; "#;
    let rule = parse(input).unwrap();
    assert_eq!(rule.raw, input);
    assert_eq!(rule.elements.len(), 4);
    let Element::Predicate(predicate) = &rule.elements[0] else {
        panic!("predicate")
    };
    assert_eq!(predicate.operator, Operator::NotEqual);
    assert_eq!(
        predicate.value,
        Value::Set {
            first: b";|&{}",
            rest: vec![(Join::Bar, b"second"), (Join::Ampersand, b"third")],
        }
    );
    let Element::Predicate(unknown) = &rule.elements[2] else {
        panic!("predicate")
    };
    assert_eq!(unknown.field.tag, b'Z');
    assert_eq!(unknown.field.offset.get(), i16::MIN);
}

#[test]
fn field_reference_values_remain_typed_without_becoming_string_literals() {
    let rule = parse(br#"Z2!=A-3;D0="authored";"#).unwrap();
    let Element::Predicate(first) = &rule.elements[0] else {
        panic!("predicate")
    };
    let Value::Field(reference) = first.value else {
        panic!("field reference")
    };
    assert_eq!(reference.tag, b'A');
    assert_eq!(reference.offset.get(), -3);
    assert_eq!(first.operator, Operator::NotEqual);
    for invalid in [b"Z2==A".as_slice(), b"Z2==A+3", b"Z2==A-3junk"] {
        assert!(parse(invalid).is_err());
    }
}

#[test]
fn malformed_rules_fail_without_accepting_a_partial_prefix() {
    for input in [
        &b""[..],
        b";;",
        b"S==\"x\"",
        b"S-==\"x\"",
        b"s0==\"x\"",
        b"S32768==\"x\"",
        b"S-32769==\"x\"",
        b"S0=\"unterminated",
        b"S0=={}",
        b"S0=={\"x\"|}",
        b"S0=={\"x\"",
        b"S0==\"x\"D0=\"y\"",
        b"S0===\"x\"",
        b"S0==\"x\";\xff",
        b"S0!\"x\"",
    ] {
        let error = parse(input).unwrap_err();
        assert!(error.offset <= input.len());
    }
}

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(std::path::PathBuf);
impl Directory {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "syousetu-rule-audit-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(directory.join("Dat")).unwrap();
        Self(directory)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn audit_keeps_original_files_and_reports_invalid_or_internal_padding() {
    let directory = Directory::new();
    let valid = complement(br#"S0=="authored-private-literal";D0="target";"#);
    let mut fixed = 2u32.to_le_bytes().to_vec();
    fixed.extend_from_slice(&valid);
    fixed.resize(4 + 1024, 0);
    let mut invalid = complement(br#"S0=="x";"#);
    invalid[0] = 0; // An internal raw zero is not discarded as page padding.
    fixed.extend_from_slice(&invalid);
    fixed.resize(4 + 2048, 0);
    let fixed_path = directory.0.join("Dat/RuleKJ.Dat");
    fs::write(&fixed_path, &fixed).unwrap();
    let mut compact = 2u32.to_le_bytes().to_vec();
    compact.extend_from_slice(&(valid.len() as u32).to_le_bytes());
    compact.extend_from_slice(&valid);
    let invalid = complement(br#"S0=={"x"|}"#);
    compact.extend_from_slice(&(invalid.len() as u32).to_le_bytes());
    compact.extend_from_slice(&invalid);
    let compact_path = directory.0.join("Dat/CRuleKJ.Dat");
    fs::write(&compact_path, &compact).unwrap();
    let report = inspect(&directory.0).unwrap();
    assert!(!report.all_nonempty_syntax_parsed);
    assert!(!report.translation_semantics_verified && !report.eligible_for_default_ja_ko);
    for table in &report.tables {
        assert_eq!(
            (table.records, table.parsed_records, table.failed_records),
            (2, 1, 1)
        );
        assert_eq!(table.failure_examples.len(), 1);
        assert_eq!(table.failure_examples[0].record, 1);
    }
    let serialized = serde_json::to_string(&report).unwrap();
    assert!(!serialized.contains("authored-private-literal"));
    assert_eq!(fs::read(fixed_path).unwrap(), fixed);
    assert_eq!(fs::read(compact_path).unwrap(), compact);
}

#[test]
fn malformed_container_is_not_reported_as_successful_rule_recovery() {
    let directory = Directory::new();
    fs::write(directory.0.join("Dat/RuleKJ.Dat"), 2u32.to_le_bytes()).unwrap();
    assert!(inspect(&directory.0).is_err());
}

#[test]
#[ignore = "requires private original tables in SYOUSETU_LEGACY_DAT_DIR"]
fn original_tables_if_available() {
    let directory = std::env::var_os("SYOUSETU_LEGACY_DAT_DIR").expect("set private Dat path");
    let report = inspect(std::path::Path::new(&directory)).unwrap();
    assert_eq!(
        report
            .tables
            .iter()
            .map(|table| table.records)
            .collect::<Vec<_>>(),
        [6086, 8860]
    );
    assert!(report.all_nonempty_syntax_parsed, "{report:?}");
}
