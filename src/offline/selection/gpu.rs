//! Full-vocabulary greedy selection using device-side reductions. A second
//! pass is required only for rows in which signed zero can affect ordering.
use anyhow::{Context, Result};
use candle_core::{DType, Device, Tensor};
use std::collections::HashSet;

#[derive(Clone)]
pub(in crate::offline) struct TokenIds {
    ids: Tensor,
    tile: Option<usize>,
}
impl TokenIds {
    pub(in crate::offline) fn new(width: usize, device: &Device) -> candle_core::Result<Self> {
        let end = u32::try_from(width)
            .map_err(|_| candle_core::Error::Msg("selection vocabulary is too large".into()))?;
        Ok(Self {
            ids: Tensor::arange(0u32, end, device)?,
            tile: tile_width(width),
        })
    }

    fn tile_width(&self, width: usize) -> Option<usize> {
        if self.ids.elem_count() == width {
            self.tile
        } else {
            tile_width(width)
        }
    }
}

fn tile_width(width: usize) -> Option<usize> {
    // Use a divisor so all tiles share the original logits allocation.
    (width >= 16_384)
        .then(|| (512..=1024).rev().find(|&tile| width.is_multiple_of(tile)))
        .flatten()
}

pub(in crate::offline) fn select_rows(
    scores: &Tensor,
    vocab: usize,
    eos: u32,
    blocked: &[HashSet<u32>],
) -> Result<Vec<u32>> {
    let ids = TokenIds::new(scores.dim(1)?, scores.device())?;
    select_with_ids(scores, vocab, eos, blocked, &ids)
}

pub(in crate::offline) fn select_with_ids(
    scores: &Tensor,
    vocab: usize,
    eos: u32,
    blocked: &[HashSet<u32>],
    ids: &TokenIds,
) -> Result<Vec<u32>> {
    let (rows, columns) = scores.dims2()?;
    let width = columns.min(vocab);
    anyhow::ensure!(rows > 0 && rows == blocked.len(), "projection row mismatch");
    anyhow::ensure!(
        matches!(scores.dtype(), DType::F16 | DType::F32),
        "GPU selection requires F16 or F32 logits"
    );
    anyhow::ensure!(width > 0, "모델이 유효한 출력 토큰을 생성하지 못했습니다");
    let tiled_columns = ids
        .tile_width(columns)
        .filter(|&tile| columns <= ids.ids.elem_count() && columns - width < tile);
    // The pinned head includes language IDs beyond the tokenizer vocabulary.
    // Keep its divisible row width and exclude that short suffix before reducing.
    let (scan_width, tile) = match tiled_columns {
        Some(tile) => (columns, Some(tile)),
        None => (width, ids.tile_width(width)),
    };
    let sentinel = u32::try_from(scan_width).context("selection vocabulary is too large")?;
    let status_limit = sentinel
        .checked_mul(2)
        .context("selection vocabulary is too large")?;
    let count = rows
        .checked_mul(scan_width)
        .context("selection size overflow")?;
    u32::try_from(count).context("selection batch is too large")?;
    let device = scores.device();
    let ids = ids.ids.narrow(0, 0, scan_width)?;
    let excluded = Tensor::new(f32::NEG_INFINITY, device)?.to_dtype(scores.dtype())?;
    let scores = scores.narrow(1, 0, scan_width)?;
    // +inf and NaN fail this comparison. -inf is already the exclusion value.
    let flat = scores
        .lt(f32::INFINITY)?
        .where_cond(&scores, &excluded.broadcast_as((rows, scan_width))?)?
        .flatten_all()?;

    let mut indices = Vec::new();
    for (row, banned) in blocked.iter().enumerate() {
        let offset = (row * scan_width) as u32;
        indices.extend(
            banned
                .iter()
                .filter(|&&id| (id as usize) < width)
                .map(|&id| offset + id),
        );
        indices.extend(
            (0..width.min(4) as u32)
                .filter(|id| *id != eos && !banned.contains(id))
                .map(|id| offset + id),
        );
    }
    if !indices.is_empty() {
        let length = indices.len();
        let indices = Tensor::from_vec(indices, length, device)?;
        let values = excluded.broadcast_as(length)?.contiguous()?;
        // Special and banned IDs are disjoint and rows have distinct spans.
        // flat owns a fresh allocation, so input logits remain unchanged.
        flat.scatter_set(&indices, &values, 0)?;
    }
    let masked = flat.reshape((rows, scan_width))?;
    drop(flat);
    if width < scan_width {
        let tail = excluded
            .broadcast_as((rows, scan_width - width))?
            .contiguous()?;
        masked.slice_set(&tail, 1, width)?;
    }
    let absent = Tensor::new(sentinel, device)?;
    let (maximum, first) = maxima_and_first(&masked, &ids, &absent, tile)?;
    // A width offset marks zero maxima. One small readback also reports
    // exhausted rows, without any scores leaving the device.
    let maximum = maximum.squeeze(1)?;
    let valid = maximum
        .gt(f32::NEG_INFINITY)?
        .mul(&first.lt(width as u32)?)?;
    let first = maximum
        .eq(0f32)?
        .where_cond(&first.broadcast_add(&absent)?, &first)?;
    let mut result = valid
        .where_cond(&first, &Tensor::new(u32::MAX, device)?.broadcast_as(rows)?)?
        .to_vec1::<u32>()?;
    anyhow::ensure!(
        result.iter().all(|&id| id < status_limit),
        "모델이 유효한 출력 토큰을 생성하지 못했습니다"
    );

    let zero_rows = result
        .iter_mut()
        .enumerate()
        .filter_map(|(row, id)| {
            if *id < sentinel {
                return None;
            }
            *id -= sentinel;
            Some(row as u32)
        })
        .collect::<Vec<_>>();
    if !zero_rows.is_empty() {
        // Numerical equality conflates the two zeros. For marked rows only,
        // look for the first +0. If there is none, the first -0 already won.
        // The equality check also excludes nonzero reciprocal overflow.
        let indexes = Tensor::from_slice(&zero_rows, zero_rows.len(), device)?;
        let scores = masked.index_select(&indexes, 0)?.to_dtype(DType::F32)?;
        let positive_zero = scores.eq(0f32)?.mul(&scores.recip()?.eq(f32::INFINITY)?)?;
        let plus_ids = lowest_id(&positive_zero, &ids, &absent)?.to_vec1::<u32>()?;
        for (row, positive) in zero_rows.into_iter().zip(plus_ids) {
            if positive < sentinel {
                result[row as usize] = positive;
            }
        }
    }
    Ok(result)
}

fn maxima_and_first(
    scores: &Tensor,
    ids: &Tensor,
    absent: &Tensor,
    tile: Option<usize>,
) -> candle_core::Result<(Tensor, Tensor)> {
    let Some(tile) = tile else {
        let maximum = scores.max_keepdim(1)?;
        let first = lowest_id(&scores.broadcast_eq(&maximum)?, ids, absent)?;
        return Ok((maximum, first));
    };
    let (rows, width) = scores.dims2()?;
    let tiles = width / tile;
    let maxima = scores.reshape((rows, tiles, tile))?.max(2)?;
    let maximum = maxima.max_keepdim(1)?;
    // Candle's CUDA argmax can break ties across lanes out of index order.
    // Compare the tile maxima, then compare only the earliest winning tile.
    let device = scores.device();
    let first_tile = lowest_id(
        &maxima.broadcast_eq(&maximum)?,
        &ids.narrow(0, 0, tiles)?,
        &Tensor::new(tiles as u32, device)?,
    )?;
    let row_offsets = Tensor::arange_step(0u32, (rows * tiles) as u32, tiles as u32, device)?;
    let winners = scores
        .reshape((rows * tiles, tile))?
        .index_select(&(&row_offsets + &first_tile)?, 0)?;
    let local = lowest_id(
        &winners.broadcast_eq(&maximum)?,
        &ids.narrow(0, 0, tile)?,
        &Tensor::new(tile as u32, device)?,
    )?;
    Ok((maximum, (&first_tile.affine(tile as f64, 0.)? + &local)?))
}

fn lowest_id(eligible: &Tensor, ids: &Tensor, absent: &Tensor) -> candle_core::Result<Tensor> {
    eligible
        .where_cond(
            &ids.broadcast_as(eligible.shape())?,
            &absent.broadcast_as(eligible.shape())?,
        )?
        .min(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference(
        scores: &[Vec<f32>],
        vocab: usize,
        eos: u32,
        blocked: &[HashSet<u32>],
    ) -> Option<Vec<u32>> {
        scores
            .iter()
            .zip(blocked)
            .map(|(row, blocked)| super::super::select_scalar(row, vocab, eos, blocked))
            .collect()
    }

    #[test]
    fn tensor_selection_matches_scalar_for_nonfinite_scores_special_ids_and_bans() {
        let values = vec![
            vec![
                100.,
                f32::NAN,
                2.,
                f32::INFINITY,
                2.,
                2.,
                f32::NEG_INFINITY,
                1.,
            ],
            vec![100., 100., 1., 100., 5., 5., 4., f32::NAN],
        ];
        let scores = Tensor::new(values.clone(), &Device::Cpu).unwrap();
        for eos in [0, 2, 4, 99] {
            for blocked in [
                vec![HashSet::new(); 2],
                vec![[0, 2, 4, 7, 100].into(), [0, 4, 5].into()],
            ] {
                for vocab in [4, 7, 8, 10] {
                    let expected = reference(&values, vocab, eos, &blocked);
                    let actual = select_rows(&scores, vocab, eos, &blocked);
                    match expected {
                        Some(expected) => assert_eq!(actual.unwrap(), expected),
                        None => assert!(actual.is_err()),
                    }
                }
            }
        }
        // In-place sparse masking must not mutate the caller's logits.
        assert!(scores.to_vec2::<f32>().unwrap()[0][1].is_nan());
        assert_eq!(scores.to_vec2::<f32>().unwrap()[1][4], 5.);
    }

    #[test]
    fn signed_zero_and_equal_scores_have_explicit_total_order_and_lower_id_ties() {
        let values: Vec<Vec<f32>> = vec![
            vec![-1., -1., -0., -1., 0., 0., -0., -1.],
            vec![-1., -1., 0., -1., -0., 0., -0., -1.],
            vec![-1., -1., -0., -1., -0., -0., -0., -1.],
        ];
        let scores = Tensor::new(values, &Device::Cpu).unwrap();
        let unblocked: [HashSet<u32>; 3] = std::array::from_fn(|_| HashSet::new());
        assert_eq!(select_rows(&scores, 8, 2, &unblocked).unwrap(), [4, 2, 2]);
        let blocked = vec![[4, 5].into(), [2, 5].into(), [2].into()];
        assert_eq!(select_rows(&scores, 8, 2, &blocked).unwrap(), [2, 4, 4]);
    }

    #[test]
    fn reciprocal_overflow_does_not_treat_small_nonzero_scores_as_zero() {
        let values = vec![
            vec![-1., -1., -1., -1., f32::from_bits(1), f32::from_bits(2)],
            vec![-1., -1., -1., -1., 0., 1.],
        ];
        let scores = Tensor::new(values, &Device::Cpu).unwrap();
        assert_eq!(
            select_rows(&scores, 6, 2, &[HashSet::new(), HashSet::new()]).unwrap(),
            [5, 5]
        );
    }

    #[test]
    fn offset_and_noncontiguous_views_match_each_original_scalar_row() {
        let mut seed = 3u64;
        for width in [7usize, 64, 65, 257] {
            let source = (0..5 * (width + 2))
                .map(|_| {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    f32::from_bits((seed >> 32) as u32)
                })
                .collect::<Vec<_>>();
            let scores = Tensor::from_vec(source, (5, width + 2), &Device::Cpu)
                .unwrap()
                .narrow(0, 1, 3)
                .unwrap()
                .narrow(1, 1, width)
                .unwrap();
            let blocked = vec![[2, 4, 6, 10].into(), [5, 7, 11].into(), HashSet::new()];
            let values = scores.to_vec2::<f32>().unwrap();
            let expected = reference(&values, width - 1, 2, &blocked).unwrap();
            assert_eq!(
                select_rows(&scores, width - 1, 2, &blocked).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn empty_invalid_and_fully_blocked_rows_are_errors() {
        let scores = Tensor::new(
            &[[f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0., 1.]],
            &Device::Cpu,
        )
        .unwrap();
        assert!(select_rows(&scores, 5, 2, &[[4].into()]).is_err());
        assert!(select_rows(&scores, 0, 2, &[HashSet::new()]).is_err());
        assert!(select_rows(&scores, 5, 2, &[]).is_err());
        let integer = Tensor::new(&[[1u32; 5]], &Device::Cpu).unwrap();
        assert!(select_rows(&integer, 5, 2, &[HashSet::new()]).is_err());
    }

    fn compare_half_logits(device: &Device) {
        let min_half = 2f32.powi(-24);
        let scores = Tensor::new(
            &[
                [99., f32::NAN, -0., 99., 0., 0., -2., f32::NEG_INFINITY],
                [
                    99.,
                    99.,
                    f32::INFINITY,
                    99.,
                    min_half,
                    2. * min_half,
                    -min_half,
                    0.,
                ],
                [99., 99., 1., 99., 1., 1., 2., -1.],
            ],
            device,
        )
        .unwrap()
        .to_dtype(DType::F16)
        .unwrap();
        let banned = [[4].into(), HashSet::new(), [6].into()];
        let ids = TokenIds::new(16, device).unwrap();
        let selected = select_with_ids(&scores, 8, 2, &banned, &ids).unwrap();
        assert_eq!(selected, [5, 5, 2]);
    }

    #[test]
    fn retained_f16_logits_preserve_subnormals_ties_and_masked_zeros() {
        compare_half_logits(&Device::Cpu);
    }

    fn compare_tiled_logits(device: &Device) {
        // Pinned SMaLL-100 vocabulary. Include ties across reduction lanes
        // and tiles, signed zeros across tiles, and invalid/banned maxima.
        let width = 128_112;
        let vocab = 128_004;
        let tile = tile_width(width).unwrap();
        let mut rows = vec![vec![-1f32; width]; 4];
        rows[0][3] = 100.;
        rows[0][8] = 4.;
        rows[0][32] = 4.;
        rows[0][tile + 8] = 4.;
        rows[1][2] = -0.;
        rows[1][vocab - 1] = 0.;
        rows[1][width - 1] = 0.;
        rows[2][4] = f32::INFINITY;
        rows[2][5] = f32::NAN;
        rows[2][tile] = 100.;
        rows[2][vocab - 1] = 2.;
        rows[2][width - 1] = 10.;
        rows[3][tile - 1] = 5.;
        rows[3][tile] = 5.;
        let blocked = [
            HashSet::new(),
            HashSet::new(),
            [tile as u32].into(),
            [(tile - 1) as u32].into(),
        ];
        let logits = Tensor::new(rows.clone(), device).unwrap();
        for width in [vocab, width] {
            let expected = reference(&rows, width, 2, &blocked).unwrap();
            for dtype in [DType::F32, DType::F16] {
                let logits = logits.to_dtype(dtype).unwrap();
                assert_eq!(select_rows(&logits, width, 2, &blocked).unwrap(), expected);
            }
        }
    }

    #[test]
    fn tiled_projection_preserves_whole_vocabulary_ordering() {
        compare_tiled_logits(&Device::Cpu);
    }

    #[cfg(feature = "cuda")]
    #[test]
    #[ignore = "requires a real CUDA device and driver"]
    fn cuda_half_greedy_selection_matches_reference_on_device() {
        let device = Device::new_cuda(0).unwrap();
        compare_half_logits(&device);
        compare_tiled_logits(&device);
    }
}
