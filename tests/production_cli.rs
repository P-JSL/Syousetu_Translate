#![cfg(not(feature = "legacy"))]
use std::process::Command;

#[test]
fn production_build_rejects_excluded_backends_and_filters() {
    for args in [
        vec!["translate", "--text", "原文", "--engine", "native"],
        vec!["translate", "--text", "原文", "--engine", "eztrans"],
        vec![
            "translate",
            "--text",
            "原文",
            "--engine",
            "passthrough",
            "--ehnd-literal-only",
        ],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_syousetu-translate"))
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("legacy"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
