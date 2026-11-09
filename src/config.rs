//! Source-visible Java Properties settings, with explicit CLI compatibility.
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub print_japanese: bool,
    pub title_format: String,
    pub filter_title_bracket: bool,
    pub page_number_format: String,
    pub page_download_delay: u64,
    pub hameln_page_download_delay: u64,
    pub max_retry_count: u32,
    pub max_cpu_core_count: isize,
    pub include_furigana: bool,
    pub include_comment: bool,
    pub comment_separator: String,
    pub translator_name: String,
    pub eztrans_path: Option<String>,
    pub model_dir: Option<String>,
    pub learning_path: Option<String>,
    pub learning_enabled: bool,
    pub parallel_mode: String,
    pub ca_bundle: Option<String>,
    pub user_agent: String,
    pub max_title_length: usize,
    pub save_without_file_chooser: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            print_japanese: false,
            title_format: "[[currentPage]/[lastPage]] [koreanTitle] / [japaneseTitle]".into(),
            filter_title_bracket: false,
            page_number_format: "#".into(),
            page_download_delay: 1200,
            hameln_page_download_delay: 5500,
            max_retry_count: 2,
            max_cpu_core_count: -1,
            include_furigana: false,
            include_comment: false,
            comment_separator: "\n--------------------\n".into(),
            translator_name: "internal".into(),
            eztrans_path: None,
            model_dir: None,
            learning_path: None,
            learning_enabled: true,
            parallel_mode: "auto".into(),
            ca_bundle: None,
            user_agent: "SyousetuTranslateRust/0.3 (personal web novel reader)".into(),
            max_title_length: 25,
            save_without_file_chooser: false,
        }
    }
}
impl Config {
    pub fn model_directory(&self) -> std::path::PathBuf {
        self.model_dir.as_ref().map(std::path::PathBuf::from).unwrap_or_else(crate::models::default_dir)
    }
    pub fn memory_file(&self) -> std::path::PathBuf {
        self.learning_path.as_ref().map(std::path::PathBuf::from).unwrap_or_else(crate::learning::default_path)
    }
    pub fn load(path: &Path) -> Result<Self> {
        let bytes =
            fs::read(path).with_context(|| format!("설정 파일 읽기 실패: {}", path.display()))?;
        let text = if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
            let encoding = if bytes[0] == 0xff {
                encoding_rs::UTF_16LE
            } else {
                encoding_rs::UTF_16BE
            };
            let (text, _, bad) = encoding.decode(&bytes[2..]);
            if bad {
                bail!("설정 파일 UTF-16 디코딩 실패");
            }
            text.into_owned()
        } else {
            String::from_utf8(bytes).context("설정 파일은 UTF-8 또는 UTF-16 BOM이어야 합니다")?
        };
        Self::parse(&text)
    }
    pub fn parse(text: &str) -> Result<Self> {
        let mut cfg = Self {
            title_format: "([currentPage]/[lastPage])\n\n[koreanTitle]".into(),
            filter_title_bracket: true,
            page_download_delay: 300,
            max_retry_count: 5,
            max_cpu_core_count: -1,
            ..Self::default()
        };
        for (key, value) in properties(text)? {
            let boolean = || {
                if value.eq_ignore_ascii_case("true") {
                    Ok(true)
                } else if value.eq_ignore_ascii_case("false") {
                    Ok(false)
                } else {
                    bail!("설정 {key}: true 또는 false가 필요합니다")
                }
            };
            let number = || {
                value
                    .parse::<u64>()
                    .with_context(|| format!("설정 {key}: 음수가 아닌 정수가 필요합니다"))
            };
            match key.as_str() {
                "print_japanese" => cfg.print_japanese = boolean()?,
                "filter_title_bracket" => cfg.filter_title_bracket = boolean()?,
                "include_furigana" => cfg.include_furigana = boolean()?,
                "include_comment" => cfg.include_comment = boolean()?,
                "save_without_file_chooser" => cfg.save_without_file_chooser = boolean()?,
                "learning_enabled" => cfg.learning_enabled = boolean()?,
                "model_dir" => cfg.model_dir = (!value.is_empty()).then_some(value),
                "learning_path" => cfg.learning_path = (!value.is_empty()).then_some(value),
                "ca_bundle" => cfg.ca_bundle = (!value.is_empty()).then_some(value),
                "parallel_mode" => {
                    if !matches!(value.as_str(),"auto"|"serial"|"parallel"){bail!("parallel_mode: auto|serial|parallel이어야 합니다");}
                    cfg.parallel_mode=value;
                },
                "title_format" => cfg.title_format = value,
                "page_number_format" => {
                    cfg.page_number_format = if value.is_empty() { "#".into() } else { value }
                }
                "comment_separator" => cfg.comment_separator = value,
                "page_download_delay" => cfg.page_download_delay = number()?,
                "hameln_page_download_delay" => cfg.hameln_page_download_delay = number()?,
                "max_retry_count" => {
                    cfg.max_retry_count =
                        number()?.try_into().context("max_retry_count 범위 초과")?;
                    if cfg.max_retry_count > 10 {
                        bail!("max_retry_count는 0~10이어야 합니다");
                    }
                }
                "max_cpu_core_count" => {
                    cfg.max_cpu_core_count = value
                        .parse()
                        .context("max_cpu_core_count는 정수여야 합니다")?;
                    if cfg.max_cpu_core_count == 0 || cfg.max_cpu_core_count < -1 {
                        bail!("max_cpu_core_count는 -1 또는 양수여야 합니다");
                    }
                }
                "max_title_length" => {
                    cfg.max_title_length =
                        number()?.try_into().context("max_title_length 범위 초과")?
                }
                "translator_name" => {
                    cfg.translator_name = match value.as_str() {
                        "dummy" | "passthrough" => "passthrough".into(),
                        "native" | "eztrans" | "internal" => value,
                        _ => bail!("지원하지 않는 translator_name: {value}"),
                    }
                }
                "eztrans_path" => cfg.eztrans_path = (!value.is_empty()).then_some(value),
                "user_agent" => {
                    reqwest::header::HeaderValue::from_str(&value)
                        .context("user_agent 헤더가 올바르지 않습니다")?;
                    cfg.user_agent = value;
                }
                "config_version" => {}
                _ => bail!("알 수 없는 설정: {key}"),
            }
        }
        format_number(1, &cfg.page_number_format)?;
        Ok(cfg)
    }
    pub fn workers(&self) -> usize {
        let available = std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1);
        let maximum = if self.max_cpu_core_count < 0 {
            available.saturating_sub(1).max(1)
        } else {
            self.max_cpu_core_count as usize
        };
        maximum.min(16).min(available).max(1)
    }

    /// Persist all settings using the Java client's UTF-8 Properties format.
    /// Validation happens before callers replace an existing settings file.
    pub fn to_properties(&self) -> Result<String> {
        fn escape(value: &str) -> String {
            let mut out = String::new();
            for (index, ch) in value.chars().enumerate() {
                match ch {
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    '\u{c}' => out.push_str("\\f"),
                    ' ' if index == 0 => out.push_str("\\ "),
                    _ => out.push(ch),
                }
            }
            out
        }
        let mut result = "# Syousetu Translate settings (UTF-8)\nconfig_version=1.7\n".to_owned();
        let fields = [
            ("print_japanese", self.print_japanese.to_string()),
            ("title_format", self.title_format.clone()),
            (
                "filter_title_bracket",
                self.filter_title_bracket.to_string(),
            ),
            ("page_number_format", self.page_number_format.clone()),
            ("page_download_delay", self.page_download_delay.to_string()),
            (
                "hameln_page_download_delay",
                self.hameln_page_download_delay.to_string(),
            ),
            ("max_retry_count", self.max_retry_count.to_string()),
            ("max_cpu_core_count", self.max_cpu_core_count.to_string()),
            ("include_furigana", self.include_furigana.to_string()),
            ("include_comment", self.include_comment.to_string()),
            ("comment_separator", self.comment_separator.clone()),
            ("translator_name", self.translator_name.clone()),
            ("model_dir", self.model_dir.clone().unwrap_or_default()),
            ("learning_path", self.learning_path.clone().unwrap_or_default()),
            ("learning_enabled", self.learning_enabled.to_string()),
            ("parallel_mode", self.parallel_mode.clone()),
            ("ca_bundle", self.ca_bundle.clone().unwrap_or_default()),
            (
                "eztrans_path",
                self.eztrans_path.clone().unwrap_or_default(),
            ),
            ("user_agent", self.user_agent.clone()),
            ("max_title_length", self.max_title_length.to_string()),
            (
                "save_without_file_chooser",
                self.save_without_file_chooser.to_string(),
            ),
        ];
        for (key, value) in fields {
            result.push_str(key);
            result.push('=');
            result.push_str(&escape(&value));
            result.push('\n');
        }
        Self::parse(&result)?;
        Ok(result)
    }
}
pub fn format_number(number: usize, pattern: &str) -> Result<String> {
    let pattern = if pattern.is_empty() { "#" } else { pattern };
    if pattern.len() > 64
        || !pattern.chars().all(|c| matches!(c, '#' | '0' | ','))
        || pattern.starts_with(',')
        || pattern.ends_with(',')
        || pattern.contains(",,")
    {
        bail!("page_number_format은 #, 0, 그룹 구분 쉼표로 된 정수 서식만 지원합니다");
    }
    let width = pattern.chars().filter(|&c| c == '0').count();
    let digits = format!("{number:0width$}");
    if let Some((_, group)) = pattern.rsplit_once(',') {
        let mut result = String::new();
        for (index, digit) in digits.chars().enumerate() {
            if index > 0 && (digits.len() - index).is_multiple_of(group.len()) {
                result.push(',');
            }
            result.push(digit);
        }
        Ok(result)
    } else {
        Ok(digits)
    }
}
fn properties(text: &str) -> Result<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    let mut logical = String::new();
    for physical in text.trim_start_matches('\u{feff}').lines() {
        let line = physical.trim_start_matches([' ', '\t', '\u{c}']);
        if logical.is_empty() && (line.starts_with('#') || line.starts_with('!') || line.is_empty())
        {
            continue;
        }
        logical.push_str(line);
        if logical.chars().rev().take_while(|&c| c == '\\').count() % 2 == 1 {
            logical.pop();
            continue;
        }
        let mut escaped = false;
        let mut split = None;
        for (index, ch) in logical.char_indices() {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == '=' || ch == ':' || ch.is_ascii_whitespace() {
                split = Some(index);
                break;
            }
        }
        let (key, raw) = match split {
            Some(index) => {
                let rest = logical[index..].trim_start_matches([' ', '\t', '\u{c}']);
                let rest = rest
                    .strip_prefix(['=', ':'])
                    .unwrap_or(rest)
                    .trim_start_matches([' ', '\t', '\u{c}']);
                (&logical[..index], rest)
            }
            None => (logical.as_str(), ""),
        };
        result.insert(unescape(key)?, unescape(raw)?);
        logical.clear();
    }
    if !logical.is_empty() {
        bail!("설정 파일의 줄 연속 표시 뒤에 값이 없습니다");
    }
    Ok(result)
}
fn unescape(text: &str) -> Result<String> {
    let mut result = Vec::<u16>::new();
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        let value = if ch != '\\' {
            ch
        } else {
            match chars.next().context("설정 escape가 완성되지 않았습니다")? {
                't' => '\t',
                'n' => '\n',
                'r' => '\r',
                'f' => '\u{c}',
                'u' => {
                    let hex: String = chars.by_ref().take(4).collect();
                    if hex.len() != 4 {
                        bail!("설정 Unicode escape는 4자리여야 합니다");
                    }
                    result.push(
                        u16::from_str_radix(&hex, 16)
                            .context("설정 Unicode escape가 올바르지 않습니다")?,
                    );
                    continue;
                }
                other => other,
            }
        };
        result.extend(value.encode_utf16(&mut [0; 2]).iter().copied());
    }
    String::from_utf16(&result).context("설정 Unicode surrogate가 올바르지 않습니다")
}
#[cfg(test)]
mod tests {
    use super::*;
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
}
