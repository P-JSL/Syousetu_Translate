//! Std-only release/dependency audit. Compile with rustc; no tooling crates
//! enter the application graph or require a separate interpreter.
use std::{collections::BTreeSet, fs, path::Path, process::Command};
type Result<T> = std::result::Result<T, String>;
pub(crate) const FILES: [&str; 8] = [
    "README.md",
    "THIRD_PARTY_NOTICES.md",
    "licenses/Apache-2.0.txt",
    "licenses/MIT-SMaLL100.txt",
    "licenses/MIT-gacha-pickup-calculator.txt",
    "syousetu-gui.exe",
    "syousetu-translate.exe",
    "웹소설 번역기.bat",
];
fn banned(name: &str) -> bool {
    matches!(
        name,
        "tokenizers"
            | "candle-nn"
            | "zip"
            | "safetensors"
            | "fancy-regex"
            | "cudarc"
            | "candle-kernels"
            | "candle-metal-kernels"
            | "cutile"
            | "pyo3"
            | "accelerate-src"
            | "intel-mkl-src"
    )
}
fn deps() -> Result<()> {
    let target = if cfg!(windows) {
        "x86_64-pc-windows-msvc"
    } else {
        "x86_64-unknown-linux-gnu"
    };
    let result = Command::new("cargo")
        .args([
            "tree",
            "--locked",
            "--edges",
            "normal",
            "--prefix",
            "none",
            "--no-default-features",
            "--features",
            "gui",
            "--target",
            target,
        ])
        .output()
        .map_err(|e| e.to_string())?;
    if !result.status.success() {
        return Err(String::from_utf8_lossy(&result.stderr).into_owned());
    }
    let graph = String::from_utf8(result.stdout).map_err(|e| e.to_string())?;
    let names = graph
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .collect::<BTreeSet<_>>();
    let unexpected = names
        .iter()
        .copied()
        .filter(|n| banned(n))
        .collect::<Vec<_>>();
    if !unexpected.is_empty() {
        return Err(format!(
            "Unused production dependencies enabled: {unexpected:?}"
        ));
    }
    println!("Production dependency audit passed: {} unique crates; no unused tokenizer/archive/training/GPU adapters.",names.len());
    Ok(())
}
fn u16le(data: &[u8], at: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(
        data.get(at..at + 2)
            .ok_or("Truncated PE")?
            .try_into()
            .unwrap(),
    ))
}
fn u32le(data: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        data.get(at..at + 4)
            .ok_or("Truncated PE")?
            .try_into()
            .unwrap(),
    ))
}
pub(crate) fn imports(data: &[u8], subsystem: u16) -> Result<Vec<String>> {
    if data.get(..2) != Some(b"MZ") {
        return Err("Not a Windows executable".into());
    }
    let pe = u32le(data, 0x3c)? as usize;
    if data.get(pe..pe + 4) != Some(b"PE\0\0") || u16le(data, pe + 4)? != 0x8664 {
        return Err("Expected Windows x64 PE".into());
    }
    let opt = pe + 24;
    if u16le(data, opt)? != 0x20b || u16le(data, opt + 68)? != subsystem {
        return Err("Wrong PE image kind/subsystem".into());
    }
    let sections = pe + 24 + u16le(data, pe + 20)? as usize;
    let count = u16le(data, pe + 6)? as usize;
    let offset = |rva: u32| -> Result<usize> {
        for i in 0..count {
            let s = sections + 40 * i;
            let address = u32le(data, s + 12)?;
            let size = u32le(data, s + 16)?;
            if rva >= address && rva - address < size {
                return Ok((u32le(data, s + 20)? + rva - address) as usize);
            }
        }
        Err("Unmapped PE RVA".into())
    };
    let rva = u32le(data, opt + 120)?;
    if rva == 0 {
        return Ok(Vec::new());
    }
    let mut at = offset(rva)?;
    let mut names = Vec::new();
    for _ in 0..256 {
        let entry = data.get(at..at + 20).ok_or("Truncated import table")?;
        if entry.iter().all(|&b| b == 0) {
            return Ok(names);
        }
        let name = offset(u32le(data, at + 12)?)?;
        let tail = data.get(name..).ok_or("Truncated import name")?;
        let end = tail
            .iter()
            .position(|&b| b == 0)
            .ok_or("Unterminated import name")?;
        names.push(
            String::from_utf8(tail[..end].to_vec())
                .map_err(|e| e.to_string())?
                .to_ascii_lowercase(),
        );
        at += 20;
    }
    Err("Oversized import table".into())
}
fn system_dll(name: &str) -> bool {
    name.starts_with("api-ms-win-")
        || name.starts_with("ext-ms-win-")
        || matches!(
            name,
            "advapi32.dll"
                | "bcrypt.dll"
                | "bcryptprimitives.dll"
                | "comctl32.dll"
                | "comdlg32.dll"
                | "crypt32.dll"
                | "dwmapi.dll"
                | "gdi32.dll"
                | "imm32.dll"
                | "kernel32.dll"
                | "msvcrt.dll"
                | "mswsock.dll"
                | "ncrypt.dll"
                | "ntdll.dll"
                | "ole32.dll"
                | "oleaut32.dll"
                | "opengl32.dll"
                | "propsys.dll"
                | "secur32.dll"
                | "shell32.dll"
                | "shlwapi.dll"
                | "user32.dll"
                | "userenv.dll"
                | "uxtheme.dll"
                | "version.dll"
                | "winmm.dll"
                | "winspool.drv"
                | "ws2_32.dll"
                | "vcruntime140.dll"
                | "vcruntime140_1.dll"
                | "ucrtbase.dll"
                | "dbghelp.dll"
                | "netapi32.dll"
                | "iphlpapi.dll"
                | "powrprof.dll"
        )
}
fn files(root: &Path, path: &Path, out: &mut BTreeSet<String>) -> Result<()> {
    for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let p = entry.path();
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() {
            return Err("Symlinks cannot enter a release".into());
        }
        if kind.is_dir() {
            files(root, &p, out)?;
        } else if kind.is_file() {
            out.insert(
                p.strip_prefix(root)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    Ok(())
}
pub(crate) fn web(root: &Path) -> Result<()> {
    let mut actual = BTreeSet::new();
    files(root, root, &mut actual)?;
    let bare = FILES.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>();
    let mut bundled = bare.clone();
    for name in [
        "config.json",
        "model-int8.gguf",
        "sentencepiece.bpe.model",
        "vocab.json",
    ] {
        bundled.insert(format!("models/ja-ko/{name}"));
    }
    if actual != bare && actual != bundled {
        return Err(format!("Unexpected release files: {actual:?}"));
    }
    let mut names = BTreeSet::new();
    for (name, kind) in [("syousetu-gui.exe", 2), ("syousetu-translate.exe", 3)] {
        for import in imports(&fs::read(root.join(name)).map_err(|e| e.to_string())?, kind)? {
            if !system_dll(&import) {
                return Err(format!("Unbundled non-system runtime dependency: {import}"));
            }
            names.insert(import);
        }
    }
    println!(
        "Release audit passed: {} files, only Windows/MSVC runtime imports: {names:?}",
        actual.len()
    );
    Ok(())
}
fn directory_from_json(text: &str) -> Result<std::path::PathBuf> {
    let suffix = text
        .split_once("\"directory\"")
        .ok_or("Directory missing in model status")?
        .1;
    let suffix = suffix
        .trim_start()
        .strip_prefix(':')
        .ok_or("Invalid directory field")?
        .trim_start()
        .strip_prefix('"')
        .ok_or("Invalid directory string")?;
    let mut path = String::new();
    let mut chars = suffix.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => return Ok(path.into()),
            '\\' => match chars.next() {
                Some('\\') => path.push('\\'),
                Some('"') => path.push('"'),
                Some('/') => path.push('/'),
                _ => return Err("Unsupported path escape".into()),
            },
            ch => path.push(ch),
        }
    }
    Err("Unterminated model directory".into())
}
fn portable(app: &Path, models: &Path, work: &Path) -> Result<()> {
    let app = fs::canonicalize(app).map_err(|e| e.to_string())?;
    let bundled = app.join("models/ja-ko");
    fs::create_dir_all(&bundled).map_err(|e| e.to_string())?;
    for name in [
        "config.json",
        "model-int8.gguf",
        "sentencepiece.bpe.model",
        "vocab.json",
    ] {
        fs::copy(models.join("ja-ko").join(name), bundled.join(name)).map_err(|e| e.to_string())?;
    }
    portable_existing(&app, work)
}
fn portable_existing(app: &Path, work: &Path) -> Result<()> {
    let app = fs::canonicalize(app).map_err(|e| e.to_string())?;
    fs::create_dir_all(work).map_err(|e| e.to_string())?;
    let exe = app.join(if cfg!(windows) {
        "syousetu-translate.exe"
    } else {
        "syousetu-translate"
    });
    let invoke = |args: &[&str]| -> Result<String> {
        let result = Command::new(&exe)
            .args(args)
            .env("SYOUSETU_MODEL_DIR", "")
            .current_dir(work)
            .output()
            .map_err(|e| e.to_string())?;
        if !result.status.success() {
            return Err(format!(
                "Portable CLI failed: {}",
                String::from_utf8_lossy(&result.stderr)
            ));
        }
        Ok(String::from_utf8_lossy(&result.stdout).into_owned())
    };
    let status = invoke(&["models", "--verify"])?;
    let selected = fs::canonicalize(directory_from_json(&status)?).map_err(|e| e.to_string())?;
    if selected != fs::canonicalize(app.join("models")).map_err(|e| e.to_string())? {
        return Err(format!("Bundled model was not selected: {status}"));
    }
    invoke(&[
        "models",
        "--prepare",
        "--ca-bundle",
        "intentionally-missing-ca.pem",
    ])?;
    let result = invoke(&[
        "translate",
        "--text",
        "私は学生です。",
        "--no-learning",
        "--jobs",
        "1",
    ])?;
    if !result.chars().any(|c| ('가'..='힣').contains(&c)) {
        return Err("Portable inference produced no Korean".into());
    }
    println!(
        "Portable model auto-discovery, offline preparation and translation passed: {}",
        result.trim()
    );
    Ok(())
}
fn check_ci(run: &str) -> Result<()> {
    if run.is_empty() || !run.chars().all(|c| c.is_ascii_digit()) {
        return Err("Invalid CI run ID".into());
    }
    let repo = std::env::var("GITHUB_REPOSITORY").map_err(|e| e.to_string())?;
    if repo != "P-JSL/Syousetu_Translate" {
        return Err("Unexpected repository".into());
    }
    let result=Command::new("gh").args(["api",&format!("repos/{repo}/actions/runs/{run}"),"--jq","select(.name==\"Rust CI\" and .status==\"completed\" and .conclusion==\"success\") | .head_sha"]).output().map_err(|e|e.to_string())?;
    let git = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|e| e.to_string())?;
    if !result.status.success()
        || !git.status.success()
        || String::from_utf8_lossy(&result.stdout).trim()
            != String::from_utf8_lossy(&git.stdout).trim()
    {
        return Err("Successful artifact CI must match checked-out source".into());
    }
    println!("Successful CI source matches.");
    Ok(())
}
fn invoke(exe: &Path, args: &[&str]) -> Result<String> {
    let status = Command::new(exe)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if !status.status.success() {
        return Err(format!(
            "CLI failed: {}",
            String::from_utf8_lossy(&status.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&status.stdout).into_owned())
}
fn model_smoke(exe: &Path, root: &Path) -> Result<()> {
    if root.exists() {
        return Err("Fresh model test requires an absent directory".into());
    }
    let dir = root.to_str().ok_or("Invalid model path")?;
    println!(
        "{}",
        invoke(exe, &["models", "--prepare", "--model-dir", dir])?
    );
    let mut names = BTreeSet::new();
    files(&root.join("ja-ko"), &root.join("ja-ko"), &mut names)?;
    let expected = [
        ("config.json", 890u64),
        ("model-int8.gguf", 354_200_608),
        ("sentencepiece.bpe.model", 2_423_393),
        ("vocab.json", 3_708_092),
    ];
    if names != expected.iter().map(|(n, _)| n.to_string()).collect() {
        return Err(format!("Unexpected model files: {names:?}"));
    }
    for (name, size) in expected {
        if fs::metadata(root.join("ja-ko").join(name))
            .map_err(|e| e.to_string())?
            .len()
            != size
        {
            return Err(format!("Unexpected model size: {name}"));
        }
    }
    invoke(exe, &["models", "--verify", "--model-dir", dir])?;
    // A nonexistent CA file proves a cached prepare does not construct any
    // network client, while retaining the environment's real proxy settings.
    invoke(
        exe,
        &[
            "models",
            "--prepare",
            "--model-dir",
            dir,
            "--ca-bundle",
            "intentionally-missing-ca.pem",
        ],
    )?;
    let output = invoke(
        exe,
        &[
            "translate",
            "--text",
            "私は学生です。",
            "--model-dir",
            dir,
            "--no-learning",
            "--jobs",
            "1",
        ],
    )?;
    if !output.chars().any(|c| ('가'..='힣').contains(&c)) {
        return Err(format!("Expected direct Korean output: {output}"));
    }
    println!(
        "Fresh INT8-only model passed: 360332983 bytes; offline reuse and translation: {}",
        output.trim()
    );
    Ok(())
}
fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    let result = match args.get(1).map(String::as_str) {
        Some("deps") => deps(),
        Some("web") => args
            .get(2)
            .ok_or("Artifact directory required".into())
            .and_then(|p| web(Path::new(p))),
        Some("check-ci") => args
            .get(2)
            .ok_or("CI run ID required".into())
            .and_then(|r| check_ci(r)),
        Some("model-smoke") if args.len() == 4 => {
            model_smoke(Path::new(&args[2]), Path::new(&args[3]))
        }
        Some("portable") if args.len() == 5 => portable(
            Path::new(&args[2]),
            Path::new(&args[3]),
            Path::new(&args[4]),
        ),
        Some("portable-existing") if args.len() == 4 => {
            portable_existing(Path::new(&args[2]), Path::new(&args[3]))
        }
        _ => Err(
            "Usage: release-audit deps | web DIR | check-ci RUN | model-smoke CLI NEW_DIR".into(),
        ),
    };
    if let Err(e) = result {
        eprintln!("ERROR: {e}");
        std::process::exit(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn optional_adapters_are_forbidden_but_used_os_loader_is_allowed() {
        assert!(banned("tokenizers"));
        assert!(banned("zip"));
        assert!(banned("candle-nn"));
        assert!(!banned("libloading"));
        assert!(system_dll("bcryptprimitives.dll"));
        assert!(!system_dll("J2KEngine.dll"));
        assert!(!system_dll("python312.dll"));
    }
    #[test]
    fn truncated_and_non_windows_images_are_rejected() {
        assert!(imports(b"not a PE", 2).is_err());
        assert!(imports(b"MZ", 2).is_err());
    }
}
