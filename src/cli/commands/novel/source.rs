//! One rate-limited loader feeds bounded, ready-only chapter windows. Network
//! requests remain serial; inference can run while the next chapter downloads.
use crate::{
    cli::args::{flag, value},
    config::Config,
    novel::{
        network::Fetcher,
        site::{parse_episode_with_comments, Episode, Work},
    },
    translation::output,
};
use anyhow::{Context, Result};
use std::{
    fs,
    ops::RangeInclusive,
    path::Path,
    sync::mpsc::{self, Receiver, TryRecvError},
    thread::Scope,
};
mod buffer;
use buffer::{Consumed, Loaded};
type Message = Result<Loaded>;
pub(super) type Source = Receiver<Message>;
pub(super) const CHAPTER_LIMIT: usize = 64;
// Stop after this many parsed rows, or one unusually long chapter. Ready-only
// collection never waits for the limit and bounds queued source text, not KV.
const ROW_LIMIT: usize = 8192;
const BYTE_LIMIT: usize = 8 * 1024 * 1024;
fn load(
    args: &[String],
    config: &Config,
    fetcher: &Fetcher,
    work: &Work,
    number: usize,
) -> Result<Episode> {
    let file =
        value(args, "--episode-html-dir").map(|dir| Path::new(&dir).join(format!("{number}.html")));
    let page = match file {
        Some(file) => fs::read_to_string(&file)
            .with_context(|| format!("에피소드 HTML 없음: {}", file.display()))?,
        None => {
            if !flag(args, "--no-delay") {
                fetcher.rest();
            }
            fetcher.get(&work.episodes[number - 1])?
        }
    };
    if let Some(directory) = value(args, "--cache-html-dir") {
        output::atomic_write(&Path::new(&directory).join(format!("{number}.html")), &page)?;
    }
    parse_episode_with_comments(
        work.site,
        &page,
        config.include_furigana,
        config.include_comment,
        &config.comment_separator,
    )
    .with_context(|| format!("에피소드 {number} 파싱 실패: {}", work.episodes[number - 1]))
}
pub(super) fn prefetch<'scope, 'env>(
    scope: &'scope Scope<'scope, 'env>,
    args: &'scope [String],
    config: &'scope Config,
    fetcher: Fetcher,
    work: &'scope Work,
    range: RangeInclusive<usize>,
) -> Source {
    let (send, receive) = mpsc::sync_channel(CHAPTER_LIMIT);
    scope.spawn(move || {
        for number in range {
            let (result, wait) = match load(args, config, &fetcher, work, number) {
                Ok(episode) => {
                    let (loaded, wait) = Loaded::new(number, episode);
                    (Ok(loaded), wait)
                }
                Err(error) => (Err(error), None),
            };
            let failed = result.is_err();
            if send.send(result).is_err() || failed {
                break;
            }
            if let Some(wait) = wait {
                if wait.recv().is_err() {
                    break;
                }
            }
        }
    });
    receive
}
pub(super) struct Window {
    pub first: usize,
    pub episodes: Vec<Episode>,
    _permits: Vec<Consumed>,
}
pub(super) struct Reader {
    receive: Source,
    next: usize,
    end: usize,
    limit: usize,
    deferred: Option<anyhow::Error>,
}
impl Reader {
    pub(super) fn new(receive: Source, first: usize, end: usize, limit: usize) -> Self {
        Self {
            receive,
            next: first,
            end,
            limit: limit.clamp(1, CHAPTER_LIMIT),
            deferred: None,
        }
    }
    pub(super) fn next_window(&mut self) -> Result<Option<Window>> {
        if let Some(error) = self.deferred.take() {
            return Err(error);
        }
        if self.next > self.end {
            return Ok(None);
        }
        let mut message = self
            .receive
            .recv()
            .with_context(|| format!("에피소드 {} 로더 연결 종료", self.next))?;
        let mut window = Window {
            first: self.next,
            episodes: Vec::new(),
            _permits: Vec::new(),
        };
        let mut rows = 0;
        let mut bytes = 0;
        loop {
            match message.and_then(|loaded| {
                anyhow::ensure!(loaded.number == self.next, "에피소드 로더 순서 오류");
                Ok(loaded)
            }) {
                Ok(Loaded {
                    episode,
                    bytes: size,
                    permit,
                    ..
                }) => {
                    rows += episode.lines.len() + 1;
                    bytes += size;
                    window.episodes.push(episode);
                    if let Some(permit) = permit {
                        window._permits.push(permit);
                    }
                    self.next += 1;
                }
                Err(error) if window.episodes.is_empty() => return Err(error),
                Err(error) => {
                    self.deferred = Some(error);
                    break;
                }
            }
            if self.next > self.end
                || window.episodes.len() == self.limit
                || rows >= ROW_LIMIT
                || bytes >= BYTE_LIMIT
            {
                break;
            }
            message = match self.receive.try_recv() {
                Ok(message) => message,
                // Do not wait to fill a window: the earliest ready chapter
                // starts translating as soon as it is available.
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.deferred = Some(anyhow::anyhow!("에피소드 {} 로더 연결 종료", self.next));
                    break;
                }
            };
        }
        Ok(Some(window))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn episode() -> Episode {
        Episode {
            title: "章".into(),
            lines: vec!["本文".into()],
        }
    }
    fn message(number: usize) -> Message {
        Ok(Loaded::new(number, episode()).0)
    }
    #[test]
    fn ready_predecessors_are_delivered_before_a_later_error() {
        let (send, receive) = mpsc::sync_channel(16);
        send.send(message(1)).unwrap();
        send.send(message(2)).unwrap();
        send.send(Err(anyhow::anyhow!("download failed"))).unwrap();
        drop(send);
        let mut reader = Reader::new(receive, 1, 3, 16);
        let window = reader.next_window().unwrap().unwrap();
        assert_eq!(window.first, 1);
        assert_eq!(window.episodes.len(), 2);
        assert!(reader
            .next_window()
            .err()
            .unwrap()
            .to_string()
            .contains("download failed"));
    }
    #[test]
    fn incomplete_source_and_wrong_order_fail_without_inventing_chapters() {
        let (send, receive) = mpsc::sync_channel(16);
        send.send(message(1)).unwrap();
        drop(send);
        let mut reader = Reader::new(receive, 1, 2, 16);
        assert_eq!(reader.next_window().unwrap().unwrap().episodes.len(), 1);
        assert!(reader.next_window().is_err());
        let (send, receive) = mpsc::sync_channel(16);
        send.send(message(2)).unwrap();
        let mut reader = Reader::new(receive, 1, 2, 16);
        assert!(reader.next_window().is_err());
    }
    #[test]
    fn a_ready_prefix_does_not_wait_for_the_next_download() {
        let (send, receive) = mpsc::sync_channel(CHAPTER_LIMIT);
        send.send(message(1)).unwrap();
        let (done, result) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            let mut reader = Reader::new(receive, 1, 2, CHAPTER_LIMIT);
            done.send(reader.next_window().unwrap().unwrap().episodes.len())
                .unwrap();
        });
        let ready = result.recv_timeout(std::time::Duration::from_secs(2));
        // Disconnect even on failure, so an accidental blocking receive cannot
        // leave a test thread waiting forever.
        drop(send);
        reader.join().unwrap();
        assert_eq!(ready.unwrap(), 1);
    }
    #[test]
    fn large_chapter_backpressure_is_released_on_completion_or_queue_cancellation() {
        for (blank, cancel) in [(false, false), (true, false), (false, true), (true, true)] {
            let (send, receive) = mpsc::sync_channel(CHAPTER_LIMIT);
            let (loaded, wait) = Loaded::new(
                1,
                Episode {
                    title: "章".into(),
                    lines: if blank {
                        vec![String::new(); 6_000]
                    } else {
                        vec!["本文".repeat(70_000)]
                    },
                },
            );
            let wait = wait.unwrap();
            send.send(Ok(loaded)).unwrap();
            drop(send);
            if cancel {
                drop(receive);
            } else {
                let mut reader = Reader::new(receive, 1, 1, CHAPTER_LIMIT);
                let window = reader.next_window().unwrap().unwrap();
                assert!(matches!(wait.try_recv(), Err(TryRecvError::Empty)));
                drop(window);
            }
            wait.recv_timeout(std::time::Duration::from_secs(2))
                .unwrap();
        }
    }
}
