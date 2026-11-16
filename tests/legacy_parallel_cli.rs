#![cfg(feature = "legacy")]
use std::{fs, process::Command};

#[test]
fn translation_text_is_sent_over_the_worker_protocol_not_worker_options() {
    for engine in ["passthrough", "native"] {
        let run = |jobs| {
            Command::new(env!("CARGO_BIN_EXE_syousetu-translate"))
                .args([
                    "translate",
                    "--engine",
                    engine,
                    "--jobs",
                    jobs,
                    "--no-learning",
                    "--text",
                    "猫",
                ])
                .output()
                .unwrap()
        };
        let expected = run("1");
        let actual = run("2");
        assert!(expected.status.success());
        assert!(
            actual.status.success(),
            "{}",
            String::from_utf8_lossy(&actual.stderr)
        );
        assert_eq!(actual.stdout, expected.stdout);
    }
}

#[test]
fn file_paths_and_decoding_options_remain_with_the_parent_process() {
    let directory = std::env::temp_dir().join(format!(
        "syousetu-legacy-file-workers-{}",
        std::process::id()
    ));
    fs::create_dir(&directory).unwrap();
    let source = directory.join("source.txt");
    let target = directory.join("target.txt");
    fs::write(&source, "猫\n犬\n猫\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_syousetu-translate"))
        .args([
            "file",
            "--engine",
            "passthrough",
            "--jobs",
            "2",
            "--no-learning",
            "--no-japanese",
            "--input-encoding",
            "utf-8",
            "--input",
        ])
        .arg(&source)
        .arg("--out")
        .arg(&target)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read_to_string(target).unwrap(), "猫\n犬\n猫\n");
    fs::remove_dir_all(directory).unwrap();
}
