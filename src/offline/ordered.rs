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
    let mut output = vec![None; input.len()];
    // Each claim hands out a different mutable slot. Workers can immediately
    // claim another line when finished, without waiting for a slower chunk.
    // The lock protects only the iterator, never inference or a result write.
    let mut tasks = input
        .iter()
        .zip(output.iter_mut())
        .enumerate()
        .collect::<Vec<_>>();
    // Longest-source-first reduces the chance that the last remaining sentence
    // leaves other runners idle. The slot/index still belongs to input order.
    tasks.sort_by_key(|(index, (text, _))| (std::cmp::Reverse(text.len()), *index));
    let pending = std::sync::Mutex::new(tasks.into_iter());
    std::thread::scope(|scope| -> Result<()> {
        let mut handles = Vec::new();
        for worker in workers.iter_mut().take(input.len()) {
            let run = &run;
            let pending = &pending;
            handles.push(scope.spawn(move || -> Result<()> {
                loop {
                    let task = pending
                        .lock()
                        .map_err(|_| anyhow::anyhow!("번역 작업 배분 잠금 오류"))?
                        .next();
                    let Some((index, (text, slot))) = task else {
                        break;
                    };
                    *slot = Some(
                        run(worker, text)
                            .with_context(|| format!("본문 {}번째 줄 번역 실패", index + 1))?,
                    );
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
    fn heavy_work_is_claimed_first_while_result_slots_keep_input_order() {
        let input = ["短", "長い日本語の文章", "中くらい"].map(str::to_owned);
        let mut workers = [Vec::<String>::new()];
        let output = map(&mut workers, &input, |order, text| {
            order.push(text.to_owned());
            Ok(text.to_owned())
        })
        .unwrap();
        assert_eq!(output, input);
        assert_eq!(workers[0], ["長い日本語の文章", "中くらい", "短"]);
    }
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
