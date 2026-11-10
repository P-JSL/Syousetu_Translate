//! Two adjacent output tiles reuse each activation broadcast. Each output
//! keeps the scalar/block FMA order of the original Q8 register kernel.
use super::{BlockQ8K, TileQ8_0, TILE_N};
use core::arch::x86_64::*;

/// Cache exactly the old signed-offset correction once per activation block,
/// instead of repeating two integer loads, addition and conversion per N tile.
pub(super) fn corrections(input: &[BlockQ8K]) -> Vec<[f32; 8]> {
    input
        .iter()
        .map(|block| {
            std::array::from_fn(|part| {
                128.0 * (block.bsums[part * 2] as i32 + block.bsums[part * 2 + 1] as i32) as f32
            })
        })
        .collect()
}

#[inline(always)]
unsafe fn broadcast(act: &BlockQ8K, offset: usize) -> __m512i {
    _mm512_set1_epi32(std::ptr::read_unaligned(act.qs.as_ptr().add(offset).cast()))
}
#[target_feature(enable = "avx512f,avx512vnni,avx512bw")]
#[allow(clippy::too_many_arguments)]
unsafe fn kernel<const M: usize, const CACHED: bool>(
    tiles: &[TileQ8_0],
    lhs: &[BlockQ8K],
    corrections: &[[f32; 8]],
    kb: usize,
    nt: usize,
    m0: usize,
    dst: *mut f32,
    n: usize,
    column: usize,
) {
    let mut acc0 = [_mm512_setzero_ps(); M];
    let mut acc1 = [_mm512_setzero_ps(); M];
    for block in 0..kb {
        let acts: [&BlockQ8K; M] = std::array::from_fn(|row| &lhs[(m0 + row) * kb + block]);
        for sub in (0..8).step_by(2) {
            let a = &tiles[nt * kb * 8 + block * 8 + sub];
            let b = &tiles[nt * kb * 8 + block * 8 + sub + 1];
            let c = &tiles[(nt + 1) * kb * 8 + block * 8 + sub];
            let d = &tiles[(nt + 1) * kb * 8 + block * 8 + sub + 1];
            let mut ia = [_mm512_setzero_si512(); M];
            let mut ib = [_mm512_setzero_si512(); M];
            let mut ic = [_mm512_setzero_si512(); M];
            let mut id = [_mm512_setzero_si512(); M];
            for group in 0..8 {
                let offset = group * TILE_N * 4;
                let wa = _mm512_loadu_si512(a.qs.as_ptr().add(offset).cast());
                let wb = _mm512_loadu_si512(b.qs.as_ptr().add(offset).cast());
                let wc = _mm512_loadu_si512(c.qs.as_ptr().add(offset).cast());
                let wd = _mm512_loadu_si512(d.qs.as_ptr().add(offset).cast());
                for row in 0..M {
                    let x = broadcast(acts[row], sub * 32 + group * 4);
                    let y = broadcast(acts[row], (sub + 1) * 32 + group * 4);
                    ia[row] = _mm512_dpbusd_epi32(ia[row], wa, x);
                    ib[row] = _mm512_dpbusd_epi32(ib[row], wb, y);
                    ic[row] = _mm512_dpbusd_epi32(ic[row], wc, x);
                    id[row] = _mm512_dpbusd_epi32(id[row], wd, y);
                }
            }
            for (part, left, right, sl, sr) in [(sub, a, c, &ia, &ic), (sub + 1, b, d, &ib, &id)] {
                let dl = _mm512_loadu_ps(left.d.as_ptr());
                let dr = _mm512_loadu_ps(right.d.as_ptr());
                for row in 0..M {
                    let correction = _mm512_set1_ps(if CACHED {
                        corrections[(m0 + row) * kb + block][part]
                    } else {
                        128.0
                            * (acts[row].bsums[part * 2] as i32
                                + acts[row].bsums[part * 2 + 1] as i32)
                                as f32
                    });
                    let scale = _mm512_set1_ps(acts[row].d);
                    let vl = _mm512_sub_ps(_mm512_cvtepi32_ps(sl[row]), correction);
                    let vr = _mm512_sub_ps(_mm512_cvtepi32_ps(sr[row]), correction);
                    acc0[row] = _mm512_fmadd_ps(vl, _mm512_mul_ps(dl, scale), acc0[row]);
                    acc1[row] = _mm512_fmadd_ps(vr, _mm512_mul_ps(dr, scale), acc1[row]);
                }
            }
        }
    }
    for row in 0..M {
        _mm512_storeu_ps(dst.add((m0 + row) * n + column), acc0[row]);
        _mm512_storeu_ps(dst.add((m0 + row) * n + column + TILE_N), acc1[row]);
    }
}
// A single K group keeps 2*M integer and 2*M float accumulators live.
// At M=6 this stays within the 32 vector registers; two K groups would
// require 6*M accumulators before even loading operands.
#[target_feature(enable = "avx512f,avx512vnni,avx512bw")]
#[allow(clippy::too_many_arguments)]
unsafe fn six_rows<const M: usize, const CACHED: bool>(
    tiles: &[TileQ8_0],
    lhs: &[BlockQ8K],
    corrections: &[[f32; 8]],
    kb: usize,
    nt: usize,
    m0: usize,
    dst: *mut f32,
    n: usize,
    column: usize,
) {
    let mut acc0 = [_mm512_setzero_ps(); M];
    let mut acc1 = [_mm512_setzero_ps(); M];
    for block in 0..kb {
        let acts: [&BlockQ8K; M] = std::array::from_fn(|row| &lhs[(m0 + row) * kb + block]);
        for sub in 0..8 {
            let left = &tiles[nt * kb * 8 + block * 8 + sub];
            let right = &tiles[(nt + 1) * kb * 8 + block * 8 + sub];
            let mut ia = [_mm512_setzero_si512(); M];
            let mut ib = [_mm512_setzero_si512(); M];
            for group in 0..8 {
                let offset = group * TILE_N * 4;
                let wa = _mm512_loadu_si512(left.qs.as_ptr().add(offset).cast());
                let wb = _mm512_loadu_si512(right.qs.as_ptr().add(offset).cast());
                for row in 0..M {
                    let x = broadcast(acts[row], sub * 32 + group * 4);
                    ia[row] = _mm512_dpbusd_epi32(ia[row], wa, x);
                    ib[row] = _mm512_dpbusd_epi32(ib[row], wb, x);
                }
            }
            let dl = _mm512_loadu_ps(left.d.as_ptr());
            let dr = _mm512_loadu_ps(right.d.as_ptr());
            for row in 0..M {
                let correction = _mm512_set1_ps(if CACHED {
                    corrections[(m0 + row) * kb + block][sub]
                } else {
                    128.0
                        * (acts[row].bsums[sub * 2] as i32 + acts[row].bsums[sub * 2 + 1] as i32)
                            as f32
                });
                let scale = _mm512_set1_ps(acts[row].d);
                let vl = _mm512_sub_ps(_mm512_cvtepi32_ps(ia[row]), correction);
                let vr = _mm512_sub_ps(_mm512_cvtepi32_ps(ib[row]), correction);
                acc0[row] = _mm512_fmadd_ps(vl, _mm512_mul_ps(dl, scale), acc0[row]);
                acc1[row] = _mm512_fmadd_ps(vr, _mm512_mul_ps(dr, scale), acc1[row]);
            }
        }
    }
    for row in 0..M {
        _mm512_storeu_ps(dst.add((m0 + row) * n + column), acc0[row]);
        _mm512_storeu_ps(dst.add((m0 + row) * n + column + TILE_N), acc1[row]);
    }
}

/// Caller checks AVX-512 VNNI, two complete N tiles and 1..=6 active rows.
#[target_feature(enable = "avx512f,avx512vnni,avx512bw")]
#[allow(clippy::too_many_arguments)]
pub(super) unsafe fn compute(
    tiles: &[TileQ8_0],
    lhs: &[BlockQ8K],
    kb: usize,
    nt: usize,
    m0: usize,
    rows: usize,
    dst: *mut f32,
    n: usize,
) {
    match rows {
        1 => kernel::<1, false>(tiles, lhs, &[], kb, nt, m0, dst, n, nt * TILE_N),
        2 => kernel::<2, false>(tiles, lhs, &[], kb, nt, m0, dst, n, nt * TILE_N),
        3 => kernel::<3, false>(tiles, lhs, &[], kb, nt, m0, dst, n, nt * TILE_N),
        4 => kernel::<4, false>(tiles, lhs, &[], kb, nt, m0, dst, n, nt * TILE_N),
        5 => six_rows::<5, false>(tiles, lhs, &[], kb, nt, m0, dst, n, nt * TILE_N),
        6 => six_rows::<6, false>(tiles, lhs, &[], kb, nt, m0, dst, n, nt * TILE_N),
        _ => unreachable!("invalid dual Q8 row block"),
    }
}

#[target_feature(enable = "avx512f,avx512vnni,avx512bw")]
#[allow(clippy::too_many_arguments)]
pub(super) unsafe fn local(
    tiles: &[TileQ8_0],
    lhs: &[BlockQ8K],
    corrections: &[[f32; 8]],
    kb: usize,
    nt: usize,
    m0: usize,
    rows: usize,
    dst: *mut f32,
) {
    match rows {
        1 => kernel::<1, true>(tiles, lhs, corrections, kb, nt, m0, dst, 32, 0),
        2 => kernel::<2, true>(tiles, lhs, corrections, kb, nt, m0, dst, 32, 0),
        3 => kernel::<3, true>(tiles, lhs, corrections, kb, nt, m0, dst, 32, 0),
        4 => kernel::<4, true>(tiles, lhs, corrections, kb, nt, m0, dst, 32, 0),
        5 => six_rows::<5, true>(tiles, lhs, corrections, kb, nt, m0, dst, 32, 0),
        6 => six_rows::<6, true>(tiles, lhs, corrections, kb, nt, m0, dst, 32, 0),
        _ => unreachable!("invalid local Q8 rows"),
    }
}
