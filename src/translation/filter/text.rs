use std::collections::HashMap;

pub(super) fn hiragana_to_hangul(input: &str) -> String {
    const TABLE:&str="あ아,い이,う우,え에,お오,か카,き키,く쿠,け케,こ코,さ사,し시,す스,せ세,そ소,た타,ち치,つ츠,て테,と토,な나,に니,ぬ누,ね네,の노,は하,ひ히,ふ후,へ헤,ほ호,ま마,み미,む무,め메,も모,や야,ゆ유,よ요,ら라,り리,る루,れ레,ろ로,わ와,を오,んㄴ,が가,ぎ기,ぐ구,げ게,ご고,ざ자,じ지,ず즈,ぜ제,ぞ조,だ다,ぢ지,づ즈,で데,ど도,ば바,び비,ぶ부,べ베,ぼ보,ぱ파,ぴ피,ぷ푸,ぺ페,ぽ포";
    let map: HashMap<char, &str> = TABLE
        .split(',')
        .filter_map(|x| {
            let mut c = x.chars();
            Some((c.next()?, c.as_str()))
        })
        .collect();
    let converted: String = input
        .chars()
        .map(|c| {
            if ('ァ'..='ヶ').contains(&c) {
                char::from_u32(c as u32 - 0x60).unwrap_or(c)
            } else {
                c
            }
        })
        .collect();
    let digraphs = [
        ("きゃ", "캬"),
        ("きゅ", "큐"),
        ("きょ", "쿄"),
        ("しゃ", "샤"),
        ("しゅ", "슈"),
        ("しょ", "쇼"),
        ("ちゃ", "챠"),
        ("ちゅ", "츄"),
        ("ちょ", "쵸"),
        ("にゃ", "냐"),
        ("にゅ", "뉴"),
        ("にょ", "뇨"),
        ("ひゃ", "햐"),
        ("ひゅ", "휴"),
        ("ひょ", "효"),
        ("みゃ", "먀"),
        ("みゅ", "뮤"),
        ("みょ", "묘"),
        ("りゃ", "랴"),
        ("りゅ", "류"),
        ("りょ", "료"),
        ("ぎゃ", "갸"),
        ("ぎゅ", "규"),
        ("ぎょ", "교"),
        ("じゃ", "자"),
        ("じゅ", "주"),
        ("じょ", "조"),
        ("びゃ", "뱌"),
        ("びゅ", "뷰"),
        ("びょ", "뵤"),
        ("ぴゃ", "퍄"),
        ("ぴゅ", "퓨"),
        ("ぴょ", "표"),
    ];
    let mut out = String::new();
    let mut chars = converted.chars().peekable();
    while let Some(c) = chars.next() {
        if c == 'っ' && chars.peek().is_some() {
            if let Some(next) = chars.peek() {
                if let Some(v) = map.get(next) {
                    out.push_str(
                        v.chars()
                            .next()
                            .map(|x| x.to_string())
                            .unwrap_or_default()
                            .as_str(),
                    );
                }
            }
            continue;
        }
        if let Some(next) = chars.peek() {
            let mut pair = String::new();
            pair.push(c);
            pair.push(*next);
            if let Some((_, kor)) = digraphs.iter().find(|(jp, _)| *jp == pair) {
                out.push_str(kor);
                chars.next();
                continue;
            }
        }
        if let Some(kor) = map.get(&c) {
            out.push_str(kor);
        } else {
            out.push(c);
        }
    }
    out
}
pub(super) fn is_japanese(s: &str) -> bool {
    s.chars()
        .any(|c| matches!(c,'\u{3400}'..='\u{9fff}'|'\u{3040}'..='\u{30ff}'))
}
pub(super) fn is_emoji(c: char) -> bool {
    matches!(c,'\u{1F300}'..='\u{1F77F}'|'\u{2600}'..='\u{26FF}'|'\u{2700}'..='\u{27BF}')
}
