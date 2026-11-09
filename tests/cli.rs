#![cfg(feature = "legacy")]
//! Offline CLI workflow regressions, including source-backed migration work.
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "syousetu-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        fs::create_dir(p.join("config")).unwrap();
        // These workflow fixtures intentionally use the deterministic glossary;
        // real neural quality/performance is validated by the opt-in model test.
        fs::write(
            p.join("config/config.properties"),
            "translator_name=native\n",
        )
        .unwrap();
        Self(p)
    }
    fn write(&self, name: &str, text: impl AsRef<[u8]>) {
        let p = self.0.join(name);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_syousetu-translate"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap()
    }
    fn read(&self, name: &str) -> String {
        fs::read_to_string(self.0.join(name)).unwrap()
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn success(r: Output) -> Output {
    assert!(
        r.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&r.stdout),
        String::from_utf8_lossy(&r.stderr)
    );
    r
}
#[test]
fn worker_reports_initialization_error_before_closing_its_channel() {
    let w = Workspace::new();
    let result = w.run(&["worker", "--native-dictionary", "missing-dictionary.tsv"]);
    assert!(!result.status.success());
    let response: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(response["ready"], false);
    assert!(response["error"]
        .as_str()
        .unwrap()
        .contains("missing-dictionary.tsv"));
    w.write("source.txt", "こんにちは");
    w.write("existing.txt", "preserved");
    let result = w.run(&[
        "file",
        "--input",
        "source.txt",
        "--out",
        "existing.txt",
        "--native-dictionary",
        "missing-dictionary.tsv",
        "--jobs",
        "2",
    ]);
    assert!(!result.status.success());
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(error.contains("missing-dictionary.tsv"), "{error}");
    assert!(!error.contains("channel is empty"));
    assert!(!error.contains("30초"));
    assert_eq!(w.read("existing.txt"), "preserved");
}
#[test]
fn parallel_translation_matches_sequential_with_dictionary_and_bilingual_lines() {
    let w = Workspace::new();
    let source = "こんにちは\n猫は学生です。\n(魔法) 😀\n\n私は学生です。\n";
    w.write("source.txt", source);
    w.write("terms.tsv", "猫\t고양이\textra ignored\n");
    success(w.run(&[
        "file",
        "--input",
        "source.txt",
        "--out",
        "single.txt",
        "--dictionary",
        "terms.tsv",
        "--japanese",
        "--jobs",
        "1",
    ]));
    success(w.run(&[
        "file",
        "--input",
        "source.txt",
        "--out",
        "parallel.txt",
        "--dictionary",
        "terms.tsv",
        "--japanese",
        "--jobs",
        "2",
    ]));
    let out = w.read("parallel.txt");
    assert_eq!(out, w.read("single.txt"));
    assert!(out.contains("고양이는 학생입니다。"));
    assert!(out.contains("(마법) 😀"));
    assert_eq!(w.read("source.txt"), source);
}
#[test]
fn worker_failure_preserves_existing_output_and_source() {
    let w = Workspace::new();
    w.write("source.txt", "こんにちは\n未知語彙\n");
    w.write("out.txt", "keep existing output");
    assert!(!w
        .run(&[
            "file",
            "--input",
            "source.txt",
            "--out",
            "out.txt",
            "--jobs",
            "2",
            "--native-strict"
        ])
        .status
        .success());
    assert_eq!(w.read("out.txt"), "keep existing output");
    assert_eq!(w.read("source.txt"), "こんにちは\n未知語彙\n");
}
#[test]
fn utf8_bom_utf16_be_and_explicit_cp932_inputs() {
    let w = Workspace::new();
    w.write("utf8.txt", "\u{feff}こんにちは\n");
    let mut utf16 = vec![0xfe, 0xff];
    for unit in "こんにちは\n".encode_utf16() {
        utf16.extend(unit.to_be_bytes());
    }
    w.write("utf16.txt", utf16);
    let (sjis, _, bad) = encoding_rs::SHIFT_JIS.encode("こんにちは\n");
    assert!(!bad);
    w.write("cp932.txt", sjis.as_ref());
    for name in ["utf8.txt", "utf16.txt"] {
        success(w.run(&["file", "--input", name, "--out", "out.txt"]));
        assert_eq!(w.read("out.txt"), "안녕하세요\n");
    }
    success(w.run(&[
        "file",
        "--input",
        "cp932.txt",
        "--out",
        "out.txt",
        "--input-encoding",
        "shift_jis",
    ]));
    assert_eq!(w.read("out.txt"), "안녕하세요\n");
}
#[test]
fn properties_control_comments_headings_and_description_translation() {
    let w = Workspace::new();
    w.write("settings.properties","translator_name=native\nprint_japanese=true\ninclude_comment=true\ncomment_separator=\\n---\\n\ntitle_format=[currentPage]/[lastPage] [koreanTitle] ([japaneseTitle])\npage_number_format=000\nmax_cpu_core_count=2\n");
    w.write("index.html","<h1>物語</h1><div class='p-novel__summary'>こんにちは</div><div class='p-eplist__sublist'><a href='/n1234ab/1/'>1</a><a href='/n1234ab/2/'>2</a><a href='/n1234ab/3/'>3</a></div>");
    w.write("chapters/2.html","<h1 class='p-novel__title--rensai'>魔法</h1><div class='js-novel-text p-novel__text p-novel__text--preface'><p>ありがとうございます</p></div><div class='js-novel-text p-novel__text'><p>私は学生です。</p></div><div class='js-novel-text p-novel__text p-novel__text--afterword'><p>こんにちは</p></div>");
    success(w.run(&[
        "download",
        "--url",
        "https://ncode.syosetu.com/n1234ab/",
        "--html",
        "index.html",
        "--episode-html-dir",
        "chapters",
        "--config",
        "settings.properties",
        "--out",
        "novel.txt",
        "--start",
        "2",
        "--end",
        "2",
    ]));
    let text = w.read("novel.txt");
    assert!(
        text.contains("物語\nhttps://ncode.syosetu.com/n1234ab/\n\n안녕하세요\n"),
        "{text}"
    );
    assert!(text.contains("002/002 마법 (魔法)"), "{text}");
    assert!(text.contains("ありがとうございます\n감사합니다"), "{text}");
    assert!(text.contains("私は学生です。\n나는 학생입니다。"), "{text}");
    assert_eq!(text.matches("---").count(), 4);
    success(w.run(&[
        "download",
        "--url",
        "https://ncode.syosetu.com/n1234ab/",
        "--html",
        "index.html",
        "--episode-html-dir",
        "chapters",
        "--config",
        "settings.properties",
        "--out",
        "without.txt",
        "--start",
        "2",
        "--end",
        "2",
        "--no-comments",
        "--no-japanese",
    ]));
    assert!(!w.read("without.txt").contains("감사합니다"));
    assert!(!w.read("without.txt").contains("---"));
}
#[test]
fn offline_pagination_uses_all_real_index_pages() {
    let w = Workspace::new();
    let mut first =
        "<h1 class='storyTitle'>物語</h1><span class='totalEpisode'>101</span>".to_owned();
    for n in 1..=100 {
        first.push_str(&format!(
            "<a class='episodeTitle' href='/story/123/{n}'>chapter</a>"
        ));
    }
    w.write("first.html", first);
    w.write(
        "index/2.html",
        "<a class='episodeTitle' href='/story/123/101'>last chapter</a>",
    );
    let args = [
        "inspect",
        "--url",
        "https://novelup.plus/story/123",
        "--html",
        "first.html",
        "--index-html-dir",
        "index",
    ];
    let result = success(w.run(&args));
    let json: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(json["episodes"].as_array().unwrap().len(), 101);
    assert_eq!(json["episodes"][100], "https://novelup.plus/story/123/101");
    fs::remove_file(w.0.join("index/2.html")).unwrap();
    assert!(!w.run(&args).status.success());
}
#[test]
fn automatic_filename_uses_configured_filter_and_length() {
    let w = Workspace::new();
    w.write("config.properties","translator_name=dummy\nfilter_title_bracket=true\nmax_title_length=2\nsave_without_file_chooser=true\n");
    w.write(
        "index.html",
        "<h1>【tag】物語 [extra]/bad?</h1><a href='/works/1/episodes/1'>chapter</a>",
    );
    w.write("chapters/1.html","<h1 class='widget-episodeTitle'>title</h1><div class='widget-episodeBody'><p>こんにちは</p></div>");
    success(w.run(&[
        "download",
        "--url",
        "https://kakuyomu.jp/works/1",
        "--html",
        "index.html",
        "--episode-html-dir",
        "chapters",
        "--config",
        "config.properties",
        "--output-dir",
        "results",
    ]));
    assert!(w.0.join("results/物語.txt").is_file());
}
#[test]
fn invalid_options_and_aliased_input_paths_fail() {
    let w = Workspace::new();
    w.write("source.txt", "こんにちは\n");
    for args in [
        vec!["file", "--input", "--out", "out.txt"],
        vec!["file", "--typo"],
        vec!["file", "--input", "source.txt", "--out", "source.txt"],
        vec!["file", "--input", "source.txt", "--out", "./source.txt"],
        vec!["not-a-command"],
    ] {
        assert!(!w.run(&args).status.success(), "{args:?}");
    }
    assert_eq!(w.read("source.txt"), "こんにちは\n");
    assert!(!w.0.join("out.txt").exists());
}

#[test]
fn config_dialog_json_properties_roundtrip_and_failed_save_preserves_file() {
    let w = Workspace::new();
    let result = success(w.run(&["inspect-config"]));
    let mut config: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    config["title_format"] = " [currentPage]\n[koreanTitle] 😀".into();
    config["comment_separator"] = " \t코멘트\r\n\\끝".into();
    config["eztrans_path"] = "C:\\번역기\\ezTrans XP".into();
    w.write("edited.json", serde_json::to_vec(&config).unwrap());
    success(w.run(&[
        "write-config",
        "--json",
        "edited.json",
        "--out",
        "settings.properties",
    ]));
    let result = success(w.run(&["inspect-config", "--config", "settings.properties"]));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&result.stdout).unwrap(),
        config
    );
    let saved = w.read("settings.properties");
    config["max_retry_count"] = 99.into();
    w.write("edited.json", serde_json::to_vec(&config).unwrap());
    assert!(!w
        .run(&[
            "write-config",
            "--json",
            "edited.json",
            "--out",
            "settings.properties"
        ])
        .status
        .success());
    assert_eq!(w.read("settings.properties"), saved);
}
