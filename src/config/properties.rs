use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
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
pub(super) fn properties(text: &str) -> Result<BTreeMap<String, String>> {
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
pub(super) fn unescape(text: &str) -> Result<String> {
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
