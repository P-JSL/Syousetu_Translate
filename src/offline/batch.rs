//! Deduplicate pending work and restore every original position, including
//! empty and repeated lines. Emit only the completed contiguous original prefix.
#[cfg(test)]
use anyhow::Context;
use anyhow::Result;
use std::collections::HashMap;
enum Slot {
    Ready(String),
    Pending(usize),
}
pub(super) struct Batch<'a> {
    pub(super) unique: Vec<String>,
    slots: Vec<Slot>,
    indices: HashMap<&'a str, usize>,
    counts: Vec<usize>,
    pub(super) ready: usize,
}
impl<'a> Batch<'a> {
    pub(super) fn new(lines: &'a [String], known: impl Fn(&str) -> Option<String>) -> Self {
        let mut batch = Self {
            unique: Vec::new(),
            slots: Vec::with_capacity(lines.len()),
            indices: HashMap::new(),
            counts: Vec::new(),
            ready: 0,
        };
        for source in lines {
            if let Some(text) = known(source) {
                batch.slots.push(Slot::Ready(text));
                batch.ready += 1;
                continue;
            }
            let index = *batch.indices.entry(source.as_str()).or_insert_with(|| {
                let i = batch.unique.len();
                batch.unique.push(source.clone());
                batch.counts.push(0);
                i
            });
            batch.counts[index] += 1;
            batch.slots.push(Slot::Pending(index));
        }
        batch
    }
    pub(super) fn count(&self, source: &str) -> usize {
        self.counts[self.indices[source]]
    }
    pub(super) fn emit_ready(
        &self,
        values: &[Option<String>],
        next: &mut usize,
        emit: &mut dyn FnMut(usize, &str) -> Result<()>,
    ) -> Result<()> {
        while let Some(slot) = self.slots.get(*next) {
            let text = match slot {
                Slot::Ready(text) => text,
                Slot::Pending(index) => match &values[*index] {
                    Some(text) => text,
                    None => break,
                },
            };
            emit(*next, text)?;
            *next += 1;
        }
        Ok(())
    }
    #[cfg(test)]
    pub(super) fn restore(mut self, values: Vec<String>) -> Result<Vec<String>> {
        anyhow::ensure!(
            values.len() == self.unique.len(),
            "고유 번역 결과가 누락되었습니다"
        );
        let mut values = values.into_iter().map(Some).collect::<Vec<_>>();
        self.slots
            .into_iter()
            .map(|slot| match slot {
                Slot::Ready(text) => Ok(text),
                Slot::Pending(i) => {
                    self.counts[i] -= 1;
                    if self.counts[i] == 0 {
                        values[i].take().context("번역 결과 누락")
                    } else {
                        values[i].clone().context("번역 결과 누락")
                    }
                }
            })
            .collect()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn streaming_waits_for_earlier_results_and_restores_duplicates() {
        let input = ["最初", "最後", "最初", ""].map(str::to_owned);
        let batch = Batch::new(&input, |source| source.is_empty().then(String::new));
        let mut values = vec![None, Some("마지막".to_owned())];
        let mut next = 0;
        let mut output = Vec::new();
        let mut emit = |index, text: &str| {
            output.push((index, text.to_owned()));
            Ok(())
        };
        batch.emit_ready(&values, &mut next, &mut emit).unwrap();
        assert_eq!(next, 0);
        values[0] = Some("처음".into());
        batch.emit_ready(&values, &mut next, &mut emit).unwrap();
        assert_eq!(next, input.len());
        assert_eq!(
            output,
            [
                (0, "처음".into()),
                (1, "마지막".into()),
                (2, "처음".into()),
                (3, "".into())
            ]
        );
    }
    #[test]
    fn duplicates_and_cached_lines_keep_original_order_and_count() {
        let input = ["同じ", "", "最後", "同じ", "cached"].map(str::to_owned);
        let b = Batch::new(&input, |s| {
            if s.is_empty() || s == "cached" {
                Some(s.into())
            } else {
                None
            }
        });
        assert_eq!(b.unique, ["同じ", "最後"]);
        assert_eq!(b.ready, 2);
        assert_eq!(b.count("同じ"), 2);
        assert_eq!(
            b.restore(vec!["같음".into(), "마지막".into()]).unwrap(),
            ["같음", "", "마지막", "같음", "cached"]
        );
        assert!(Batch::new(&input, |_| None).restore(vec![]).is_err());
    }
}
