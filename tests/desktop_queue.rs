use std::{
    ffi::OsString,
    fs,
    time::{Duration, Instant},
};
use syousetu_translate::{
    config::Config,
    jobs::{Queue, Status},
};

#[test]
fn desktop_queue_reorders_waiting_jobs_isolates_settings_and_continues_after_failure() {
    let directory = std::env::temp_dir().join(format!("syousetu-desktop-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    let index = "<h1>物語</h1><div class='p-novel__summary'>こんにちは</div><div class='p-eplist__sublist'><a href='/n1234ab/1/'>1</a></div>";
    let episode = "<h1 class='p-novel__title--rensai'>魔法</h1><div class='js-novel-text p-novel__text'><p>私は学生です。</p></div>";
    fs::write(directory.join("index.html"), index).unwrap();
    fs::create_dir(directory.join("chapters")).unwrap();
    fs::write(directory.join("chapters/1.html"), episode).unwrap();
    let mut queue = Queue::new(env!("CARGO_BIN_EXE_syousetu-translate"), &directory);
    let config = Config {translator_name:"passthrough".into(), ..Config::default()};
    let args = |input: &str, output: &str| -> Vec<OsString> {
        [
            "download",
            "--url",
            "https://ncode.syosetu.com/n1234ab/",
            "--html",
            input,
            "--episode-html-dir",
            "chapters",
            "--out",
            output,
            "--progress-json",
            "--no-delay",
        ]
        .map(OsString::from)
        .to_vec()
    };
    let failed = queue
        .enqueue(
            "bad source".into(),
            args("absent.html", "missing.txt"),
            &config,
        )
        .unwrap();
    let plain = queue
        .enqueue("plain".into(), args("index.html", "plain.txt"), &config)
        .unwrap();
    let mut bilingual = config.clone();
    bilingual.print_japanese = true;
    let both = queue
        .enqueue(
            "bilingual".into(),
            args("index.html", "both.txt"),
            &bilingual,
        )
        .unwrap();
    queue.move_waiting(both, -1);
    assert_eq!(
        queue.jobs.iter().map(|job| job.id).collect::<Vec<_>>(),
        vec![failed, both, plain]
    );
    queue.poll();
    queue.remove(failed); // Running tasks cannot be deleted or moved.
    queue.move_waiting(failed, 1);
    assert_eq!(queue.jobs[0].id, failed);
    let deadline = Instant::now() + Duration::from_secs(15);
    while queue.busy() && Instant::now() < deadline {
        queue.poll();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!queue.busy(), "queue timed out");
    assert_eq!(queue.get(failed).unwrap().status, Status::Error);
    assert_eq!(queue.get(plain).unwrap().status, Status::Done);
    assert_eq!(queue.get(both).unwrap().status, Status::Done);
    assert_eq!(
        queue
            .get(both)
            .unwrap()
            .progress
            .as_ref()
            .map(|p| (p.0, p.1)),
        Some((1, 1))
    );
    assert!(fs::read_to_string(directory.join("plain.txt"))
        .unwrap()
        .contains("私は学生です。\n"));
    assert!(!fs::read_to_string(directory.join("plain.txt"))
        .unwrap()
        .contains("私は学生です。\n私は学生です。\n"));
    assert!(fs::read_to_string(directory.join("both.txt"))
        .unwrap()
        .contains("私は学生です。\n私は学生です。\n"));
    assert_eq!(
        fs::read_to_string(directory.join("index.html")).unwrap(),
        index
    );
    assert_eq!(
        fs::read_to_string(directory.join("chapters/1.html")).unwrap(),
        episode
    );
    assert!(!directory.join("missing.txt").exists());
    queue.remove(failed);
    assert!(queue.get(failed).is_none());
    let inspect = queue
        .enqueue(
            "work information".into(),
            [
                "inspect",
                "--url",
                "https://ncode.syosetu.com/n1234ab/",
                "--html",
                "index.html",
            ]
            .map(OsString::from)
            .to_vec(),
            &bilingual,
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while queue.busy() && Instant::now() < deadline {
        queue.poll();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!queue.busy());
    let job = queue.get(inspect).unwrap();
    assert_eq!(job.status, Status::Done);
    assert!(!job.output_truncated);
    let work: serde_json::Value = serde_json::from_str(&job.output).unwrap();
    assert_eq!(work["title"], "物語");
    assert_eq!(work["episodes"].as_array().unwrap().len(), 1);
    drop(queue);
    fs::remove_dir_all(directory).unwrap();
}
