//! Exact greedy reduction over Q8 output tiles without a dense logits tensor.
use super::{dual_q8, kernels, BlockQ8K, TileQ8_0, TILE_N};
use core::arch::x86_64::*;
use rayon::prelude::*;
use std::collections::HashSet;
type Winner = Option<(u32, f32)>;
fn better(candidate: (u32, f32), previous: Winner) -> bool {
    previous.is_none_or(|(id, score)| {
        candidate.1.total_cmp(&score).is_gt()
            || (candidate.1.total_cmp(&score).is_eq() && candidate.0 < id)
    })
}
#[target_feature(enable = "avx512f")]
unsafe fn consider_tile(
    values: &[f32],
    base: usize,
    vocab: usize,
    eos: u32,
    blocked: &HashSet<u32>,
    winner: &mut Winner,
) {
    let scores = _mm512_loadu_ps(values.as_ptr());
    let bits = _mm512_castps_si512(scores);
    let exponent = _mm512_set1_epi32(0x7f800000);
    let finite = _mm512_cmpneq_epi32_mask(_mm512_and_si512(bits, exponent), exponent);
    let keys = _mm512_xor_si512(bits, _mm512_srli_epi32::<1>(_mm512_srai_epi32::<31>(bits)));
    let old = winner.map_or(i32::MIN, |(_, value)| {
        let bits = value.to_bits() as i32;
        bits ^ (((bits >> 31) as u32 >> 1) as i32)
    });
    let mut mask = finite & _mm512_cmpge_epi32_mask(keys, _mm512_set1_epi32(old));
    while mask != 0 {
        let lane = mask.trailing_zeros() as usize;
        mask &= mask - 1;
        let id = (base + lane) as u32;
        if id as usize >= vocab || (id < 4 && id != eos) {
            continue;
        }
        let score = values[lane];
        if better((id, score), *winner) && !blocked.contains(&id) {
            *winner = Some((id, score));
        }
    }
}
#[target_feature(enable = "avx512f,avx512vnni,avx512bw")]
unsafe fn tail(
    tiles: &[TileQ8_0],
    lhs: &[BlockQ8K],
    kb: usize,
    nt: usize,
    m0: usize,
    rows: usize,
    dst: *mut f32,
) {
    match rows {
        1 => kernels::q8_0_unit::<1>(tiles, lhs, kb, nt, m0, dst, 32, 0),
        2 => kernels::q8_0_unit::<2>(tiles, lhs, kb, nt, m0, dst, 32, 0),
        3 => kernels::q8_0_unit::<3>(tiles, lhs, kb, nt, m0, dst, 32, 0),
        4 => kernels::q8_0_unit::<4>(tiles, lhs, kb, nt, m0, dst, 32, 0),
        5 => kernels::q8_0_unit::<5>(tiles, lhs, kb, nt, m0, dst, 32, 0),
        6 => kernels::q8_0_unit::<6>(tiles, lhs, kb, nt, m0, dst, 32, 0),
        _ => unreachable!("invalid tail rows"),
    }
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn run(
    tiles: &[TileQ8_0],
    lhs: &[BlockQ8K],
    m: usize,
    k: usize,
    n: usize,
    vocab: usize,
    eos: u32,
    blocked: &[HashSet<u32>],
) -> crate::Result<Vec<u32>> {
    let corrections = dual_q8::corrections(lhs);
    let row_step = 6;
    let row_units = m.div_ceil(row_step);
    let n_tiles = n / TILE_N;
    let items = n_tiles.div_ceil(2) * row_units;
    let chunks = items.min(rayon::current_num_threads() * 4).max(1);
    let width = items.div_ceil(chunks);
    let partials = (0..chunks)
        .into_par_iter()
        .map(|chunk| {
            let mut winners = vec![None; m];
            let mut values = [0f32; 6 * 32];
            for unit in chunk * width..items.min((chunk + 1) * width) {
                let nt = unit / row_units * 2;
                let m0 = unit % row_units * row_step;
                let rows = (m - m0).min(row_step);
                let columns = if nt + 1 < n_tiles { 32 } else { 16 };
                // SAFETY: caller gates AVX-512 VNNI. Tile/activation sizes are
                // checked by the matrix wrapper. Every write stays in `values`.
                unsafe {
                    // Kernels index destination rows from m0. A local activation
                    // slice makes both source and destination start at zero.
                    let source = &lhs[m0 * (k / 256)..(m0 + rows) * (k / 256)];
                    let sums = &corrections[m0 * (k / 256)..(m0 + rows) * (k / 256)];
                    if columns == 32 {
                        dual_q8::local(
                            tiles,
                            source,
                            sums,
                            k / 256,
                            nt,
                            0,
                            rows,
                            values.as_mut_ptr(),
                        );
                    } else {
                        tail(tiles, source, k / 256, nt, 0, rows, values.as_mut_ptr());
                    }
                }
                for row in 0..rows {
                    for column in (0..columns).step_by(16) {
                        // SAFETY: the wrapper gates AVX-512; complete tile
                        // slices contain exactly sixteen initialized scores.
                        unsafe {
                            consider_tile(
                                &values[row * 32 + column..][..16],
                                nt * TILE_N + column,
                                vocab,
                                eos,
                                &blocked[m0 + row],
                                &mut winners[m0 + row],
                            );
                        }
                    }
                }
            }
            winners
        })
        .collect::<Vec<_>>();
    let mut winners = vec![None; m];
    for partial in partials {
        for (best, candidate) in winners.iter_mut().zip(partial) {
            if let Some(candidate) = candidate {
                if better(candidate, *best) {
                    *best = Some(candidate);
                }
            }
        }
    }
    winners
        .into_iter()
        .map(|winner| {
            winner
                .map(|(id, _)| id)
                .ok_or_else(|| crate::Error::Msg("no finite unblocked output token".into()))
        })
        .collect()
}
