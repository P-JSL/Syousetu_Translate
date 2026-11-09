//! Read-only EHND filter definition reader and opt-in, *partial* executor.
//!
//! The user-provided EHND files have literal (flag 0), regex (flag 1),
//! and nontrivial directives (_TGL, _skip_sub, _SkipLayer).
//! This module preserves the exact fields of every source line and can
//! execute standalone literal rules. It DOES NOT implement the original
//! engine's conditional stage graph, .NET regex semantics, or user dictionary
//! injection. Only use apply_literal for explicit exploratory comparisons.
use anyhow::{bail, Context, Result};
use encoding_rs::{UTF_16BE, UTF_16LE};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

const MAX_FILTER_BYTES: usize = 20_000_000;
const MAX_OUTPUT_BYTES: usize = 8_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum Phase {
    Pre,
    Post,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum RuleType {
    Literal,
    Regex,
    Directive,
    Unknown,
}
#[derive(Clone, Debug)]
pub struct Rule {
    pub phase: Phase,
    pub origin: String,
    pub line: usize,
    pub raw: String,
    pub fields: Vec<String>,
    pub kind: RuleType,
    pub priority: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Default)]
pub struct RuleCounts {
    pub raw_lines: usize,
    pub comments_or_blanks: usize,
    pub literals: usize,
    pub regexes: usize,
    pub directives: usize,
    pub unknown: usize,
    pub literal_empty_search: usize,
}
#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub directory: String,
    pub counts: RuleCounts,
    pub all_rules_parsed: bool,
    pub all_rules_executable: bool,
    pub actual_engine_semantics_preserved: bool,
    pub config: Vec<(String, String)>,
    pub supporting_files: Vec<SupportingFile>,
    pub warnings: Vec<&'static str>,
}
#[derive(Clone, Debug, Serialize)]
pub struct SupportingFile {
    pub filename: String,
    pub present: bool,
    pub raw_lines: usize,
    pub active_lines: usize,
}
pub struct Ehnd {
    directory: PathBuf,
    pub rules: Vec<Rule>,
    pub counts: RuleCounts,
    pub config: Vec<(String, String)>,
    pub supporting_files: Vec<SupportingFile>,
}
fn read_text(path: &Path) -> Result<String> {
    let bytes =
        fs::read(path).with_context(|| format!("EHND 파일 읽기 실패: {}", path.display()))?;
    if bytes.len() > MAX_FILTER_BYTES {
        bail!("EHND 규칙 파일 크기가 제한을 초과함: {}", path.display());
    }
    if bytes.starts_with(&[0xff, 0xfe]) {
        let (value, _, bad) = UTF_16LE.decode(&bytes[2..]);
        if bad {
            bail!("EHND UTF-16LE 디코딩 오류: {}", path.display());
        }
        Ok(value.into_owned())
    } else if bytes.starts_with(&[0xfe, 0xff]) {
        let (value, _, bad) = UTF_16BE.decode(&bytes[2..]);
        if bad {
            bail!("EHND UTF-16BE 디코딩 오류: {}", path.display());
        }
        Ok(value.into_owned())
    } else {
        String::from_utf8(bytes)
            .map(|x| x.trim_start_matches('\u{feff}').to_owned())
            .with_context(|| format!("EHND UTF-8 디코딩 오류: {}", path.display()))
    }
}
fn classify(fields: &[String]) -> (RuleType, Option<u64>) {
    // Known control keywords take precedence over numeric metadata. Other
    // patterns may start with '_' and remain ordinary replacement rules.
    if fields
        .first()
        .is_some_and(|s| matches!(s.as_str(), "_TGL" | "_skip_sub" | "_SkipLayer"))
    {
        return (RuleType::Directive, None);
    }
    if fields.len() >= 4 && !fields[0].is_empty() {
        if let Ok(priority) = fields[2].parse::<u64>() {
            return match fields[3].as_str() {
                "0" => (RuleType::Literal, Some(priority)),
                "1" => (RuleType::Regex, Some(priority)),
                _ => (RuleType::Unknown, Some(priority)),
            };
        }
    }
    (RuleType::Unknown, None)
}
fn parse_config(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('[') {
                return None;
            }
            let (key, value) = trimmed.split_once('=')?;
            if key.trim().is_empty() {
                return None;
            }
            Some((key.trim().to_ascii_uppercase(), value.trim().to_owned()))
        })
        .collect()
}
impl Ehnd {
    /// Supports a directory containing Ehnd/ or Ehnd itself.
    /// Only reads files; does not load the original DLL or modify definitions.
    pub fn load(dir: &Path) -> Result<Self> {
        let directory = if dir.join("Ehnd").is_dir() {
            dir.join("Ehnd")
        } else {
            dir.to_owned()
        };
        let config = parse_config(&read_text(&directory.join("ehnd_conf.ini"))?);
        let files = [
            (Phase::Pre, "PreFilter_@Hdor.txt"),
            (Phase::Pre, "PreFilter_@Hdor_C.txt"),
            (Phase::Post, "PostFilter_@Hdor.txt"),
            (Phase::Post, "PostFilter_@Hdor_C.txt"),
        ];
        let mut counts = RuleCounts::default();
        let mut rules = Vec::new();
        for (phase, filename) in files {
            let contents = read_text(&directory.join(filename))?;
            for (index, raw) in contents.lines().enumerate() {
                counts.raw_lines += 1;
                let trimmed = raw.trim_end_matches('\r');
                if trimmed.trim().is_empty() || trimmed.trim_start().starts_with("//") {
                    counts.comments_or_blanks += 1;
                    continue;
                }
                let fields = trimmed.split('\t').map(str::to_owned).collect::<Vec<_>>();
                let (kind, priority) = classify(&fields);
                match kind {
                    RuleType::Literal => {
                        counts.literals += 1;
                        if fields[0].is_empty() {
                            counts.literal_empty_search += 1;
                        }
                    }
                    RuleType::Regex => counts.regexes += 1,
                    RuleType::Directive => counts.directives += 1,
                    RuleType::Unknown => counts.unknown += 1,
                }
                rules.push(Rule {
                    phase,
                    origin: filename.to_owned(),
                    line: index + 1,
                    raw: trimmed.to_owned(),
                    fields,
                    kind,
                    priority,
                });
            }
        }
        // Supporting files have separate grammar: preserve their presence
        // and line counts. Never treat user dictionary columns or SkipLayer
        // patterns as ordinary global search-and-replace rules.
        let mut supporting_files = Vec::new();
        for filename in [
            "SkipLayer_@Hdor.txt",
            "PreFilter_@1Hdor_Set.txt",
            "UserDict_@Hdor#0#req.txt",
            "UserDict_@Hdor#1.txt",
            "UserDict_@Hdor#name.txt",
        ] {
            let path = directory.join(filename);
            if path.is_file() {
                let contents = read_text(&path)?;
                let raw_lines = contents.lines().count();
                let active_lines = contents
                    .lines()
                    .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with("//"))
                    .count();
                supporting_files.push(SupportingFile {
                    filename: filename.to_owned(),
                    present: true,
                    raw_lines,
                    active_lines,
                });
            } else {
                supporting_files.push(SupportingFile {
                    filename: filename.to_owned(),
                    present: false,
                    raw_lines: 0,
                    active_lines: 0,
                });
            }
        }
        Ok(Self {
            directory,
            rules,
            counts,
            config,
            supporting_files,
        })
    }
    pub fn report(&self) -> Report {
        Report{
            directory:self.directory.to_string_lossy().into_owned(),
            counts:self.counts.clone(),
            all_rules_parsed:true,
            all_rules_executable:false,
            actual_engine_semantics_preserved:false,
            config:self.config.clone(),
            supporting_files:self.supporting_files.clone(),
            warnings:vec![
                "Only literal replacement rules can run; .NET regex and EHND directives are not interpreted",
                "Priority is sorted numerically, preserving original order on ties; original engine stage ordering has not been independently verified",
                "UserDict_@Hdor and SkipLayer_@Hdor rules are not injected into the proprietary translation engine",
                "Applying literal-only filters to an unrelated translation engine may change text incorrectly",
            ],
        }
    }
    /// Explicitly incomplete literal-only evaluation for comparative testing.
    /// Does not claim production fidelity, and does not mutate source files.
    pub fn apply_literal(&self, phase: Phase, input: &str) -> Result<String> {
        if input.len() > MAX_OUTPUT_BYTES {
            bail!("EHND 입력 길이 제한 초과");
        }
        let key = match phase {
            Phase::Pre => "PREFILTER_SWITCH",
            Phase::Post => "POSTFILTER_SWITCH",
        };
        if self
            .config
            .iter()
            .any(|(name, value)| name == key && value.eq_ignore_ascii_case("OFF"))
        {
            return Ok(input.to_owned());
        }
        let mut output = input.to_owned();
        let mut items = self
            .rules
            .iter()
            .enumerate()
            .filter(|(_, r)| r.phase == phase && r.kind == RuleType::Literal)
            .collect::<Vec<_>>();
        // Preserve relative source order among identical priorities.
        items.sort_by_key(|(ordinal, rule)| (rule.priority.unwrap_or(u64::MAX), *ordinal));
        for (_, rule) in items {
            let search = &rule.fields[0];
            if search.is_empty() || !output.contains(search) {
                continue;
            }
            let replaced = output.replace(search, &rule.fields[1]);
            if replaced.len() > MAX_OUTPUT_BYTES {
                bail!("EHND 출력 길이 제한 초과: {}:{}", rule.origin, rule.line);
            }
            output = replaced;
        }
        Ok(output)
    }
    pub fn source_rule(&self, phase: Phase, filename: &str, line: usize) -> Option<&Rule> {
        self.rules
            .iter()
            .find(|r| r.phase == phase && r.origin == filename && r.line == line)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn underscore_patterns_are_not_control_keywords() {
        use super::*;
        for (pattern, flag, expected) in [
            ("_ㄹㅡㄴ∬해", "0", RuleType::Literal),
            ("_prefix(?=suffix)", "1", RuleType::Regex),
            ("_TGL_extra", "0", RuleType::Literal),
        ] {
            let fields = vec![
                pattern.into(),
                "replacement".into(),
                "10".into(),
                flag.into(),
            ];
            assert_eq!(classify(&fields), (expected, Some(10)));
        }
        for keyword in ["_TGL", "_skip_sub", "_SkipLayer"] {
            let fields = vec![
                keyword.into(),
                "replacement".into(),
                "10".into(),
                "0".into(),
            ];
            assert_eq!(classify(&fields), (RuleType::Directive, None));
        }
    }
    use super::*;
    #[test]
    fn classify_literals_regex_and_directives() {
        let fields = |x: &str| x.split('\t').map(str::to_owned).collect::<Vec<_>>();
        assert_eq!(classify(&fields("a\tb\t11110100\t0")).0, RuleType::Literal);
        assert_eq!(classify(&fields("a+\tb\t11110100\t1")).0, RuleType::Regex);
        assert_eq!(
            classify(&fields("_TGL\t_ON:x\tregex\trep\t11110100\t1")).0,
            RuleType::Directive
        );
        assert_eq!(classify(&fields("_skip_sub\tdata")).0, RuleType::Directive);
        // Exact control keywords take precedence over numeric rule metadata.
        assert_eq!(
            classify(&fields("_SkipLayer\tvalue\t100\t1")),
            (RuleType::Directive, None)
        );
        // An ordinary regex may start with '_' without becoming a directive.
        assert_eq!(
            classify(&fields("_bad\tvalue\t100\t1")),
            (RuleType::Regex, Some(100))
        );
    }
    #[test]
    fn config_switches() {
        let config =
            parse_config("// hey\n[CONFIG]\nPREFILTER_SWITCH = ON\nPOSTFILTER_SWITCH=OFF\n");
        assert_eq!(
            config,
            vec![
                ("PREFILTER_SWITCH".to_owned(), "ON".to_owned()),
                ("POSTFILTER_SWITCH".to_owned(), "OFF".to_owned()),
            ]
        );
    }
    #[test]
    fn load_and_apply_only_explicit_literals() {
        let dir = std::env::temp_dir().join(format!("ehnd-fixture-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("ehnd_conf.ini"),
            b"[CONFIG]\nPREFILTER_SWITCH=ON\n",
        )
        .unwrap();
        for (name, contents) in [
            (
                "PreFilter_@Hdor.txt",
                "A\tB\t100\t0\nC+\tD\t200\t1\n_TGL\t_OFF\n",
            ),
            ("PreFilter_@Hdor_C.txt", "B\tC\t150\t0\n"),
            ("PostFilter_@Hdor.txt", "C\tOK\t1\t0\n"),
            ("PostFilter_@Hdor_C.txt", ""),
        ] {
            fs::write(dir.join(name), contents).unwrap();
        }
        let ehnd = Ehnd::load(&dir).unwrap();
        assert_eq!(ehnd.counts.literals, 3);
        assert_eq!(ehnd.counts.regexes, 1);
        assert_eq!(ehnd.counts.directives, 1);
        assert!(!ehnd.report().actual_engine_semantics_preserved);
        assert_eq!(ehnd.apply_literal(Phase::Pre, "A C+").unwrap(), "C C+");
        assert_eq!(ehnd.apply_literal(Phase::Post, "C").unwrap(), "OK");
        fs::write(
            dir.join("ehnd_conf.ini"),
            b"[CONFIG]\nPREFILTER_SWITCH=OFF\nPOSTFILTER_SWITCH=OFF\n",
        )
        .unwrap();
        let disabled = Ehnd::load(&dir).unwrap();
        assert_eq!(disabled.apply_literal(Phase::Pre, "A").unwrap(), "A");
        assert_eq!(disabled.apply_literal(Phase::Post, "C").unwrap(), "C");
        for name in [
            "ehnd_conf.ini",
            "PreFilter_@Hdor.txt",
            "PreFilter_@Hdor_C.txt",
            "PostFilter_@Hdor.txt",
            "PostFilter_@Hdor_C.txt",
        ] {
            fs::remove_file(dir.join(name)).unwrap();
        }
        fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn real_ehnd_files_if_available() {
        let Some(directory) = std::env::var_os("SYOUSETU_EHND_DIR") else {
            return;
        };
        let set = Ehnd::load(Path::new(&directory)).unwrap();
        assert_eq!(set.counts.raw_lines, 37_870);
        assert_eq!(set.counts.literals, 8_668);
        assert_eq!(set.counts.regexes, 8_597);
        assert_eq!(set.counts.directives, 3_173);
        assert_eq!(set.counts.unknown, 7);
        assert!(set.supporting_files.iter().all(|file| file.present));
        assert!(!set.report().actual_engine_semantics_preserved);
    }
    #[test]
    fn utf16_ini_decoder() {
        let path = std::env::temp_dir().join(format!("ehnd-utf16-{}.txt", std::process::id()));
        let mut data = vec![0xff, 0xfe];
        for unit in "설정=ON".encode_utf16() {
            data.extend(unit.to_le_bytes());
        }
        fs::write(&path, data).unwrap();
        assert_eq!(read_text(&path).unwrap(), "설정=ON");
        fs::remove_file(path).unwrap();
    }
}
