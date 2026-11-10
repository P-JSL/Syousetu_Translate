//! Conservative, source-qualified sentence-ending conversion. Unrecognized
//! morphology stays unchanged; no gender/personality is inferred from names.
use super::Utterance;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Register {
    Keep,
    Narrative,
    Casual,
    Polite,
}
fn classify(source: &str) -> Register {
    let utterance = Utterance::parse(source);
    let text = utterance.inner;
    if [
        "ございます",
        "いたします",
        "申します",
        "存じます",
        "であります",
        "ですわ",
        "陛下",
        "閣下",
    ]
    .iter()
    .any(|part| text.contains(part))
    {
        return Register::Keep;
    }
    let tail = text.trim_end_matches(['。', '！', '？', '!', '?', '…', '.']);
    let polite = [tail, tail.trim_end_matches(['か', 'よ', 'ね'])]
        .iter()
        .any(|tail| {
            !tail.ends_with("ますます")
                && ["です", "ます", "でした", "ました", "ください", "ません"]
                    .iter()
                    .any(|part| tail.ends_with(part))
        });
    if utterance.quotes.is_some() {
        if polite {
            return Register::Polite;
        }
        if [
            "だよ",
            "だろ",
            "じゃん",
            "んだ",
            "ってば",
            "だぞ",
            "だぜ",
            "ねえ",
            "かよ",
            "よね",
            "だね",
            "なきゃ",
            "ちゃった",
        ]
        .iter()
        .any(|part| text.contains(part))
            || [
                "よ",
                "ね",
                "ぞ",
                "ぜ",
                "なきゃ",
                "ちゃった",
                "てる",
                "てない",
                "くれない",
                "いけないの",
                "ならないの",
            ]
            .iter()
            .any(|ending| tail.ends_with(ending))
        {
            return Register::Casual;
        }
        return Register::Keep;
    }
    if polite || source.contains(['「', '」', '『', '』', '“', '”', '"']) {
        Register::Keep
    } else {
        Register::Narrative
    }
}

// Ending suffixes are ordered by specificity. These conversions preserve
// lexical content, negation, tense and honorific markers where supported.
const ENDINGS: &[(&str, &str, &str, &str)] = &[
    ("야 합니다", "야 한다", "야 해", "야 해요"),
    ("야 합니까", "야 하는가", "야 해", "야 해요"),
    ("반갑니다", "반갑다", "반가워", "반가워요"),
    ("차갑니다", "차갑다", "차가워", "차가워요"),
    ("셨습니다", "셨다", "셨어", "셨어요"),
    ("했습니다", "했다", "했어", "했어요"),
    ("였습니다", "였다", "였어", "였어요"),
    ("었습니다", "었다", "었어", "었어요"),
    ("았습니다", "았다", "았어", "았어요"),
    ("겠습니까", "겠는가", "겠어", "겠어요"),
    ("있습니까", "있는가", "있어", "있어요"),
    ("없습니까", "없는가", "없어", "없어요"),
    ("했습니까", "했는가", "했어", "했어요"),
    ("합니까", "하는가", "해", "해요"),
    ("아닙니다", "아니다", "아니야", "아니에요"),
    ("모릅니다", "모른다", "몰라", "몰라요"),
    ("압니다", "안다", "알아", "알아요"),
    ("괜찮습니다", "괜찮다", "괜찮아", "괜찮아요"),
    ("무섭습니다", "무섭다", "무서워", "무서워요"),
    ("어렵습니다", "어렵다", "어려워", "어려워요"),
    ("부끄럽습니다", "부끄럽다", "부끄러워", "부끄러워요"),
    ("힘듭니다", "힘들다", "힘들어", "힘들어요"),
    ("아픕니다", "아프다", "아파", "아파요"),
    ("슬픕니다", "슬프다", "슬퍼", "슬퍼요"),
    ("기쁩니다", "기쁘다", "기뻐", "기뻐요"),
    ("좋습니다", "좋다", "좋아", "좋아요"),
    ("싫습니다", "싫다", "싫어", "싫어요"),
    ("싶습니다", "싶다", "싶어", "싶어요"),
    ("있습니다", "있다", "있어", "있어요"),
    ("없습니다", "없다", "없어", "없어요"),
    ("드립니다", "드린다", "드려", "드려요"),
    ("합니다", "한다", "해", "해요"),
    ("됩니다", "된다", "돼", "돼요"),
    ("갑니다", "간다", "가", "가요"),
    ("옵니다", "온다", "와", "와요"),
    ("봅니다", "본다", "봐", "봐요"),
];
fn noun_ending(stem: &str, register: Register) -> Option<&'static str> {
    if register == Register::Narrative {
        return Some("이다");
    }
    let last = stem.trim_end().chars().next_back()?;
    if !('가'..='힣').contains(&last) {
        return None;
    }
    let consonant = !(last as u32 - '가' as u32).is_multiple_of(28);
    Some(match (register, consonant) {
        (Register::Casual, true) => "이야",
        (Register::Casual, false) => "야",
        (Register::Polite, true) => "이에요",
        (Register::Polite, false) => "예요",
        _ => return None,
    })
}
fn sentence(text: &str, register: Register, nominal: bool) -> String {
    let trimmed = text.trim_end();
    for &(old, narrative, casual, polite) in ENDINGS {
        if let Some(stem) = trimmed.strip_suffix(old) {
            if register == Register::Narrative && old == "합니다" {
                let adjective = [
                    "행복",
                    "불행",
                    "미안",
                    "죄송",
                    "중요",
                    "필요",
                    "불필요",
                    "가능",
                    "불가능",
                    "충분",
                    "안전",
                    "위험",
                    "조용",
                    "정확",
                    "확실",
                    "분명",
                    "당연",
                    "특별",
                    "성실",
                    "비슷",
                    "편안",
                    "간단",
                    "복잡",
                    "친절",
                    "불편",
                    "궁금",
                    "이상",
                    "소중",
                ];
                let verb = [
                    "생각",
                    "말",
                    "대답",
                    "이야기",
                    "후회",
                    "사랑",
                    "행동",
                    "시작",
                    "계속",
                    "확인",
                    "판단",
                    "예상",
                    "대화",
                    "노력",
                    "결심",
                    "주장",
                    "설명",
                    "연습",
                    "공부",
                    "운동",
                    "기도",
                    "부탁",
                    "요청",
                    "기억",
                    "약속",
                    "의심",
                    "질문",
                ];
                let ending = if adjective.iter().any(|word| stem.ends_with(word)) {
                    "하다"
                } else if verb.iter().any(|word| stem.ends_with(word)) {
                    "한다"
                } else {
                    return text.to_owned();
                };
                return format!("{stem}{ending}{}", &text[trimmed.len()..]);
            }
            return format!(
                "{stem}{}{}",
                match register {
                    Register::Narrative => narrative,
                    Register::Casual => casual,
                    Register::Polite => polite,
                    Register::Keep => old,
                },
                &text[trimmed.len()..]
            );
        }
    }
    if let Some(stem) = trimmed.strip_suffix("입니다") {
        // Verbs such as 보이다/먹이다 also end in 입니다. Their morphology
        // cannot safely be treated as the noun copula.
        let verb = ["보", "모", "쌓", "먹", "속", "죽", "쓰", "움직"]
            .iter()
            .any(|ending| stem.ends_with(ending));
        if let Some(ending) = (nominal && !verb)
            .then(|| noun_ending(stem, register))
            .flatten()
        {
            return format!("{}{ending}{}", stem.trim_end(), &text[trimmed.len()..]);
        }
    }
    // Only reviewed Korean endings are reused. In particular, do not strip
    // every 요 or conjugate an arbitrary stem as a verb.
    for &(formal, narrative, casual, polite) in ENDINGS {
        let forms = match register {
            Register::Casual if !matches!(formal, "갑니다" | "옵니다") => [polite, narrative],
            Register::Polite => [narrative, narrative],
            _ => continue,
        };
        for old in forms {
            if let Some(stem) = trimmed.strip_suffix(old) {
                let ending = if register == Register::Casual {
                    casual
                } else {
                    polite
                };
                return format!("{stem}{ending}{}", &text[trimmed.len()..]);
            }
        }
    }
    if register == Register::Casual {
        if let Some(stem) = trimmed.strip_suffix("어요") {
            let past = stem.chars().next_back().is_some_and(|ch| {
                ('가'..='힣').contains(&ch) && (ch as u32 - '가' as u32) % 28 == 20
            });
            if past {
                return format!("{stem}어{}", &text[trimmed.len()..]);
            }
        }
    }
    text.to_owned()
}
pub(super) fn render(source: &str, target: &str) -> String {
    let register = classify(source);
    if register == Register::Keep || target.contains(['“', '”', '‘', '’', '「', '」', '『', '』'])
    {
        return target.to_owned();
    }
    let inner = Utterance::parse(source).inner;
    let tail = inner.trim_end_matches(['。', '！', '？', '!', '?', '…', '.', 'よ', 'ね', 'か']);
    let nominal = ["です", "だ", "である", "だった", "でした"]
        .iter()
        .any(|ending| tail.ends_with(ending));
    let softened = (register == Register::Casual
        && ["しないで", "すんな"]
            .iter()
            .any(|part| source.contains(part)))
    .then(|| target.replace("지 말라", "지 마"));
    let target = softened.as_deref().unwrap_or(target);
    let mut result = String::with_capacity(target.len());
    let mut start = 0;
    for (index, ch) in target.char_indices() {
        if matches!(ch, '.' | '!' | '?' | '…' | '。' | '！' | '？') {
            result.push_str(&sentence(&target[start..index], register, nominal));
            result.push(ch);
            start = index + ch.len_utf8();
        }
    }
    result.push_str(&sentence(&target[start..], register, nominal));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn narration_casual_dialogue_and_polite_dialogue_keep_distinct_registers() {
        assert_eq!(
            render("彼は扉を開けた。", "그는 문을 열었습니다."),
            "그는 문을 열었다."
        );
        assert_eq!(
            render("「俺だって怖いんだよ」", "나도 무섭습니다."),
            "나도 무서워."
        );
        assert_eq!(render("「怖いです」", "저도 무섭습니다."), "저도 무서워요.");
        assert_eq!(
            render("「陛下に申し上げます」", "폐하께 아룁니다."),
            "폐하께 아룁니다."
        );
        assert_eq!(
            render("彼は「はい」と答えた。", "그는 ‘네’라고 대답했습니다."),
            "그는 ‘네’라고 대답했습니다."
        );
    }
    #[test]
    fn tense_negation_honorifics_and_noun_consonants_are_preserved() {
        assert_eq!(
            render("「来なかったんだよ」", "그분은 오지 않으셨습니다."),
            "그분은 오지 않으셨어."
        );
        assert_eq!(render("「学生です」", "학생입니다."), "학생이에요.");
        assert_eq!(render("「医者です」", "의사입니다."), "의사예요.");
        assert_eq!(render("「友人なんだ」", "친구입니다."), "친구야.");
        assert_eq!(
            render("重さは1.5 kgだ。", "1.5 kg 입니다. 값은 3.14입니다."),
            "1.5 kg이다. 값은 3.14이다."
        );
        assert_eq!(render("「てめえ」", "너를 찾습니다."), "너를 찾습니다.");
        assert_eq!(render("彼は幸せだ。", "그는 행복합니다."), "그는 행복하다.");
        assert_eq!(render("彼は考えた。", "그는 생각합니다."), "그는 생각한다.");
        assert_eq!(render("冷たい。", "손이 차갑니다."), "손이 차갑다.");
        assert_eq!(render("嬉しい。", "정말 반갑니다."), "정말 반갑다.");
        assert_eq!(
            render("「ますます怖いんだよ」", "점점 무섭습니다."),
            "점점 무서워."
        );
        assert_eq!(render("「怖いですよね？」", "무섭습니다?"), "무서워요?");
        assert_eq!(render("よく見える。", "잘 보입니다."), "잘 보입니다.");
        assert_eq!(
            render("「知ってるんだよ」", "알고 있었어요!"),
            "알고 있었어!"
        );
        assert_eq!(
            render("「本当に嬉しいです」", "정말 기쁘다."),
            "정말 기뻐요."
        );
        assert_eq!(
            render("「ねえ、そんな顔しないでよ。」", "그런 얼굴을 하지 말라."),
            "그런 얼굴을 하지 마."
        );
    }
}
