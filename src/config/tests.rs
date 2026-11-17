use super::*;
#[test]
fn gpu_settings_preserve_legacy_defaults_and_roundtrip_all_devices() {
    let old = Config::parse("translator_name=internal\n").unwrap();
    assert_eq!(old.inference_device, "auto");
    assert_eq!(old.cuda_device, 0);
    let old_json: Config = serde_json::from_str(r#"{"translator_name":"internal"}"#).unwrap();
    assert_eq!(old_json.inference_device, "auto");
    assert_eq!(old_json.cuda_device, 0);
    for device in ["auto", "cpu", "cuda"] {
        for ordinal in [0, 31] {
            let config = Config {
                inference_device: device.into(),
                cuda_device: ordinal,
                ..Config::default()
            };
            assert_eq!(
                Config::parse(&config.to_properties().unwrap()).unwrap(),
                config
            );
        }
    }
}
#[test]
fn invalid_gpu_settings_cannot_be_saved_or_loaded() {
    for text in [
        "inference_device=gpu",
        "inference_device=CUDA",
        "cuda_device=-1",
        "cuda_device=32",
        "cuda_device=not-a-number",
    ] {
        assert!(Config::parse(text).is_err(), "{text}");
    }
    let mut config = Config {
        cuda_device: 32,
        ..Config::default()
    };
    assert!(config.to_properties().is_err());
    config.cuda_device = 0;
    config.inference_device = "gpu".into();
    assert!(config.to_properties().is_err());
}
#[test]
fn download_folder_roundtrips_and_windows_default_matches_requested_path() {
    let c = Config {
        download_dir: Some(r"C:\Users\rabbit\OneDrive\문서".into()),
        ..Config::default()
    };
    assert_eq!(
        Config::parse(&c.to_properties().unwrap())
            .unwrap()
            .download_directory(),
        std::path::PathBuf::from(r"C:\Users\rabbit\OneDrive\문서")
    );
    #[cfg(windows)]
    assert_eq!(
        Config::default().download_directory(),
        std::path::PathBuf::from(r"C:\Users\rabbit\OneDrive\문서")
    );
}
#[test]
fn java_properties_escapes_continuations_and_last_value() {
    let cfg=Config::parse("!comment\nprint_japanese:false\nprint_japanese = true\ntitle_format = [currentPage]\\n\\\n  [koreanTitle]\\u0021\ntranslator_name=dummy\ncomment_separator=\\uD83D\\uDE00\npage_number_format=000\n").unwrap();
    assert!(cfg.print_japanese);
    assert_eq!(cfg.title_format, "[currentPage]\n[koreanTitle]!");
    assert_eq!(cfg.comment_separator, "😀");
    assert_eq!(cfg.translator_name, "passthrough");
    assert_eq!(format_number(12, &cfg.page_number_format).unwrap(), "012");
}
#[test]
fn legacy_empty_pattern_and_validation() {
    assert_eq!(
        Config::parse("page_number_format=\n")
            .unwrap()
            .page_number_format,
        "#"
    );
    for text in [
        "include_comment=yes",
        "max_retry_count=-1",
        "max_cpu_core_count=0",
        "page_number_format=0.0",
        "unknown_property=x",
        "title_format=\\uZZZZ",
    ] {
        assert!(Config::parse(text).is_err(), "{text}");
    }
    assert_eq!(format_number(12345, "#,##0").unwrap(), "12,345");
    assert_eq!(format_number(7, "0000").unwrap(), "0007");
}
