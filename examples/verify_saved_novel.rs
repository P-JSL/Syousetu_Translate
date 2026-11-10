//! Offline structural verification of default-format Kakuyomu bilingual TXT.
//! Prints only counts/checksums. It does not certify semantic translation quality.
use anyhow::{ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};
use syousetu_translate::novel::site::{parse_episode, Episode, Site};

fn verify(episodes: &[Episode], text: &str) -> Result<(usize, usize, usize)> {
    let lines = text.lines().collect::<Vec<_>>();
    let pattern = regex::Regex::new(r"^\[(\d+)/(\d+)\] (.*)$")?;
    let markers = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| pattern.captures(line).map(|caps| (index, caps)))
        .collect::<Vec<_>>();
    ensure!(markers.len() == episodes.len(), "chapter count mismatch");
    let mut counts = (0, 0, 0);
    for (index, episode) in episodes.iter().enumerate() {
        let (start, marker) = &markers[index];
        ensure!(
            marker[1].parse::<usize>()? == index + 1
                && marker[2].parse::<usize>()? == episodes.len(),
            "chapter order/total mismatch at {}",
            index + 1
        );
        ensure!(
            marker[3].ends_with(&format!(" / {}", episode.title)),
            "chapter title mismatch at {}",
            index + 1
        );
        ensure!(lines.get(start + 1) == Some(&""), "missing title separator");
        let end = markers
            .get(index + 1)
            .map_or(lines.len(), |(offset, _)| *offset);
        let body = lines.get(start + 2..end).context("missing chapter body")?;
        let mut cursor = 0;
        for (row, source) in episode.lines.iter().enumerate() {
            if source.trim().is_empty() {
                ensure!(
                    body.get(cursor).is_some_and(|line| line.trim().is_empty()),
                    "missing paragraph break at chapter {}, row {}",
                    index + 1,
                    row + 1
                );
                cursor += 1;
                counts.0 += 1;
                continue;
            }
            ensure!(
                body.get(cursor) == Some(&source.as_str()),
                "original mismatch at chapter {}, row {}",
                index + 1,
                row + 1
            );
            let translation = body.get(cursor + 1).context("missing translated row")?;
            cursor += 2;
            counts.0 += 1;
            if !source.trim().is_empty() {
                ensure!(
                    !translation.trim().is_empty(),
                    "empty translation at chapter {}, row {}",
                    index + 1,
                    row + 1
                );
                counts.1 += 1;
                counts.2 += usize::from(translation.chars().any(|ch| ('가'..='힣').contains(&ch)));
            }
        }
        ensure!(
            body[cursor..].iter().all(|line| line.trim().is_empty()),
            "extra chapter body lines"
        );
    }
    Ok(counts)
}

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    ensure!(
        args.len() == 3,
        "usage: verify_saved_novel SOURCE_HTML_DIR BILINGUAL_TXT CHAPTERS"
    );
    let chapters: usize = args[2].parse()?;
    ensure!(chapters > 0, "CHAPTERS must be positive");
    let episodes = (1..=chapters)
        .map(|number| {
            parse_episode(
                Site::Kakuyomu,
                &fs::read_to_string(Path::new(&args[0]).join(format!("{number}.html")))?,
                false,
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let bytes = fs::read(&args[1])?;
    let text = std::str::from_utf8(&bytes)?;
    let (source_lines, nonempty_source_lines, korean_lines) = verify(&episodes, text)?;
    println!(
        "{}",
        serde_json::json!({
            "complete":true, "semantic_accuracy_certified":false,
            "chapters":chapters, "bytes":bytes.len(), "sha256":format!("{:x}",Sha256::digest(&bytes)),
            "source_lines":source_lines, "nonempty_source_lines":nonempty_source_lines, "korean_lines":korean_lines
        })
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn episodes() -> [Episode; 2] {
        [
            Episode {
                title: "一".into(),
                lines: vec!["原文".into(), "".into()],
            },
            Episode {
                title: "二".into(),
                lines: vec!["次".into()],
            },
        ]
    }
    const SAVED: &str =
        "metadata\n\n[1/2] 첫째 / 一\n\n原文\n원문\n\n\n\n\n[2/2] 둘째 / 二\n\n次\n다음\n";
    #[test]
    fn all_original_pairs_and_chapter_order_are_required() {
        assert_eq!(verify(&episodes(), SAVED).unwrap(), (3, 2, 2));
        for invalid in [
            SAVED.replace("原文\n", "변경\n"),
            SAVED.replace("원문\n", "\n"),
            SAVED.replace("[2/2]", "[1/2]"),
            SAVED.replace("다음\n", "다음\nEXTRA\n"),
            SAVED.replace("次\n다음\n", ""),
        ] {
            assert!(verify(&episodes(), &invalid).is_err());
        }
    }
}
