use std::process::Command;
use syousetu_translate::config::Config;

#[test]
fn older_profiles_and_explicit_reuse_settings_round_trip() {
    let old = Config::parse("learning_enabled=false\n").unwrap();
    assert!(old.reuse_translations);
    let changed = Config::parse("reuse_translations=false\nlearning_enabled=true\n").unwrap();
    assert!(!changed.reuse_translations && changed.learning_enabled);
    assert_eq!(
        Config::parse(&changed.to_properties().unwrap()).unwrap(),
        changed
    );
}

#[test]
fn explicit_disable_preserves_passthrough_and_conflicting_flags_are_rejected() {
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_syousetu-translate"))
            .args(args)
            .output()
            .unwrap()
    };
    let result = run(&[
        "translate",
        "--engine",
        "passthrough",
        "--text",
        "ASCII 123",
        "--jobs",
        "1",
        "--no-result-cache",
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        String::from_utf8(result.stdout).unwrap().trim(),
        "ASCII 123"
    );
    let result = run(&[
        "translate",
        "--engine",
        "passthrough",
        "--text",
        "ASCII",
        "--reuse-results",
        "--no-result-cache",
    ]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("함께 사용할 수 없습니다"));
}
