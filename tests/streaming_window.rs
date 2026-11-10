//! A later download failure must not discard an already downloaded prefix.
use std::{fs, path::PathBuf, process::Command};
struct Workspace(PathBuf);
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn missing_later_episode_flushes_predecessor_and_retains_original_backup() {
    let dir = std::env::temp_dir().join(format!("syousetu-window-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    let workspace = Workspace(dir);
    fs::create_dir(workspace.0.join("episodes")).unwrap();
    fs::write(
        workspace.0.join("index.html"),
        "<h1>物語</h1><a href='/works/1/episodes/1'>一</a><a href='/works/1/episodes/2'>二</a>",
    )
    .unwrap();
    fs::write(workspace.0.join("episodes/1.html"), "<h1 class='widget-episodeTitle'>第一章</h1><div class='widget-episodeBody'><p>最初の本文</p></div>").unwrap();
    fs::write(workspace.0.join("live.txt"), "previous output").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_syousetu-translate"))
        .current_dir(&workspace.0)
        .args([
            "download",
            "--url",
            "https://kakuyomu.jp/works/1",
            "--html",
            "index.html",
            "--episode-html-dir",
            "episodes",
            "--engine",
            "passthrough",
            "--out",
            "live.txt",
            "--no-delay",
            "--progress-json",
        ])
        .output()
        .unwrap();
    assert!(!result.status.success());
    let text = fs::read_to_string(workspace.0.join("live.txt")).unwrap();
    assert!(text.contains("[1/2] 第一章 / 第一章"), "{text}");
    assert!(text.contains("最初の本文"));
    assert!(!text.contains("[2/2]"));
    assert_eq!(
        fs::read_to_string(workspace.0.join("live.backup")).unwrap(),
        "previous output"
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("2.html"), "{stderr}");
    assert!(stderr
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .any(|event| event["unit"] == "chapters" && event["done"] == 1 && event["total"] == 2));
}
