//! Bounded workers and disjoint output slots, adapted from the CPU-budget and
//! deterministic merge pattern in P-JSL/gacha_pickup_calculator (see notices).
use anyhow::{Context, Result};

pub fn map<W: Send>(
    workers: &mut [W],
    input: &[String],
    run: impl Fn(&mut W, &str) -> Result<String> + Sync,
) -> Result<Vec<String>> {
    if input.is_empty() {
        return Ok(Vec::new());
    }
    anyhow::ensure!(!workers.is_empty(), "번역 작업자가 없습니다");
    let chunk_size = input.len().div_ceil(workers.len().min(input.len()));
    let mut output = vec![None; input.len()];
    std::thread::scope(|scope| -> Result<()> {
        let mut handles = Vec::new();
        for (chunk, (worker, (source, slots))) in workers
            .iter_mut()
            .zip(input.chunks(chunk_size).zip(output.chunks_mut(chunk_size)))
            .enumerate()
        {
            let run = &run;
            handles.push(scope.spawn(move || -> Result<()> {
                for (offset, (text, slot)) in source.iter().zip(slots).enumerate() {
                    *slot = Some(run(worker, text).with_context(|| {
                        format!("본문 {}번째 줄 번역 실패", chunk * chunk_size + offset + 1)
                    })?);
                }
                Ok(())
            }));
        }
        // Join all workers after errors too. Results cannot leak into a later request.
        let mut error = None;
        for handle in handles {
            let result = handle
                .join()
                .map_err(|_| anyhow::anyhow!("내부 번역 작업이 중단되었습니다"))
                .and_then(|r| r);
            if let Err(e) = result {
                if error.is_none() {
                    error = Some(e);
                }
            }
        }
        if let Some(error) = error {
            return Err(error);
        }
        Ok(())
    })?;
    output
        .into_iter()
        .map(|slot| slot.context("번역 결과 누락: 파일을 저장하지 않습니다"))
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn variable_work_times_preserve_order_duplicates_and_empty_lines() {
        let input = ["遅い", "", "同じ", "同じ", "😀", "最後"].map(str::to_owned);
        let run = |_: &mut (), s: &str| {
            if s == "遅い" {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Ok(format!("訳:{s}"))
        };
        let serial = map(&mut [()], &input, run).unwrap();
        for count in [2, 3, 4, 8] {
            assert_eq!(map(&mut vec![(); count], &input, run).unwrap(), serial);
        }
    }
    #[test]
    fn failures_never_return_a_partial_success() {
        let input = ["ok", "bad", "ok"].map(str::to_owned);
        assert!(map(&mut [(), ()], &input, |_, s| {
            anyhow::ensure!(s != "bad", "injected failure");
            Ok(s.into())
        })
        .is_err());
        assert!(map(&mut [], &input, |_: &mut (), s| Ok(s.into())).is_err());
        assert!(map(&mut [()], &[], |_, s| Ok(s.into())).unwrap().is_empty());
    }
}
