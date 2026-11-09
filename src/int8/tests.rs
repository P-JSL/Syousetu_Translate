use super::*;
#[test]
fn pinned_int8_has_complete_integrity_metadata() {
    let spec = spec();
    assert_eq!(spec.format_version, 1);
    assert_eq!(spec.file, "model-int8.gguf");
    assert!(spec.size > 300_000_000 && spec.size < 400_000_000);
    assert_eq!(spec.sha256.len(), 64);
    assert!(spec.sha256.chars().all(|c| c.is_ascii_hexdigit()));
    assert!(!spec.file.contains(['/', '\\']));
}
