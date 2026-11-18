//! Conservative preservation of explicit quantities, amounts and game stats.
//! Values use exact fixed-point arithmetic. Only one unmatched value on each
//! side of the same counter/stat anchor can be repaired; ambiguous changes are
//! exposed for diagnostics and left untouched. This is not semantic validation.
use regex::Regex;
use std::{borrow::Cow, collections::BTreeMap, ops::Range, sync::OnceLock};

const SCALE: i128 = 1_000_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    People,
    Yen,
    Won,
    Gold,
    Percent,
    Age,
    Items,
    Times,
    Magnitude,
    Stat(&'static str),
}
struct Number<'a> {
    range: Range<usize>,
    text: &'a str,
    value: i128,
    kind: Kind,
    digits: bool,
    continuation: bool,
}
type Unmatched<'a> = (Vec<&'a Number<'a>>, Vec<&'a Number<'a>>);

fn magnitude(ch: char) -> Option<i128> {
    Some(match ch {
        '十' | '십' => 10,
        '百' | '백' => 100,
        '千' | '천' => 1_000,
        '万' | '萬' | '만' => 10_000,
        '億' | '억' => 100_000_000,
        '兆' | '조' => 1_000_000_000_000,
        _ => return None,
    })
}
fn normalized(text: &str) -> String {
    text.chars()
        .filter_map(|ch| match ch {
            '０'..='９' => char::from_u32(ch as u32 - '０' as u32 + '0' as u32),
            '．' => Some('.'),
            '−' | '－' => Some('-'),
            ',' | '，' | ' ' | '\t' => None,
            _ => Some(ch),
        })
        .collect()
}
fn decimal(text: &str) -> Option<i128> {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if fraction.len() > 9 || whole.is_empty() {
        return None;
    }
    let whole = whole.parse::<i128>().ok()?.checked_mul(SCALE)?;
    let fraction = if fraction.is_empty() {
        0
    } else {
        fraction
            .parse::<i128>()
            .ok()?
            .checked_mul(10i128.pow(9 - fraction.len() as u32))?
    };
    whole.checked_add(fraction)
}
fn value(text: &str) -> Option<i128> {
    let normalized = normalized(text);
    let (sign, text) = if let Some(rest) = normalized.strip_prefix('-') {
        (-1, rest)
    } else {
        (1, normalized.strip_prefix('+').unwrap_or(&normalized))
    };
    let (mut total, mut section, mut start) = (0i128, 0i128, 0usize);
    let mut last_large = i128::MAX;
    for (index, ch) in text.char_indices() {
        let Some(multiplier) = magnitude(ch) else {
            continue;
        };
        let coefficient = if start == index {
            None
        } else {
            Some(decimal(&text[start..index])?)
        };
        if multiplier < 10_000 {
            section = section.checked_add(coefficient.unwrap_or(SCALE).checked_mul(multiplier)?)?;
        } else {
            if multiplier >= last_large {
                return None;
            }
            let coefficient = coefficient.unwrap_or(if section == 0 { SCALE } else { 0 });
            total =
                total.checked_add(section.checked_add(coefficient)?.checked_mul(multiplier)?)?;
            section = 0;
            last_large = multiplier;
        }
        start = index + ch.len_utf8();
    }
    let remainder = if start < text.len() {
        decimal(&text[start..])?
    } else {
        0
    };
    total
        .checked_add(section)?
        .checked_add(remainder)?
        .checked_mul(sign)
}
fn stat(prefix: &str) -> Option<Kind> {
    let prefix = prefix.trim_end_matches(|ch: char| {
        ch.is_whitespace() || matches!(ch, ':' | '：' | '=' | '＝' | '.')
    });
    for (label, key) in [
        ("HP", "hp"),
        ("MP", "mp"),
        ("SP", "sp"),
        ("TP", "tp"),
        ("STR", "str"),
        ("VIT", "vit"),
        ("DEX", "dex"),
        ("AGI", "agi"),
        ("INT", "int"),
        ("LUK", "luk"),
        ("ATK", "atk"),
        ("DEF", "def"),
        ("EXP", "exp"),
        ("LV", "level"),
        ("LEVEL", "level"),
        ("レベル", "level"),
        ("레벨", "level"),
        ("攻撃力", "attack"),
        ("공격력", "attack"),
        ("防御力", "defence"),
        ("방어력", "defence"),
        ("体力", "health"),
        ("체력", "health"),
        ("魔力", "magic"),
        ("마력", "magic"),
        ("経験値", "exp"),
        ("경험치", "exp"),
    ] {
        if let Some(start) = prefix.len().checked_sub(label.len()) {
            if let Some(tail) = prefix.get(start..) {
                if tail.eq_ignore_ascii_case(label)
                    && prefix[..start]
                        .chars()
                        .next_back()
                        .is_none_or(|ch| !ch.is_ascii_alphanumeric())
                {
                    return Some(Kind::Stat(key));
                }
            }
        }
    }
    None
}
fn counter(suffix: &str) -> Option<Kind> {
    let suffix = suffix.trim_start_matches([' ', '\t']);
    for (unit, kind) in [
        ("人", Kind::People),
        ("名", Kind::People),
        ("명", Kind::People),
        ("사람", Kind::People),
        ("円", Kind::Yen),
        ("엔", Kind::Yen),
        ("ウォン", Kind::Won),
        ("원", Kind::Won),
        ("ゴールド", Kind::Gold),
        ("골드", Kind::Gold),
        ("G", Kind::Gold),
        ("%", Kind::Percent),
        ("％", Kind::Percent),
        ("퍼센트", Kind::Percent),
        ("歳", Kind::Age),
        ("才", Kind::Age),
        ("살", Kind::Age),
        ("세", Kind::Age),
        ("個", Kind::Items),
        ("개", Kind::Items),
        ("回", Kind::Times),
        ("회", Kind::Times),
        ("번", Kind::Times),
        ("レベル", Kind::Stat("level")),
        ("레벨", Kind::Stat("level")),
    ] {
        if suffix.starts_with(unit)
            && (unit != "G" || !suffix[1..].chars().next().is_some_and(char::is_alphabetic))
        {
            return Some(kind);
        }
    }
    None
}
fn numbers(text: &str, korean: bool) -> Vec<Number<'_>> {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        let digits = r"[0-9０-９]+(?:[,，][0-9０-９]{3})*(?:[.．][0-9０-９]+)?";
        Regex::new(&format!(
            r"[+\-−－]?{digits}(?:[ \t]*[十百千万萬億兆십백천만억조](?:[ \t]*{digits})?)*"
        ))
        .unwrap()
    });
    let mut result: Vec<Number<'_>> = Vec::new();
    for matched in pattern.find_iter(text) {
        let mut token = matched.as_str().trim_end_matches([' ', '\t']);
        let mut start = matched.start();
        // A range separator is not a negative sign: HP 100-200.
        if token.starts_with(['-', '−', '－'])
            && text[..start]
                .chars()
                .next_back()
                .is_some_and(|ch| ch.is_ascii_digit() || ('０'..='９').contains(&ch))
        {
            let length = token.chars().next().unwrap().len_utf8();
            start += length;
            token = &token[length..];
        }
        let end = start + token.len();
        let Some(value) = value(token) else {
            continue;
        };
        let direct = stat(&text[..start]);
        let inherited = result
            .last()
            .filter(|previous| {
                text[previous.range.end..start]
                    .chars()
                    .all(|ch| ch.is_whitespace() || "/／~〜～→-−－".contains(ch))
            })
            .map(|previous| previous.kind);
        let kind = direct
            .or(inherited)
            .or_else(|| counter(&text[end..]))
            .or_else(|| {
                token
                    .chars()
                    .any(|ch| magnitude(ch).is_some())
                    .then_some(Kind::Magnitude)
            });
        if let Some(kind) = kind {
            result.push(Number {
                range: start..end,
                text: token,
                value,
                kind,
                digits: true,
                continuation: direct.is_none() && inherited.is_some(),
            });
        }
    }
    if korean {
        for (word, count) in [
            ("한", 1),
            ("두", 2),
            ("세", 3),
            ("네", 4),
            ("다섯", 5),
            ("여섯", 6),
            ("일곱", 7),
            ("여덟", 8),
            ("아홉", 9),
            ("열", 10),
            ("열한", 11),
            ("열두", 12),
            ("열세", 13),
            ("열네", 14),
            ("열다섯", 15),
            ("열여섯", 16),
            ("열일곱", 17),
            ("열여덟", 18),
            ("열아홉", 19),
            ("스무", 20),
            ("혼자", 1),
        ] {
            for (start, _) in text.match_indices(word) {
                if text[..start]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_alphanumeric)
                {
                    continue;
                }
                let end = start + word.len();
                if word == "혼자" || counter(&text[end..]) == Some(Kind::People) {
                    result.push(Number {
                        range: start..end,
                        text: word,
                        value: count * SCALE,
                        kind: Kind::People,
                        digits: false,
                        continuation: false,
                    });
                }
            }
        }
    }
    result
}
fn unmatched<'a>(source: &'a [Number<'_>], target: &'a [Number<'_>]) -> Vec<Unmatched<'a>> {
    let mut groups = BTreeMap::<Kind, Unmatched<'_>>::new();
    for number in source {
        groups.entry(number.kind).or_default().0.push(number);
    }
    for number in target {
        if let Some(group) = groups.get_mut(&number.kind) {
            group.1.push(number);
        }
    }
    groups
        .into_iter()
        .map(|(kind, (source, target))| {
            if matches!(kind, Kind::Stat(_))
                && source.len() == target.len()
                && source.iter().skip(1).all(|n| n.continuation)
                && target.iter().skip(1).all(|n| n.continuation)
            {
                // Ratios/current-vs-maximum stats retain their slot ordering.
                return source
                    .into_iter()
                    .zip(target)
                    .filter(|(s, t)| s.value != t.value)
                    .unzip();
            }
            let mut remaining = target.clone();
            let equivalent = source.iter().all(|s| {
                if let Some(index) = remaining.iter().position(|t| t.value == s.value) {
                    remaining.remove(index);
                    true
                } else {
                    false
                }
            }) && remaining.is_empty();
            if equivalent {
                (Vec::new(), Vec::new())
            } else {
                // Multiple people/amounts may have different roles. Cancelling
                // one coincidentally equal value must not authorize a repair.
                (source, target)
            }
        })
        .collect()
}
fn render(target: &Number<'_>, value: i128) -> Option<String> {
    if !target.digits {
        return None;
    }
    let (coefficient, suffix, multiplier) = match target
        .text
        .char_indices()
        .find(|(_, ch)| magnitude(*ch).is_some())
    {
        Some((index, ch)) => {
            if index + ch.len_utf8() != target.text.len() {
                return None;
            }
            let coefficient = target.text[..index].trim_end_matches([' ', '\t']);
            (
                coefficient,
                &target.text[coefficient.len()..],
                magnitude(ch)?,
            )
        }
        None => (target.text, "", 1),
    };
    if value % multiplier != 0 {
        return None;
    }
    let value = value / multiplier;
    let sign = if value < 0 {
        "-"
    } else if coefficient.starts_with('+') {
        "+"
    } else {
        ""
    };
    let value = value.checked_abs()?;
    let mut whole = (value / SCALE).to_string();
    if coefficient.contains([',', '，']) {
        let mut grouped = String::new();
        for (index, ch) in whole.chars().enumerate() {
            if index > 0 && (whole.len() - index).is_multiple_of(3) {
                grouped.push(',');
            }
            grouped.push(ch);
        }
        whole = grouped;
    }
    let minimum = normalized(coefficient)
        .split_once('.')
        .map_or(0, |(_, tail)| tail.len());
    let mut fraction = format!("{:09}", value % SCALE);
    while fraction.ends_with('0') && fraction.len() > minimum {
        fraction.pop();
    }
    let point = if fraction.is_empty() { "" } else { "." };
    Some(format!("{sign}{whole}{point}{fraction}{suffix}"))
}

/// Detect an explicit quantity that was changed, omitted or duplicated in the
/// same recognized anchor. Unsupported word forms may require human review.
#[cfg(test)]
pub fn has_mismatch(source: &str, translated: &str) -> bool {
    let source = numbers(source, false);
    let target = numbers(translated, true);
    unmatched(&source, &target)
        .iter()
        .any(|(s, t)| !s.is_empty() || !t.is_empty())
}

/// Repair only unambiguous numeric substitutions. Ambiguous counts, compound
/// target notation and word-form counts stay unchanged instead of aborting work.
pub fn repair<'a>(source: &str, translated: &'a str) -> Cow<'a, str> {
    let source = numbers(source, false);
    let target = numbers(translated, true);
    let mut replacements = unmatched(&source, &target)
        .into_iter()
        .filter_map(|(s, t)| {
            if let ([source], [target]) = (s.as_slice(), t.as_slice()) {
                render(target, source.value).map(|value| (target.range.clone(), value))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    if replacements.is_empty() {
        return Cow::Borrowed(translated);
    }
    replacements.sort_by_key(|(range, _)| range.start);
    let mut output = translated.to_owned();
    for (range, value) in replacements.into_iter().rev() {
        output.replace_range(range, &value);
    }
    Cow::Owned(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quantities_keep_exact_magnitude_and_korean_particles() {
        assert_eq!(
            repair("2万人が集まった。", "20만 명이 모였다."),
            "2만 명이 모였다."
        );
        assert_eq!(
            repair("２万人を救う", "200,000명을 구한다"),
            "20,000명을 구한다"
        );
        assert_eq!(repair("1.5万円を払う", "15만 엔을 낸다"), "1.5만 엔을 낸다");
        assert_eq!(
            repair("報酬は2000ゴールド", "보상은 20,000골드다"),
            "보상은 2,000골드다"
        );
        assert_eq!(
            repair("成功率は12.5%", "성공률은 15.0%다"),
            "성공률은 12.5%다"
        );
    }
    #[test]
    fn equivalent_reordered_quantities_and_native_counts_remain_unchanged() {
        for (source, translated) in [
            ("2万人", "20,000명"),
            ("2万5000人", "2만 5000명"),
            ("1人", "한 명"),
            ("2人", "두 사람"),
            ("11人", "열한 명"),
            ("1人で", "혼자서"),
            ("100円で2個", "2개를 100엔에"),
        ] {
            assert!(!has_mismatch(source, translated), "{source} / {translated}");
            assert!(matches!(repair(source, translated), Cow::Borrowed(_)));
        }
    }
    #[test]
    fn stat_anchors_preserve_separate_fields_and_ratio_format() {
        assert_eq!(
            repair(
                "HP: 100/200 MP: 50 STR: 12",
                "HP: 100/2000 MP: 500 STR: 120"
            ),
            "HP: 100/200 MP: 50 STR: 12"
        );
        assert_eq!(
            repair("攻撃力: 25 レベル: 3", "레벨: 30 공격력: 250"),
            "레벨: 3 공격력: 25"
        );
        assert_eq!(repair("HP: -10", "HP: +10"), "HP: -10");
        assert_eq!(repair("HP: 100-200", "HP: 100-2000"), "HP: 100-200");
    }
    #[test]
    fn ambiguous_or_missing_values_are_detected_without_guessing_or_aborting() {
        for (source, translated) in [
            ("HP: 100/200", "HP: 300/400"),
            ("HP: 100/200", "HP: 200/300"),
            ("HP: 100/200", "HP: 200/100"),
            ("HP: 100 HP: 200", "HP: 100 HP: 300"),
            ("1人と2人", "2명과 3명"),
            ("2人", "사람"),
            ("3人", "두 명"),
        ] {
            assert!(has_mismatch(source, translated));
            assert_eq!(repair(source, translated), translated);
        }
        assert_eq!(repair("品番123", "품번456"), "품번456");
        assert_eq!(repair("電話0123", "전화4567"), "전화4567");
    }
    #[test]
    fn arithmetic_rejects_overflow_and_never_rounds_values() {
        assert_eq!(value("1億2万3千4"), Some(100_023_004 * SCALE));
        assert_eq!(value("0.000000001"), Some(1));
        assert_eq!(value("1.0000000001"), None);
        assert_eq!(value("9999999999999999999999999999999999999999"), None);
        assert_eq!(repair("0.000000001%", "1万%"), "1万%");
    }
}
