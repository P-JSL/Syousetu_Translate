//! A bounded source window can refill inference across chapter boundaries.
//! Only the completed original prefix is written, including translated titles.
use super::{chapter_title, Config, Episode, Session};
use anyhow::{Context, Result};
use std::io::Write;
struct Span {
    heading: usize,
    end: usize,
    number: usize,
}

pub fn write_episodes(
    output: &mut dyn Write,
    session: &mut Session,
    episodes: &[Episode],
    first: usize,
    total: usize,
    config: &Config,
    completed: &mut dyn FnMut(usize, &Episode) -> Result<()>,
) -> Result<()> {
    if episodes.is_empty() {
        return Ok(());
    }
    let mut sources = Vec::new();
    let mut spans = Vec::with_capacity(episodes.len());
    for (offset, episode) in episodes.iter().enumerate() {
        let heading = sources.len();
        sources.push(episode.title.clone());
        sources.extend(episode.lines.iter().cloned());
        spans.push(Span {
            heading,
            end: sources.len(),
            number: first
                .checked_add(offset)
                .context("chapter number overflow")?,
        });
    }
    let mut chapter = 0;
    session.translate_lines_into(&sources, &mut |index, translated| {
        let span = spans
            .get(chapter)
            .context("chapter output exceeds source window")?;
        let episode = &episodes[chapter];
        if index == span.heading {
            let title = chapter_title(translated, &episode.title, span.number, total, config)?;
            write!(output, "\n\n{title}\n\n")?;
            output.flush()?;
        } else {
            if config.print_japanese && !sources[index].trim().is_empty() {
                writeln!(output, "{}", sources[index])?;
            }
            writeln!(output, "{translated}")?;
            if !translated.trim().is_empty() {
                output.flush()?;
            }
        }
        if index + 1 == span.end {
            output.flush()?;
            completed(span.number, episode)?;
            chapter += 1;
        }
        Ok(())
    })?;
    anyhow::ensure!(chapter == episodes.len(), "incomplete chapter window");
    output.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::translation::engine::Translator;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    struct Echo {
        rows: Arc<AtomicUsize>,
        fail_after: Option<usize>,
    }
    impl Translator for Echo {
        fn translate(&mut self, source: &str) -> Result<String> {
            Ok(format!("KO:{source}"))
        }
        fn translate_lines_into(
            &mut self,
            lines: &[String],
            emit: &mut dyn FnMut(usize, &str) -> Result<()>,
        ) -> Result<()> {
            self.rows.store(lines.len(), Ordering::Relaxed);
            for (index, source) in lines.iter().enumerate() {
                anyhow::ensure!(
                    Some(index) != self.fail_after,
                    "injected translation failure"
                );
                emit(
                    index,
                    &if source.is_empty() {
                        String::new()
                    } else {
                        self.translate(source)?
                    },
                )?;
            }
            Ok(())
        }
    }
    #[test]
    fn titles_paragraphs_and_completion_events_keep_original_chapter_order() {
        let rows = Arc::new(AtomicUsize::new(0));
        let mut session = Session::direct(Box::new(Echo {
            rows: Arc::clone(&rows),
            fail_after: None,
        }));
        let episodes = [
            Episode {
                title: "章一".into(),
                lines: vec!["本文一".into(), "".into(), "本文二".into()],
            },
            Episode {
                title: "章二".into(),
                lines: vec![],
            },
        ];
        let config = Config {
            print_japanese: true,
            ..Config::default()
        };
        let mut output = Vec::new();
        let mut done = Vec::new();
        write_episodes(
            &mut output,
            &mut session,
            &episodes,
            2,
            3,
            &config,
            &mut |number, _| {
                done.push(number);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(rows.load(Ordering::Relaxed), 5);
        assert_eq!(done, [2, 3]);
        assert_eq!(String::from_utf8(output).unwrap(), "\n\n[2/3] KO:章一 / 章一\n\n本文一\nKO:本文一\n\n本文二\nKO:本文二\n\n\n[3/3] KO:章二 / 章二\n\n");
    }
    #[test]
    fn failure_preserves_only_completed_source_prefix_and_reports_completed_chapters() {
        let rows = Arc::new(AtomicUsize::new(0));
        let mut session = Session::direct(Box::new(Echo {
            rows,
            fail_after: Some(2),
        }));
        let episodes = [
            Episode {
                title: "一".into(),
                lines: vec!["本文".into()],
            },
            Episode {
                title: "二".into(),
                lines: vec!["続き".into()],
            },
        ];
        let mut output = Vec::new();
        let mut done = Vec::new();
        assert!(write_episodes(
            &mut output,
            &mut session,
            &episodes,
            1,
            2,
            &Config::default(),
            &mut |n, _| {
                done.push(n);
                Ok(())
            }
        )
        .is_err());
        assert_eq!(done, [1]);
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "\n\n[1/2] KO:一 / 一\n\nKO:本文\n"
        );
    }
}
