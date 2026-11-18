//! Conservative, source-qualified sentence-ending conversion. Unrecognized
//! morphology stays unchanged; no gender/personality is inferred from names.
use super::Utterance;
use std::borrow::Cow;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Register {
    Keep,
    Narrative,
    Casual,
    Polite,
}
fn classify(utterance: Utterance<'_>, prohibitive: bool) -> Register {
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
        if prohibitive {
            return Register::Casual;
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
                "ちゃう",
                "じゃう",
                "てる",
                "てない",
                "くれない",
                "ないで",
                "ありがとう",
                "ごめん",
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
    if polite || super::alignment::contains_quote(text) {
        Register::Keep
    } else {
        Register::Narrative
    }
}

pub(super) fn contrasting_dialogue(first: Utterance<'_>, second: Utterance<'_>) -> bool {
    let tone = |utterance| classify(utterance, super::japanese::prohibitive(utterance));
    matches!(
        (tone(first), tone(second)),
        (Register::Polite, Register::Casual) | (Register::Casual, Register::Polite)
    )
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
    ("그렇습니다", "그렇다", "그래", "그래요"),
    ("감사합니다", "감사한다", "고마워", "고마워요"),
    ("감사해요", "감사한다", "고마워", "고마워요"),
    ("고맙습니다", "고맙다", "고마워", "고마워요"),
    ("미안합니다", "미안하다", "미안해", "미안해요"),
    ("죄송합니다", "죄송하다", "죄송해", "죄송해요"),
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

pub(super) fn finite_dialogue(text: &str) -> bool {
    let body = text.trim_end_matches(['.', '!', '?', '…', '。', '！', '？']);
    ENDINGS.iter().any(|&(formal, plain, informal, polite)| {
        [formal, plain, polite]
            .iter()
            .any(|ending| body.ends_with(ending))
            || (informal.len() >= "괜찮아".len() && body.ends_with(informal))
    }) || [
        "거야",
        "거예요",
        "겠어요",
        "같아",
        "같아요",
        "됐어",
        "됐어요",
        "지 마",
        "지 마세요",
        "지 마십시오",
        "지 말라",
        "지 말아라",
        "지 않아",
    ]
    .iter()
    .any(|ending| body.ends_with(ending))
}
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
fn sentence(text: &str, register: Register, nominal: bool) -> Option<String> {
    let trimmed = text.trim_end();
    if register == Register::Casual {
        for (previous, corrected) in [("안 되나요", "안 돼"), ("안되나요", "안돼")] {
            if let Some(stem) = trimmed.strip_suffix(previous) {
                return Some(format!("{stem}{corrected}{}", &text[trimmed.len()..]));
            }
        }
    }
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
                    return None;
                };
                return Some(format!("{stem}{ending}{}", &text[trimmed.len()..]));
            }
            return Some(format!(
                "{stem}{}{}",
                match register {
                    Register::Narrative => narrative,
                    Register::Casual => casual,
                    Register::Polite => polite,
                    Register::Keep => old,
                },
                &text[trimmed.len()..]
            ));
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
            return Some(format!(
                "{}{ending}{}",
                stem.trim_end(),
                &text[trimmed.len()..]
            ));
        }
    }
    // Only reviewed Korean endings are reused. In particular, do not strip
    // every 요 or conjugate an arbitrary stem as a verb.
    for &(formal, narrative, casual, polite) in ENDINGS {
        let forms = match register {
            Register::Casual if !matches!(formal, "갑니다" | "옵니다") => [polite, narrative],
            Register::Polite if casual.len() >= "괜찮아".len() => [narrative, casual],
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
                return Some(format!("{stem}{ending}{}", &text[trimmed.len()..]));
            }
        }
    }
    // The finite negative ending is unambiguous; do not conjugate arbitrary
    // 다 endings (which may be nouns or irregular adjectives).
    if matches!(register, Register::Casual | Register::Polite) {
        if let Some(stem) = trimmed.strip_suffix("지 않는다") {
            let ending = if register == Register::Casual {
                "지 않아"
            } else {
                "지 않아요"
            };
            return Some(format!("{stem}{ending}{}", &text[trimmed.len()..]));
        }
    }
    if register == Register::Casual {
        if let Some(stem) = trimmed.strip_suffix("어요") {
            let past = stem.chars().next_back().is_some_and(|ch| {
                ('가'..='힣').contains(&ch) && (ch as u32 - '가' as u32) % 28 == 20
            });
            if past {
                return Some(format!("{stem}어{}", &text[trimmed.len()..]));
            }
        }
    }
    None
}
pub(super) fn render<'a>(utterance: Utterance<'_>, target: &'a str) -> Cow<'a, str> {
    let prohibitive = super::japanese::prohibitive(utterance);
    let register = classify(utterance, prohibitive);
    render_tone(utterance, target, register, prohibitive, true)
}

fn render_tone<'a>(
    utterance: Utterance<'_>,
    target: &'a str,
    register: Register,
    prohibitive: bool,
    first_phrase: bool,
) -> Cow<'a, str> {
    if register == Register::Keep || super::alignment::contains_quote(target) {
        return Cow::Borrowed(target);
    }
    let inner = utterance.inner;
    let tail = inner.trim_end_matches(['。', '！', '？', '!', '?', '…', '.']);
    let nominal = ["です", "だ", "である", "だった", "でした"]
        .iter()
        .any(|ending| tail.trim_end_matches(['よ', 'ね', 'か']).ends_with(ending));
    let negative_request = prohibitive
        || [
            "ないで",
            "ないでね",
            "ないでよ",
            "ないでくれ",
            "すんな",
            "すんなよ",
        ]
        .iter()
        .any(|ending| tail.ends_with(ending));
    let body = target.trim_end_matches(['.', '!', '?', '…', '。', '！', '？']);
    let marks = &target[body.len()..];
    if negative_request {
        let ending = match register {
            Register::Casual => Some("지 마"),
            Register::Polite => Some("지 마세요"),
            _ => None,
        };
        if let Some(ending) = ending {
            for previous in ["지 말아라", "지 말라", "지 마십시오", "지 마세요"] {
                if previous == ending {
                    continue;
                }
                if let Some(stem) = body.strip_suffix(previous) {
                    return Cow::Owned(format!("{stem}{ending}{marks}"));
                }
            }
        }
    }

    let mut text = Cow::Borrowed(body);
    if first_phrase && register == Register::Casual {
        for (source_prefix, previous, corrected) in [
            ("ありがとう、", "감사합니다,", "고마워,"),
            ("ありがとう、", "감사해요,", "고마워,"),
            ("うん、", "그렇습니다,", "응,"),
            ("うん、", "네,", "응,"),
        ] {
            if inner.starts_with(source_prefix) {
                if let Some(tail) = body.strip_prefix(previous) {
                    text = Cow::Owned(format!("{corrected}{tail}"));
                    break;
                }
            }
        }
    }
    if let Some(mut corrected) = sentence(&text, register, nominal) {
        corrected.push_str(marks);
        Cow::Owned(corrected)
    } else if let Cow::Owned(mut corrected) = text {
        corrected.push_str(marks);
        Cow::Owned(corrected)
    } else {
        Cow::Borrowed(target)
    }
}

/// With no sentence correspondence, only apply a shared speech register.
/// Individual source/target clauses are never paired by this path.
pub(super) fn render_uniform<'a>(utterance: Utterance<'_>, target: &'a str) -> Cow<'a, str> {
    let prohibitive = super::japanese::prohibitive(utterance);
    let register = classify(utterance, prohibitive);
    if register == Register::Keep
        || !super::alignment::sentences(utterance.inner).all(|inner| {
            let clause = Utterance { inner, ..utterance };
            let tone = classify(clause, super::japanese::prohibitive(clause));
            tone == register || (tone == Register::Keep && utterance.quotes.is_some())
        })
    {
        return Cow::Borrowed(target);
    }
    if super::alignment::sentences(target).nth(1).is_none() {
        return render_tone(utterance, target, register, prohibitive, true);
    }
    let mut output = String::with_capacity(target.len());
    for (index, part) in super::alignment::sentences(target).enumerate() {
        if !output.is_empty() {
            output.push(' ');
        }
        output.push_str(&render_tone(
            utterance,
            part,
            register,
            prohibitive,
            index == 0,
        ));
    }
    Cow::Owned(output)
}

#[cfg(test)]
mod tests {
    use super::Utterance;
    fn render<'a>(source: &str, target: &'a str) -> std::borrow::Cow<'a, str> {
        super::render(Utterance::parse(source), target)
    }
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
            render("「中に入っちゃダメです。」", "안에 들어가지 말아라."),
            "안에 들어가지 마세요."
        );
        assert_eq!(
            render("「触っちゃダメですよ。」", "터치하지 말라."),
            "터치하지 마세요."
        );
        assert_eq!(
            render("「もう食べちゃだめなの？」", "더 이상 먹어도 안 되나요?"),
            "더 이상 먹어도 안 돼?"
        );
        assert_eq!(
            render("「食べちゃ駄目ですか？」", "먹어도 안 되나요?"),
            "먹어도 안 되나요?"
        );
        assert_eq!(
            render("「もうやってません。」", "이제 하지 않는다."),
            "이제 하지 않아요."
        );
        assert_eq!(
            render("「やってないんだよ。」", "하지 않는다."),
            "하지 않아."
        );
        assert_eq!(render("「ありがとよ！」", "감사합니다!"), "고마워!");
        assert_eq!(
            render(
                "「ありがとう、助かったよ！」",
                "감사합니다, 덕분에 살았습니다!"
            ),
            "고마워, 덕분에 살았어!"
        );
        assert_eq!(
            render("「うん、平気だよ。」", "그렇습니다, 괜찮습니다."),
            "응, 괜찮아."
        );
        assert_eq!(
            render("「ううん、違うんだよ。」", "그렇습니다, 다릅니다."),
            "그렇습니다, 다릅니다."
        );
        assert_eq!(
            render("「無理しないでね。」", "무리하지 말아라."),
            "무리하지 마."
        );
        assert_eq!(
            render(
                "「彼は『来ないで』と言った。」",
                "그는 ‘오지 말아라’라고 말했다."
            ),
            "그는 ‘오지 말아라’라고 말했다."
        );
        assert_eq!(
            render("「来なかったんだよ」", "그분은 오지 않으셨습니다."),
            "그분은 오지 않으셨어."
        );
        assert_eq!(render("「学生です」", "학생입니다."), "학생이에요.");
        assert_eq!(render("「医者です」", "의사입니다."), "의사예요.");
        assert_eq!(render("「友人なんだ」", "친구입니다."), "친구야.");
        assert_eq!(render("重さは1.5 kgだ。", "1.5 kg 입니다."), "1.5 kg이다.");
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
