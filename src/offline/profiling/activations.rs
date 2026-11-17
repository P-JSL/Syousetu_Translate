//! Read-only, bounded samples of exact ReLU zeros; not a pruning predictor.
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_ROWS_PER_CALL: usize = 8;
static COUNTERS: [AtomicU64; 9] = [const { AtomicU64::new(0) }; 9];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ActivationProfile {
    pub calls: u64,
    pub observed_rows: u64,
    pub sampled_rows: u64,
    pub sampled_values: u64,
    pub exact_zeros: u64,
    pub groups_of_four: u64,
    pub zero_groups_of_four: u64,
    pub groups_of_32: u64,
    pub zero_groups_of_32: u64,
}

fn count(values: &[f32], width: usize) -> ActivationProfile {
    if width == 0 || values.is_empty() || !values.len().is_multiple_of(width) {
        return ActivationProfile::default();
    }
    let rows = values.len() / width;
    let samples = rows.min(MAX_ROWS_PER_CALL);
    let mut result = ActivationProfile {
        calls: 1,
        observed_rows: rows as u64,
        sampled_rows: samples as u64,
        ..Default::default()
    };
    for sample in 0..samples {
        let start = (sample * rows / samples) * width;
        let row = &values[start..start + width];
        result.sampled_values += width as u64;
        result.exact_zeros += row.iter().filter(|&&value| value == 0.).count() as u64;
        for (size, groups, zeros) in [
            (
                4,
                &mut result.groups_of_four,
                &mut result.zero_groups_of_four,
            ),
            (32, &mut result.groups_of_32, &mut result.zero_groups_of_32),
        ] {
            for group in row.chunks_exact(size) {
                *groups += 1;
                *zeros += u64::from(group.iter().all(|&value| value == 0.));
            }
        }
    }
    result
}

impl ActivationProfile {
    fn values(self) -> [u64; 9] {
        [
            self.calls,
            self.observed_rows,
            self.sampled_rows,
            self.sampled_values,
            self.exact_zeros,
            self.groups_of_four,
            self.zero_groups_of_four,
            self.groups_of_32,
            self.zero_groups_of_32,
        ]
    }
}

pub(crate) fn observe_relu(values: &[f32], width: usize) {
    for (counter, value) in COUNTERS.iter().zip(count(values, width).values()) {
        counter.fetch_add(value, Ordering::Relaxed);
    }
}

pub(super) fn snapshot() -> ActivationProfile {
    let value = COUNTERS
        .each_ref()
        .map(|counter| counter.load(Ordering::Relaxed));
    ActivationProfile {
        calls: value[0],
        observed_rows: value[1],
        sampled_rows: value[2],
        sampled_values: value[3],
        exact_zeros: value[4],
        groups_of_four: value[5],
        zero_groups_of_four: value[6],
        groups_of_32: value[7],
        zero_groups_of_32: value[8],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zero_groups_are_proved_from_all_members_and_never_from_nonfinite_values() {
        let mut values = vec![0.; 32];
        values.extend([1.; 32]);
        values.extend((0..32).map(|i| (i % 2) as f32));
        values.extend((0..32).map(|i| if i < 4 { 0. } else { 1. }));
        let profile = count(&values, 32);
        assert_eq!(profile.values(), [1, 4, 4, 128, 52, 32, 9, 4, 1]);
        values[..4].copy_from_slice(&[
            f32::NAN,
            f32::INFINITY,
            f32::MIN_POSITIVE,
            f32::from_bits(1),
        ]);
        let profile = count(&values, 32);
        assert_eq!(profile.exact_zeros, 48);
        assert_eq!(profile.zero_groups_of_four, 8);
        assert_eq!(profile.zero_groups_of_32, 0);
    }
    #[test]
    fn sample_counts_are_bounded_and_distinguished_from_all_observed_rows() {
        let profile = count(&vec![0.; 17 * 32], 32);
        assert_eq!(profile.observed_rows, 17);
        assert_eq!(profile.sampled_rows, 8);
        assert_eq!(profile.sampled_values, 256);
        for (values, width) in [(&[][..], 32), (&[1., 2.][..], 0), (&[1., 2.][..], 3)] {
            assert_eq!(count(values, width), ActivationProfile::default());
        }
    }
}
