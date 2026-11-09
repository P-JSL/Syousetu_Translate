//! Source-visible Java Properties settings, with explicit CLI compatibility.
mod properties;
use anyhow::{bail, Context, Result};
pub use properties::format_number;
use properties::properties;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

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
    pub download_dir: Option<String>,
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
            download_dir: None,
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
    pub fn download_directory(&self) -> std::path::PathBuf {
        self.download_dir
            .as_ref()
            .filter(|s| !s.trim().is_empty())
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                if cfg!(windows) {
                    std::path::PathBuf::from(r"C:\Users\rabbit\OneDrive\문서")
                } else {
                    std::path::PathBuf::from(".")
                }
            })
    }
    pub fn model_directory(&self) -> std::path::PathBuf {
        self.model_dir
            .as_ref()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(crate::models::default_dir)
    }
    pub fn memory_file(&self) -> std::path::PathBuf {
        self.learning_path
            .as_ref()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(crate::learning::default_path)
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
                "download_dir" => cfg.download_dir = (!value.is_empty()).then_some(value),
                "learning_path" => cfg.learning_path = (!value.is_empty()).then_some(value),
                "ca_bundle" => cfg.ca_bundle = (!value.is_empty()).then_some(value),
                "parallel_mode" => {
                    if !matches!(value.as_str(), "auto" | "serial" | "parallel") {
                        bail!("parallel_mode: auto|serial|parallel이어야 합니다");
                    }
                    cfg.parallel_mode = value;
                }
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
            (
                "download_dir",
                self.download_dir.clone().unwrap_or_default(),
            ),
            (
                "learning_path",
                self.learning_path.clone().unwrap_or_default(),
            ),
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

#[cfg(test)]
mod tests;
