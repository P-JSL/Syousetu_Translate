//! Correct a mistranslated leading family subject only with explicit evidence.
//! Ambiguous 兄/姉 and sentences mentioning multiple relatives are excluded.
use super::Utterance;

struct Relation {
    source: &'static str,
    target: &'static str,
    accepted: &'static [&'static str],
}
const RELATIONS: &[Relation] = &[
    Relation {
        source: "祖父",
        target: "할아버지",
        accepted: &["할아버지", "조부"],
    },
    Relation {
        source: "祖母",
        target: "할머니",
        accepted: &["할머니", "조모"],
    },
    Relation {
        source: "父",
        target: "아버지",
        accepted: &["아버지", "아빠", "부친"],
    },
    Relation {
        source: "母",
        target: "어머니",
        accepted: &["어머니", "엄마", "모친"],
    },
    Relation {
        source: "弟",
        target: "남동생",
        accepted: &["남동생", "동생"],
    },
    Relation {
        source: "妹",
        target: "여동생",
        accepted: &["여동생", "동생"],
    },
    Relation {
        source: "息子",
        target: "아들",
        accepted: &["아들"],
    },
];
pub(super) fn restore(utterance: Utterance<'_>, generated: &str) -> Option<String> {
    let (relation, rest) = RELATIONS.iter().find_map(|relation| {
        let rest = utterance.inner.strip_prefix(relation.source)?;
        rest.strip_prefix(['は', 'が', 'も'])
            .map(|rest| (relation, rest))
    })?;
    if rest.contains(['「', '『', '“', '‘'])
        || RELATIONS
            .iter()
            .any(|relation| rest.contains(relation.source))
        || rest.contains(['兄', '姉', '娘'])
        || rest.contains(['夫', '妻'])
        || relation
            .accepted
            .iter()
            .any(|word| generated.contains(word))
    {
        return None;
    }
    // Only a known family noun followed by a complete subject particle and a
    // word boundary qualifies. Objects, names and free noun substrings do not.
    let (prefix, generated) = ["내 ", "제 ", "우리 "]
        .iter()
        .find_map(|prefix| generated.strip_prefix(prefix).map(|rest| (*prefix, rest)))
        .unwrap_or(("", generated));
    let nouns = RELATIONS
        .iter()
        .flat_map(|relation| relation.accepted.iter().copied())
        .chain(["형", "오빠", "누나", "언니", "아내", "남편", "여자", "남자"]);
    for noun in nouns {
        let Some(rest) = generated.strip_prefix(noun) else {
            continue;
        };
        let Some(particle) = rest.chars().next() else {
            continue;
        };
        if !matches!(particle, '은' | '는' | '이' | '가' | '도') {
            continue;
        }
        let rest = &rest[particle.len_utf8()..];
        if !rest.starts_with(char::is_whitespace) && !rest.starts_with(',') {
            continue;
        }
        if relation.accepted.contains(&noun) {
            return None;
        }
        let last = relation.target.chars().next_back()?;
        let consonant = !(last as u32 - '가' as u32).is_multiple_of(28);
        let particle = match particle {
            '은' | '는' if consonant => '은',
            '은' | '는' => '는',
            '이' | '가' if consonant => '이',
            '이' | '가' => '가',
            other => other,
        };
        return Some(format!("{prefix}{}{particle}{rest}", relation.target));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_family_subjects_keep_their_relationship_and_particle() {
        for relation in RELATIONS {
            let wrong = if relation.source == "父" {
                "어머니"
            } else {
                "아빠"
            };
            for (source_particle, target_particle) in [('は', '는'), ('が', '가'), ('も', '도')]
            {
                let source = format!("「{}{source_particle}外にいる。」", relation.source);
                let raw = format!("{wrong}{target_particle} 밖에 있어.");
                let fixed = restore(Utterance::parse(&source), &raw).unwrap();
                assert!(fixed.starts_with(relation.target));
                assert!(fixed.ends_with(" 밖에 있어."));
            }
        }
        assert_eq!(
            restore(
                Utterance::parse("「弟は走ってるよ。」"),
                "아빠는 달리고 있어."
            )
            .unwrap(),
            "남동생은 달리고 있어."
        );
        assert_eq!(
            restore(Utterance::parse("妹が待つ。"), "오빠가 기다린다.").unwrap(),
            "여동생이 기다린다."
        );
        assert_eq!(
            restore(
                Utterance::parse("「妹は待ってるよ。」"),
                "내 아내는 기다리고 있어."
            )
            .unwrap(),
            "내 여동생은 기다리고 있어."
        );
        assert_eq!(
            restore(Utterance::parse("「妹も読んだよ。」"), "여자도 읽었어.").unwrap(),
            "여동생도 읽었어."
        );
    }
    #[test]
    fn ambiguity_other_relatives_quoted_words_and_objects_are_left_alone() {
        for (source, raw) in [
            ("「兄は外にいる。」", "언니는 밖에 있어."),
            ("「姉は外にいる。」", "누나는 밖에 있어."),
            ("「娘は外にいる。」", "어머니는 밖에 있어."),
            ("「弟子は外にいる。」", "아빠는 밖에 있어."),
            ("「弟は父と話してる。」", "아빠는 동생과 얘기해."),
            ("「父は弟を呼んだ。」", "남동생이 아빠를 불렀어."),
            ("「弟は『父』と言った。」", "아빠는 ‘아버지’라고 말했어."),
            ("「私の弟は外にいる。」", "아빠는 밖에 있어."),
            ("「弟は外にいる。」", "아빠에게 물어봐."),
            ("「弟は外にいる。」", "아빠가게에 갔어."),
            ("「弟は外にいる。」", "동생은 밖에 있어."),
            ("「父は外にいる。」", "아빠는 밖에 있어."),
            ("「弟は男だ。」", "남자는 동생이다."),
            ("「弟は妻と外にいる。」", "아빠는 아내와 밖에 있어."),
        ] {
            assert!(restore(Utterance::parse(source), raw).is_none(), "{source}");
        }
    }
}
