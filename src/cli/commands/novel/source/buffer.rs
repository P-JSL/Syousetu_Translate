//! Large chapters use a lifetime guard so the producer cannot queue another
//! chapter until the large one has left the active translation window.
use crate::novel::site::Episode;
use std::sync::mpsc::{self, Receiver, Sender};
const SMALL_BYTES: usize = 128 * 1024;
pub(super) struct Consumed(Sender<()>);
impl Drop for Consumed {
    fn drop(&mut self) {
        // An unbounded acknowledgement never blocks during error unwinding.
        let _ = self.0.send(());
    }
}
pub(crate) struct Loaded {
    pub(super) number: usize,
    pub(super) episode: Episode,
    pub(super) bytes: usize,
    pub(super) permit: Option<Consumed>,
}
impl Loaded {
    pub(super) fn new(number: usize, episode: Episode) -> (Self, Option<Receiver<()>>) {
        // Include empty-line String headers and spare capacity as well as
        // text, so an HTML page with many blank paragraphs is not "small".
        let headers = std::mem::size_of::<Episode>()
            .saturating_add(episode.title.capacity())
            .saturating_add(
                episode
                    .lines
                    .capacity()
                    .saturating_mul(std::mem::size_of::<String>()),
            );
        let bytes = episode
            .lines
            .iter()
            .fold(headers, |bytes, line| bytes.saturating_add(line.capacity()));
        let (permit, wait) = if bytes > SMALL_BYTES {
            let (send, receive) = mpsc::channel();
            (Some(Consumed(send)), Some(receive))
        } else {
            (None, None)
        };
        (
            Self {
                number,
                episode,
                bytes,
                permit,
            },
            wait,
        )
    }
}
