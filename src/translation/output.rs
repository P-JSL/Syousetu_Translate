use crate::{
    config::{format_number, Config},
    novel::site::{Episode, Work},
    translation::session::Session,
};
use anyhow::Result;
use std::{io::Write, sync::LazyLock};
static TITLE_TOKENS: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"\[currentPage\]|\[lastPage\]|\[koreanTitle\]|\[japaneseTitle\]")
        .expect("static title tokens")
});
static TITLE_PREFIX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^【[^】]+】").expect("static title prefix"));
static INVALID_FILENAME: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r#"[\\/:*?"<>|]|\[[^\]]*\]"#).expect("static filename rule")
});
pub use crate::storage::atomic_write;
mod chapters;
pub use chapters::write_episodes;

pub fn format_episode(
    session: &mut Session,
    episode: &Episode,
    number: usize,
    total: usize,
    config: &Config,
) -> Result<String> {
    let mut output = Vec::new();
    write_episode(&mut output, session, episode, number, total, config)?;
    Ok(String::from_utf8(output)?)
}
pub fn write_episode(
    output: &mut dyn Write,
    session: &mut Session,
    episode: &Episode,
    number: usize,
    total: usize,
    config: &Config,
) -> Result<()> {
    let ko_title = session.translate(&episode.title)?;
    let title = chapter_title(&ko_title, &episode.title, number, total, config)?;
    write!(output, "\n\n{title}\n\n")?;
    output.flush()?;
    session.translate_lines_into(&episode.lines, &mut |index, translation| {
        // A paragraph break has no second-language content. Emit it once,
        // while keeping both source and translation for nonblank rows.
        if config.print_japanese && !episode.lines[index].trim().is_empty() {
            writeln!(output, "{}", episode.lines[index])?;
        }
        writeln!(output, "{translation}")?;
        if !translation.trim().is_empty() {
            output.flush()?;
        }
        Ok(())
    })?;
    output.flush()?;
    Ok(())
}
fn chapter_title(
    korean: &str,
    japanese: &str,
    number: usize,
    total: usize,
    config: &Config,
) -> Result<String> {
    let current = format_number(number, &config.page_number_format)?;
    let last = format_number(total, &config.page_number_format)?;
    Ok(TITLE_TOKENS
        .replace_all(&config.title_format, |caps: &regex::Captures<'_>| {
            match &caps[0] {
                "[currentPage]" => current.as_str(),
                "[lastPage]" => last.as_str(),
                "[koreanTitle]" => korean,
                _ => japanese,
            }
            .to_owned()
        })
        .into_owned())
}
pub fn format_header(session: &mut Session, work: &Work) -> Result<String> {
    let title = session.translate(&work.title)?;
    let description = session
        .translate_lines(
            &work
                .description
                .lines()
                .map(str::to_owned)
                .collect::<Vec<_>>(),
        )?
        .join("\n");
    Ok(format!(
        "{title}\n{}\n{}\n\n{description}\n\n",
        work.title, work.url
    ))
}
pub fn suggested_filename(title: &str, config: &Config) -> Result<String> {
    let without_prefix = if config.filter_title_bracket {
        TITLE_PREFIX.replace(title, "").into_owned()
    } else {
        title.to_owned()
    };
    let sanitized = INVALID_FILENAME.replace_all(&without_prefix, "");
    let name: String = sanitized
        .chars()
        .filter(|c| !c.is_control())
        .take(config.max_title_length)
        .collect();
    let name = name.trim().trim_end_matches('.').trim();
    let mut name = if name.is_empty() {
        "novel".to_owned()
    } else {
        name.to_owned()
    };
    let base = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    if ["CON", "PRN", "AUX", "NUL"].contains(&base.as_str())
        || (base.len() == 4
            && (base.starts_with("COM") || base.starts_with("LPT"))
            && matches!(base.as_bytes()[3], b'1'..=b'9'))
    {
        name.insert(0, '_');
    }
    Ok(format!("{name}.txt"))
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn writes_temp_then_rename() {
        let path = std::env::temp_dir().join(format!("syousetu-test-{}.txt", std::process::id()));
        atomic_write(&path, "안녕\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "안녕\n");
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn missing_translated_lines_are_rejected() {
        struct Incomplete;
        impl crate::translation::engine::Translator for Incomplete {
            fn translate(&mut self, _: &str) -> Result<String> {
                Ok("제목".into())
            }
            fn translate_lines(&mut self, _: &[String]) -> Result<Vec<String>> {
                Ok(vec!["한 줄만".into()])
            }
        }
        let mut session = Session::direct(Box::new(Incomplete));
        let episode = Episode {
            title: "章".into(),
            lines: vec!["一".into(), "二".into()],
        };
        assert!(format_episode(&mut session, &episode, 1, 1, &Config::default()).is_err());
    }
    #[test]
    fn bilingual_paragraph_break_is_written_once_without_merging_dialogue() {
        let mut session = Session::direct(Box::new(crate::translation::engine::Passthrough));
        let episode = Episode {
            title: "章".into(),
            lines: vec!["「一」".into(), "".into(), "「二」".into()],
        };
        let config = Config {
            print_japanese: true,
            ..Config::default()
        };
        let output = format_episode(&mut session, &episode, 1, 1, &config).unwrap();
        assert!(output.ends_with("「一」\n「一」\n\n「二」\n「二」\n"));
    }
}
