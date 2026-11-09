pub(super) fn choose_josa(last: Option<char>, suffix: &str) -> &str {
    let syllable = last.unwrap_or(' ');
    let jong = if ('가'..='힣').contains(&syllable) {
        (syllable as u32 - 0xAC00) % 28
    } else {
        0
    };
    match suffix {
        "은" | "는" => {
            if jong != 0 {
                "은"
            } else {
                "는"
            }
        }
        "이" | "가" => {
            if jong != 0 {
                "이"
            } else {
                "가"
            }
        }
        "을" | "를" => {
            if jong != 0 {
                "을"
            } else {
                "를"
            }
        }
        "과" | "와" => {
            if jong != 0 {
                "과"
            } else {
                "와"
            }
        }
        "로" | "으로" => {
            if jong == 0 || jong == 8 {
                "로"
            } else {
                "으로"
            }
        }
        "야" | "아" => {
            if jong != 0 {
                "아"
            } else {
                "야"
            }
        }
        "이야" => {
            if jong != 0 {
                "이야"
            } else {
                "야"
            }
        }
        _ => suffix,
    }
}
pub(super) fn boundary(c: Option<char>) -> bool {
    c.is_none_or(|c| c.is_whitespace() || ".,!?;:)]}'\"…".contains(c))
}
pub(super) fn resolve_token(replacement: &str, remaining: &str) -> (String, usize) {
    let (remaining, marker_len) = if let Some(rest) = remaining.strip_prefix("#N") {
        (rest, 2)
    } else {
        (remaining, 0)
    };
    let candidates = [
        "이야", "으로", "은", "는", "이", "가", "을", "를", "과", "와", "로", "야", "아",
    ];
    for candidate in candidates {
        if let Some(rest) = remaining.strip_prefix(candidate) {
            if boundary(rest.chars().next()) {
                return (
                    format!(
                        "{}{}",
                        replacement,
                        choose_josa(replacement.chars().last(), candidate)
                    ),
                    candidate.len() + marker_len,
                );
            }
        }
    }
    (replacement.to_owned(), marker_len)
}
