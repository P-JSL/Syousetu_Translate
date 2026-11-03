use crate::{config::Config, flag, num, value};
use anyhow::{bail, Result};
use std::{collections::HashSet, path::Path};
pub fn settings(args: &[String]) -> Result<Config> {
    let default = Path::new("config/config.properties");
    let mut cfg = if let Some(path) = value(args, "--config") {
        Config::load(Path::new(&path))?
    } else if default.is_file() {
        Config::load(default)?
    } else {
        Config::default()
    };
    for (positive, negative, field) in [
        ("--japanese", "--no-japanese", &mut cfg.print_japanese),
        ("--furigana", "--no-furigana", &mut cfg.include_furigana),
        (
            "--include-comments",
            "--no-comments",
            &mut cfg.include_comment,
        ),
        (
            "--filter-title-bracket",
            "--keep-title-bracket",
            &mut cfg.filter_title_bracket,
        ),
    ] {
        if flag(args, positive) && flag(args, negative) {
            bail!("서로 충돌하는 옵션: {positive}, {negative}");
        }
        if flag(args, positive) {
            *field = true;
        }
        if flag(args, negative) {
            *field = false;
        }
    }
    if let Some(title) = value(args, "--title-format") {
        cfg.title_format = title;
    }
    if let Some(pattern) = value(args, "--page-number-format") {
        crate::config::format_number(1, &pattern)?;
        cfg.page_number_format = pattern;
    }
    if let Some(separator) = value(args, "--comment-separator") {
        cfg.comment_separator = separator;
    }
    if let Some(agent) = value(args, "--user-agent") {
        cfg.user_agent = agent;
    }
    if value(args, "--delay-ms").is_some() {
        cfg.page_download_delay = num(args, "--delay-ms", 0)? as u64;
        cfg.hameln_page_download_delay = cfg.page_download_delay;
    }
    if value(args, "--retry").is_some() {
        let count = num(args, "--retry", 0)?;
        if count > 10 {
            bail!("--retry는 0~10이어야 합니다");
        }
        cfg.max_retry_count = count as u32;
    }
    if value(args, "--jobs").is_some() {
        let jobs = num(args, "--jobs", 1)?;
        if !(1..=16).contains(&jobs) {
            bail!("--jobs는 1~16이어야 합니다");
        }
        cfg.max_cpu_core_count = jobs as isize;
    }
    Ok(cfg)
}
pub fn validate(command: &str, args: &[String]) -> Result<()> {
    let common_values = [
        "--config",
        "--dictionary",
        "--engine",
        "--eztrans-dir",
        "--native-dictionary",
        "--ehnd-dir",
        "--jobs",
    ];
    let common_flags = [
        "--japanese",
        "--no-japanese",
        "--passthrough",
        "--native-strict",
        "--ehnd-regex-experimental",
        "--ehnd-regex-strict",
        "--ehnd-literal-only",
    ];
    let download_values = [
        "--url",
        "--out",
        "--html",
        "--episode-html-dir",
        "--index-html-dir",
        "--start",
        "--end",
        "--delay-ms",
        "--retry",
        "--user-agent",
        "--title-format",
        "--page-number-format",
        "--comment-separator",
        "--output-dir",
    ];
    let download_flags = [
        "--furigana",
        "--no-furigana",
        "--include-comments",
        "--no-comments",
        "--no-delay",
        "--filter-title-bracket",
        "--keep-title-bracket",
        "--auto-output",
    ];
    let mut values = Vec::new();
    let mut flags = Vec::new();
    match command {
        "download" | "file" | "worker" => {
            values.extend(common_values);
            flags.extend(common_flags);
            if command != "file" {
                values.extend(download_values);
                flags.extend(download_flags);
            } else {
                values.extend(["--input", "--out", "--input-encoding"]);
            }
            if command == "worker" {
                values.extend(["--input", "--input-encoding"]);
            }
        }
        "inspect" => values.extend([
            "--config",
            "--url",
            "--html",
            "--index-html-dir",
            "--delay-ms",
            "--retry",
            "--user-agent",
        ]),
        "inspect-config" => values.push("--config"),
        "inspect-root-dll" => values.extend(["--dll", "--out"]),
        "inspect-dat" | "inspect-ehnd" | "audit-ehnd-control" => values.extend(["--dir", "--out"]),
        "audit-ehnd-regex" => {
            values.extend(["--dir", "--out"]);
            flags.push("--strict");
        }
        "compare-engines" => {
            values.extend(["--eztrans-dir", "--input", "--out", "--native-dictionary"])
        }
        _ => bail!("알 수 없는 명령: {command}"),
    }
    let mut index = 0;
    let mut seen = HashSet::new();
    while index < args.len() {
        let arg = args[index].as_str();
        if !seen.insert(arg) {
            bail!("중복 옵션: {arg}");
        }
        if values.contains(&arg) {
            let next = args
                .get(index + 1)
                .filter(|v| !v.starts_with("--"))
                .ok_or_else(|| anyhow::anyhow!("옵션 {arg}의 값이 없습니다"))?;
            if next.is_empty() && !["--comment-separator", "--title-format"].contains(&arg) {
                bail!("옵션 {arg}의 값이 비어 있습니다");
            }
            index += 2;
        } else if flags.contains(&arg) {
            index += 1;
        } else {
            bail!("지원하지 않는 옵션: {arg}");
        }
    }
    if flag(args, "--out") && flag(args, "--output-dir") {
        bail!("--out과 --output-dir는 함께 사용할 수 없습니다");
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }
    #[test]
    fn invalid_options_are_not_silently_ignored() {
        for values in [
            vec!["--input"],
            vec!["--input", "--out", "x"],
            vec!["--typo"],
            vec!["--jobs", "2", "--jobs", "3"],
        ] {
            assert!(validate("file", &args(&values)).is_err());
        }
        assert!(validate("typo", &[]).is_err());
        assert!(settings(&args(&["--jobs", "0"])).is_err());
        assert!(settings(&args(&["--japanese", "--no-japanese"])).is_err());
    }
}
