//! Independent Japanese-to-Korean, deterministic glossary translator.
//!
//! This is a native Rust implementation, not a reproduction of the proprietary
//! ezTrans morphological and transfer-rule engine. Unknown Japanese is retained
//! instead of being silently presented as successfully translated Korean.
use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};
use anyhow::{bail, Result};
use std::{collections::HashMap, path::Path};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    Word,
    Phrase,
    Particle,
    Ending,
}

#[derive(Debug)]
struct Entry {
    source: String,
    target: String,
    kind: Kind,
}

/// Compiled glossary. Leftmost-longest search is O(input bytes + matches),
/// after a one-time dictionary-build cost. Translation quality is NOT
/// equivalent to ezTrans, nor is the whole application O(n).
pub struct NativeTranslator {
    entries: Vec<Entry>,
    matcher: AhoCorasick,
    strict: bool,
}

const BUILTIN: &[(&str, &str, Kind)] = &[
    ("こんにちは", "안녕하세요", Kind::Phrase),
    ("こんばんは", "안녕하세요", Kind::Phrase),
    ("おはようございます", "좋은 아침입니다", Kind::Phrase),
    ("ありがとうございます", "감사합니다", Kind::Phrase),
    ("ありがとう", "고마워", Kind::Phrase),
    ("すみません", "죄송합니다", Kind::Phrase),
    ("ごめんなさい", "미안합니다", Kind::Phrase),
    ("さようなら", "안녕히 가세요", Kind::Phrase),
    ("いただきます", "잘 먹겠습니다", Kind::Phrase),
    ("おやすみなさい", "안녕히 주무세요", Kind::Phrase),
    ("はい", "네", Kind::Phrase),
    ("いいえ", "아니요", Kind::Phrase),
    ("私", "나", Kind::Word),
    ("僕", "나", Kind::Word),
    ("俺", "나", Kind::Word),
    ("彼女", "그녀", Kind::Word),
    ("彼", "그", Kind::Word),
    ("あなた", "당신", Kind::Word),
    ("君", "너", Kind::Word),
    ("皆さん", "여러분", Kind::Word),
    ("みんな", "모두", Kind::Word),
    ("私たち", "우리", Kind::Word),
    ("これ", "이것", Kind::Word),
    ("それ", "그것", Kind::Word),
    ("あれ", "저것", Kind::Word),
    ("ここ", "여기", Kind::Word),
    ("そこ", "거기", Kind::Word),
    ("あそこ", "저기", Kind::Word),
    ("今日", "오늘", Kind::Word),
    ("明日", "내일", Kind::Word),
    ("昨日", "어제", Kind::Word),
    ("今", "지금", Kind::Word),
    ("朝", "아침", Kind::Word),
    ("昼", "낮", Kind::Word),
    ("夜", "밤", Kind::Word),
    ("時間", "시간", Kind::Word),
    ("世界", "세계", Kind::Word),
    ("異世界", "이세계", Kind::Word),
    ("魔法", "마법", Kind::Word),
    ("魔王", "마왕", Kind::Word),
    ("勇者", "용사", Kind::Word),
    ("冒険者", "모험가", Kind::Word),
    ("冒険", "모험", Kind::Word),
    ("王国", "왕국", Kind::Word),
    ("王女", "왕녀", Kind::Word),
    ("王子", "왕자", Kind::Word),
    ("騎士", "기사", Kind::Word),
    ("少女", "소녀", Kind::Word),
    ("少年", "소년", Kind::Word),
    ("人間", "인간", Kind::Word),
    ("友達", "친구", Kind::Word),
    ("家族", "가족", Kind::Word),
    ("父", "아버지", Kind::Word),
    ("母", "어머니", Kind::Word),
    ("名前", "이름", Kind::Word),
    ("学校", "학교", Kind::Word),
    ("学生", "학생", Kind::Word),
    ("先生", "선생님", Kind::Word),
    ("本", "책", Kind::Word),
    ("小説", "소설", Kind::Word),
    ("物語", "이야기", Kind::Word),
    ("言葉", "말", Kind::Word),
    ("日本語", "일본어", Kind::Word),
    ("韓国語", "한국어", Kind::Word),
    ("翻訳", "번역", Kind::Word),
    ("文章", "문장", Kind::Word),
    ("猫", "고양이", Kind::Word),
    ("犬", "개", Kind::Word),
    ("鳥", "새", Kind::Word),
    ("花", "꽃", Kind::Word),
    ("空", "하늘", Kind::Word),
    ("海", "바다", Kind::Word),
    ("山", "산", Kind::Word),
    ("水", "물", Kind::Word),
    ("火", "불", Kind::Word),
    ("風", "바람", Kind::Word),
    ("雨", "비", Kind::Word),
    ("雪", "눈", Kind::Word),
    ("天気", "날씨", Kind::Word),
    ("家", "집", Kind::Word),
    ("街", "거리", Kind::Word),
    ("町", "마을", Kind::Word),
    ("道", "길", Kind::Word),
    ("部屋", "방", Kind::Word),
    ("扉", "문", Kind::Word),
    ("窓", "창문", Kind::Word),
    ("食事", "식사", Kind::Word),
    ("食べ物", "음식", Kind::Word),
    ("仕事", "일", Kind::Word),
    ("力", "힘", Kind::Word),
    ("心", "마음", Kind::Word),
    ("夢", "꿈", Kind::Word),
    ("愛", "사랑", Kind::Word),
    ("敵", "적", Kind::Word),
    ("味方", "아군", Kind::Word),
    ("武器", "무기", Kind::Word),
    ("剣", "검", Kind::Word),
    ("戦争", "전쟁", Kind::Word),
    ("戦い", "싸움", Kind::Word),
    ("勝利", "승리", Kind::Word),
    ("敗北", "패배", Kind::Word),
    ("死", "죽음", Kind::Word),
    ("生", "삶", Kind::Word),
    ("新しい", "새로운", Kind::Word),
    ("古い", "오래된", Kind::Word),
    ("大きい", "큰", Kind::Word),
    ("小さい", "작은", Kind::Word),
    ("良い", "좋은", Kind::Word),
    ("いい", "좋은", Kind::Word),
    ("悪い", "나쁜", Kind::Word),
    ("美しい", "아름다운", Kind::Word),
    ("強い", "강한", Kind::Word),
    ("弱い", "약한", Kind::Word),
    ("見る", "보다", Kind::Word),
    ("聞く", "듣다", Kind::Word),
    ("読む", "읽다", Kind::Word),
    ("書く", "쓰다", Kind::Word),
    ("話す", "말하다", Kind::Word),
    ("行く", "가다", Kind::Word),
    ("来る", "오다", Kind::Word),
    ("帰る", "돌아가다", Kind::Word),
    ("食べる", "먹다", Kind::Word),
    ("飲む", "마시다", Kind::Word),
    ("思う", "생각하다", Kind::Word),
    ("知る", "알다", Kind::Word),
    ("分かる", "이해하다", Kind::Word),
    ("である", "이다", Kind::Ending),
    ("でした", "이었습니다", Kind::Ending),
    ("です", "입니다", Kind::Ending),
    ("だった", "이었다", Kind::Ending),
    ("だ", "이다", Kind::Ending),
    ("ました", "했습니다", Kind::Ending),
    ("ます", "합니다", Kind::Ending),
    ("は", "는", Kind::Particle),
    ("が", "가", Kind::Particle),
    ("を", "를", Kind::Particle),
    ("も", "도", Kind::Particle),
    ("の", "의", Kind::Particle),
    ("に", "에", Kind::Particle),
    ("で", "에서", Kind::Particle),
    ("と", "와", Kind::Particle),
    ("へ", "에", Kind::Particle),
    ("から", "부터", Kind::Particle),
    ("まで", "까지", Kind::Particle),
];

fn contains_japanese(input: &str) -> bool {
    input.chars().any(|c| {
        matches!(c,
        '\u{3040}'..='\u{30ff}' | '\u{3400}'..='\u{9fff}')
    })
}

fn last_hangul(input: &str) -> Option<char> {
    input
        .chars()
        .rev()
        .find(|c| !c.is_whitespace())
        .filter(|c| ('가'..='힣').contains(c))
}

fn particle_suffix(previous: Option<char>, suffix: &str) -> &str {
    let jong = previous.map(|c| (c as u32 - 0xac00) % 28).unwrap_or(0);
    match suffix {
        "는" => {
            if jong == 0 {
                "는"
            } else {
                "은"
            }
        }
        "가" => {
            if jong == 0 {
                "가"
            } else {
                "이"
            }
        }
        "를" => {
            if jong == 0 {
                "를"
            } else {
                "을"
            }
        }
        "와" => {
            if jong == 0 {
                "와"
            } else {
                "과"
            }
        }
        _ => suffix,
    }
}

impl NativeTranslator {
    /// Optional TSV glossary has Japanese then Korean text in its first two
    /// columns. Extra columns in user-owned legacy Ehnd dictionaries are ignored.
    pub fn new(dictionary: Option<&Path>, strict: bool) -> Result<Self> {
        let mut entries: Vec<Entry> = Vec::new();
        let mut keys: HashMap<String, usize> = HashMap::new();
        for &(source, target, kind) in BUILTIN {
            insert(&mut entries, &mut keys, source, target, kind);
        }
        for &(source, target) in crate::morph::CONJUGATED {
            insert(&mut entries, &mut keys, source, target, Kind::Word);
        }
        if let Some(path) = dictionary {
            if path.is_dir() {
                // EHND's text dictionaries are available in the supplied
                // local files. This loads lexical pairs only, NOT the original
                // user's priority flags, POS constraints, or grammar behavior.
                let dir = if path.join("Ehnd").is_dir() {
                    path.join("Ehnd")
                } else {
                    path.to_path_buf()
                };
                for filename in [
                    "UserDict_@Hdor#0#req.txt",
                    "UserDict_@Hdor#1.txt",
                    "UserDict_@Hdor#name.txt",
                ] {
                    let file = dir.join(filename);
                    for (source, target) in crate::dictionary::read(&file)? {
                        if source.contains('#')
                            || target.contains('#')
                            || source.contains('二')
                            || target.contains('二')
                        {
                            // EHND's internal markers are not plain glossary
                            // surface forms. Do not fabricate their semantics.
                            continue;
                        }
                        insert(&mut entries, &mut keys, &source, &target, Kind::Word);
                    }
                }
            } else {
                for (source, target) in crate::dictionary::read(path)? {
                    insert(&mut entries, &mut keys, &source, &target, Kind::Word);
                }
            }
        }
        entries.sort_unstable_by(|a, b| a.source.cmp(&b.source));
        let matcher = AhoCorasickBuilder::new()
            .match_kind(MatchKind::LeftmostLongest)
            .build(entries.iter().map(|e| e.source.as_str()))?;
        Ok(Self {
            entries,
            matcher,
            strict,
        })
    }

    pub fn translate(&self, input: &str) -> Result<String> {
        let mut output = String::with_capacity(input.len());
        let mut end = 0;
        let mut spaced_after_particle = false;
        for hit in self.matcher.find_iter(input) {
            let unknown = &input[end..hit.start()];
            if !unknown.is_empty() {
                if spaced_after_particle
                    && !unknown.starts_with(char::is_whitespace)
                    && !unknown.starts_with(|c: char| c.is_ascii_punctuation())
                {
                    output.push(' ');
                }
                output.push_str(unknown);
                spaced_after_particle = false;
            }
            let entry = &self.entries[hit.pattern().as_usize()];
            match entry.kind {
                Kind::Particle => {
                    output.push_str(particle_suffix(last_hangul(&output), &entry.target));
                    spaced_after_particle = true;
                }
                Kind::Ending => {
                    output.push_str(&entry.target);
                    spaced_after_particle = false;
                }
                Kind::Word | Kind::Phrase => {
                    if spaced_after_particle && !output.ends_with(char::is_whitespace) {
                        output.push(' ');
                    }
                    output.push_str(&entry.target);
                    spaced_after_particle = entry.kind == Kind::Phrase;
                }
            }
            end = hit.end();
        }
        output.push_str(&input[end..]);
        if self.strict && contains_japanese(&output) {
            bail!("네이티브 사전에 없는 일본어가 남았습니다. --native-dictionary로 보완하거나 --native-strict를 해제하세요");
        }
        Ok(output)
    }
}

fn insert(
    entries: &mut Vec<Entry>,
    keys: &mut HashMap<String, usize>,
    source: &str,
    target: &str,
    kind: Kind,
) {
    if let Some(&idx) = keys.get(source) {
        entries[idx].target = target.to_owned();
        entries[idx].kind = kind;
    } else {
        keys.insert(source.to_owned(), entries.len());
        entries.push(Entry {
            source: source.to_owned(),
            target: target.to_owned(),
            kind,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn basic_sentences_and_particles() {
        let t = NativeTranslator::new(None, false).unwrap();
        assert_eq!(t.translate("私は学生です。").unwrap(), "나는 학생입니다。");
        assert_eq!(
            t.translate("彼は猫を見る。").unwrap(),
            "그는 고양이를 보다。"
        );
        assert_eq!(t.translate("学校が").unwrap(), "학교가");
        assert_eq!(t.translate("本が").unwrap(), "책이");
        assert_eq!(t.translate("こんにちは！").unwrap(), "안녕하세요！");
    }
    #[test]
    fn selected_verb_conjugations_avoid_fragmented_suffixes() {
        let t = NativeTranslator::new(None, false).unwrap();
        assert_eq!(t.translate("本を読んだ。").unwrap(), "책을 읽었다。");
        assert_eq!(
            t.translate("水を飲みました。").unwrap(),
            "물을 마셨습니다。"
        );
        assert_eq!(
            t.translate("学校に行かなかった。").unwrap(),
            "학교에 가지 않았다。"
        );
        assert_eq!(t.translate("猫を見た。").unwrap(), "고양이를 봤다。");
    }
    #[test]
    fn unknown_text_is_not_fabricated() {
        let t = NativeTranslator::new(None, false).unwrap();
        assert_eq!(t.translate("未知語").unwrap(), "未知語");
        assert_eq!(t.translate("😀").unwrap(), "😀");
        assert!(NativeTranslator::new(None, true)
            .unwrap()
            .translate("未知語")
            .is_err());
    }
    #[test]
    fn can_load_local_ehnd_lexicon_without_vendor_engine() {
        use std::fs;
        let dir = std::env::temp_dir().join(format!("native-ehnd-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        for filename in [
            "UserDict_@Hdor#0#req.txt",
            "UserDict_@Hdor#1.txt",
            "UserDict_@Hdor#name.txt",
        ] {
            fs::write(dir.join(filename), "クロムウェル\t크롬웰#N\t1\tVHn\n").unwrap();
        }
        let translator = NativeTranslator::new(Some(&dir), false).unwrap();
        assert_eq!(
            translator.translate("クロムウェルは学生です。").unwrap(),
            "크롬웰은 학생입니다。"
        );
        for filename in [
            "UserDict_@Hdor#0#req.txt",
            "UserDict_@Hdor#1.txt",
            "UserDict_@Hdor#name.txt",
        ] {
            fs::remove_file(dir.join(filename)).unwrap();
        }
        fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn long_input_remains_bounded_and_unmodified_ascii() {
        let t = NativeTranslator::new(None, false).unwrap();
        let input = "ABC😀".repeat(12_000);
        assert_eq!(t.translate(&input).unwrap(), input);
    }
}
