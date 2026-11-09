use super::*;
use std::{
    io::{BufRead, BufReader},
    net::TcpListener,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
};
static ID: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    root: std::path::PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "syousetu-direct-{}-{}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        Self { root }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn asset(url: String, body: &[u8]) -> DownloadAsset {
    DownloadAsset {
        model: "ja-ko".into(),
        file: "model-int8.gguf".into(),
        size: body.len() as u64,
        sha256: format!("{:x}", Sha256::digest(body)),
        url,
        github_release_api: None,
    }
}
fn server(responses: Vec<String>) -> (String, Arc<Mutex<Vec<String>>>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&requests);
    let task = thread::spawn(move || {
        for response in responses {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(&mut stream);
            let mut request = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
                request.push_str(&line);
            }
            log.lock().unwrap().push(request);
            stream.write_all(response.as_bytes()).unwrap();
        }
    });
    (format!("http://{address}"), requests, task)
}
fn ok(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}
fn client() -> Client {
    Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
}
#[test]
fn resumed_download_uses_range_and_verifies_the_complete_file() {
    let f = Fixture::new();
    let path = f.root.join("model-int8.gguf");
    let part = path.with_extension("part");
    fs::write(&part, b"abc").unwrap();
    let (url,requests,task)=server(vec!["HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 3-5/6\r\nContent-Length: 3\r\nConnection: close\r\n\r\ndef".into()]);
    let a = asset(url, b"abcdef");
    fetch(&client(), &a, &path, |_| {}).unwrap();
    task.join().unwrap();
    assert_eq!(fs::read(path).unwrap(), b"abcdef");
    assert!(!part.exists());
    assert!(requests.lock().unwrap()[0]
        .to_ascii_lowercase()
        .contains("range: bytes=3-"));
}
#[test]
fn ignored_range_restarts_and_corrupt_response_cannot_replace_existing_file() {
    let f = Fixture::new();
    let path = f.root.join("model-int8.gguf");
    let part = path.with_extension("part");
    fs::write(&part, b"abc").unwrap();
    let (url, _, task) = server(vec![ok("abcdef")]);
    fetch(&client(), &asset(url, b"abcdef"), &path, |_| {}).unwrap();
    task.join().unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"abcdef");
    let (url, _, task) = server(vec![ok("broken")]);
    assert!(fetch(&client(), &asset(url, b"abcdef"), &path, |_| {}).is_err());
    task.join().unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"abcdef");
    assert!(!part.exists());
}
#[test]
fn interrupted_download_keeps_prefix_for_retry_and_wrong_range_is_rejected() {
    let f = Fixture::new();
    let path = f.root.join("model-int8.gguf");
    let part = path.with_extension("part");
    let (url, _, task) = server(vec![
        "HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\nabc".into(),
    ]);
    assert!(fetch(&client(), &asset(url, b"abcdef"), &path, |_| {}).is_err());
    task.join().unwrap();
    assert_eq!(fs::read(&part).unwrap(), b"abc");
    assert!(!path.exists());
    let(url,_,task)=server(vec!["HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-2/6\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc".into()]);
    assert!(fetch(&client(), &asset(url, b"abcdef"), &path, |_| {}).is_err());
    task.join().unwrap();
    assert_eq!(fs::read(part).unwrap(), b"abc");
}
#[test]
fn api_fallback_needs_no_token_and_never_requests_fp32() {
    let f = Fixture::new();
    let path = f.root.join("model-int8.gguf");
    let (url, requests, task) = server(vec![
        "HTTP/1.1 504 Gateway Timeout\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
        ok(r#"{"assets":[{"id":17,"name":"model-int8.gguf","size":6}]}"#),
        ok("abcdef"),
    ]);
    let mut a = asset(format!("{url}/direct-int8"), b"abcdef");
    a.github_release_api = Some(format!("{url}/repos/test/model/releases/tags/v1"));
    fetch(&client(), &a, &path, |_| {}).unwrap();
    task.join().unwrap();
    let calls = requests.lock().unwrap();
    assert_eq!(calls.len(), 3);
    assert!(calls[2].starts_with("GET /repos/test/model/releases/assets/17 "));
    for request in calls.iter() {
        assert!(!request.to_ascii_lowercase().contains("authorization:"));
        assert!(!request.contains("model.safetensors"));
    }
    assert_eq!(fs::read(path).unwrap(), b"abcdef");
}
#[test]
fn completed_partial_is_installed_without_network() {
    let f = Fixture::new();
    let path = f.root.join("model-int8.gguf");
    fs::write(path.with_extension("part"), b"abcdef").unwrap();
    fetch(
        &client(),
        &asset("http://127.0.0.1:1/unreachable".into(), b"abcdef"),
        &path,
        |_| {},
    )
    .unwrap();
    assert_eq!(fs::read(path).unwrap(), b"abcdef");
}
