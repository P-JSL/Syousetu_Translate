#[cfg(feature = "offline-model")]
use {super::paths::bundled_dir, std::fs::File};

use super::*;
#[test]
fn direct_manifest_excludes_fp32_and_pins_the_existing_int8() {
    let assets = download_manifest();
    assert_eq!(assets.len(), 4);
    assert_eq!(assets.iter().map(|a| a.size).sum::<u64>(), 360_332_983);
    let mut paths = std::collections::HashSet::new();
    for a in &assets {
        assert!(paths.insert(format!("{}/{}", a.model, a.file)));
        assert!(!a.file.contains(['/', '\\']));
        assert!(!a.file.ends_with("safetensors"));
        assert!(a.url.starts_with("https://"));
        assert!(a.size > 0);
        assert_eq!(a.sha256.len(), 64);
        assert!(a.sha256.chars().all(|c| c.is_ascii_hexdigit()));
    }
    #[cfg(feature = "offline-model")]
    {
        let spec = crate::int8::spec();
        let a = assets.iter().find(|a| a.file == spec.file).unwrap();
        assert_eq!(
            (a.size, a.sha256.as_str()),
            (spec.size, spec.sha256.as_str())
        );
    }
    let fresh = status(Path::new("intentionally-absent-model-test"));
    assert!(!fresh.ready);
    assert_eq!(fresh.download_bytes, 360_332_983);
}
#[test]
fn provenance_is_pinned_but_not_a_runtime_download() {
    for a in manifest() {
        assert_eq!(a.revision.len(), 40);
        assert_eq!(a.sha256.len(), 64);
    }
}
#[cfg(feature = "offline-model")]
#[test]
fn portable_discovery_requires_the_complete_bundle_next_to_the_executable() {
    let root = std::env::temp_dir().join(format!("syousetu-portable-path-{}", std::process::id()));
    let exe = root.join("syousetu-translate.exe");
    assert!(bundled_dir(&exe).is_none());
    for asset in download_manifest() {
        let path = root.join("models").join(asset.model).join(asset.file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        File::create(path).unwrap().set_len(asset.size).unwrap();
    }
    assert_eq!(bundled_dir(&exe), Some(root.join("models")));
    fs::remove_file(root.join("models/ja-ko/vocab.json")).unwrap();
    assert!(bundled_dir(&exe).is_none());
    fs::remove_dir_all(root).unwrap();
}
