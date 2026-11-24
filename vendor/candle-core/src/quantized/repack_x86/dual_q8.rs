//! Two adjacent output tiles reuse each activation broadcast. Each output
//! keeps the scalar/block FMA order of the original Q8 register kernel.
use super::{BlockQ8K, TileQ8_0, TILE_N};
use core::arch::x86_64::*;
use std::ops::Range;

pub(super) const SLAB_ROWS: usize = 24;
const K_WINDOW: usize = 2;

#[inline(always)]
unsafe fn broadcast(act: &BlockQ8K, offset: usize) -> __m512i {
    _mm512_set1_epi32(std::ptr::read_unaligned(
        act.qs.as_ptr().add(offset).cast::<i32>(),
    ))
}
#[target_feature(enable = "avx512f,avx512vnni,avx512bw")]
#[allow(clippy::too_many_arguments)]
unsafe fn kernel<const M: usize>(
    tiles: &[TileQ8_0],
    lhs: &[BlockQ8K],
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
            let mut ia = [_mm512_loadu_si512(a.bias.as_ptr().cast()); M];
            let mut ib = [_mm512_loadu_si512(b.bias.as_ptr().cast()); M];
            let mut ic = [_mm512_loadu_si512(c.bias.as_ptr().cast()); M];
            let mut id = [_mm512_loadu_si512(d.bias.as_ptr().cast()); M];
            for group in 0..8 {
                let offset = group * TILE_N * 4;
                let wa = _mm512_loadu_si512(a.qs.as_ptr().add(offset).cast());
                let wb = _mm512_loadu_si512(b.qs.as_ptr().add(offset).cast());
                let wc = _mm512_loadu_si512(c.qs.as_ptr().add(offset).cast());
                let wd = _mm512_loadu_si512(d.qs.as_ptr().add(offset).cast());
                for row in 0..M {
                    let x = broadcast(acts[row], sub * 32 + group * 4);
                    let y = broadcast(acts[row], (sub + 1) * 32 + group * 4);
                    ia[row] = _mm512_dpbusd_epi32(ia[row], x, wa);
                    ib[row] = _mm512_dpbusd_epi32(ib[row], y, wb);
                    ic[row] = _mm512_dpbusd_epi32(ic[row], x, wc);
                    id[row] = _mm512_dpbusd_epi32(id[row], y, wd);
                }
            }
            for (left, right, sl, sr) in [(a, c, &ia, &ic), (b, d, &ib, &id)] {
                let dl = _mm512_loadu_ps(left.d.as_ptr());
                let dr = _mm512_loadu_ps(right.d.as_ptr());
                for row in 0..M {
                    let scale = _mm512_set1_ps(acts[row].d);
                    let vl = _mm512_cvtepi32_ps(sl[row]);
                    let vr = _mm512_cvtepi32_ps(sr[row]);
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
unsafe fn six_rows<const M: usize, const RESUME: bool>(
    tiles: &[TileQ8_0],
    lhs: &[BlockQ8K],
    kb: usize,
    nt: usize,
    m0: usize,
    dst: *mut f32,
    n: usize,
    column: usize,
    blocks: Range<usize>,
) {
    let mut acc0 = [_mm512_setzero_ps(); M];
    let mut acc1 = [_mm512_setzero_ps(); M];
    if RESUME && blocks.start != 0 {
        for row in 0..M {
            acc0[row] = _mm512_loadu_ps(dst.add((m0 + row) * n + column));
            acc1[row] = _mm512_loadu_ps(dst.add((m0 + row) * n + column + TILE_N));
        }
    }
    for block in blocks {
        let acts: [&BlockQ8K; M] = std::array::from_fn(|row| &lhs[(m0 + row) * kb + block]);
        for sub in 0..8 {
            let left = &tiles[nt * kb * 8 + block * 8 + sub];
            let right = &tiles[(nt + 1) * kb * 8 + block * 8 + sub];
            let mut ia = [_mm512_loadu_si512(left.bias.as_ptr().cast()); M];
            let mut ib = [_mm512_loadu_si512(right.bias.as_ptr().cast()); M];
            for group in 0..8 {
                let offset = group * TILE_N * 4;
                let wa = _mm512_loadu_si512(left.qs.as_ptr().add(offset).cast());
                let wb = _mm512_loadu_si512(right.qs.as_ptr().add(offset).cast());
                for row in 0..M {
                    let x = broadcast(acts[row], sub * 32 + group * 4);
                    ia[row] = _mm512_dpbusd_epi32(ia[row], x, wa);
                    ib[row] = _mm512_dpbusd_epi32(ib[row], x, wb);
                }
            }
            let dl = _mm512_loadu_ps(left.d.as_ptr());
            let dr = _mm512_loadu_ps(right.d.as_ptr());
            for row in 0..M {
                let scale = _mm512_set1_ps(acts[row].d);
                let vl = _mm512_cvtepi32_ps(ia[row]);
                let vr = _mm512_cvtepi32_ps(ib[row]);
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
        1 => kernel::<1>(tiles, lhs, kb, nt, m0, dst, n, nt * TILE_N),
        2 => kernel::<2>(tiles, lhs, kb, nt, m0, dst, n, nt * TILE_N),
        3 => kernel::<3>(tiles, lhs, kb, nt, m0, dst, n, nt * TILE_N),
        4 => kernel::<4>(tiles, lhs, kb, nt, m0, dst, n, nt * TILE_N),
        5 => six_rows::<5, false>(tiles, lhs, kb, nt, m0, dst, n, nt * TILE_N, 0..kb),
        6 => six_rows::<6, false>(tiles, lhs, kb, nt, m0, dst, n, nt * TILE_N, 0..kb),
        _ => unreachable!("invalid dual Q8 row block"),
    }
}

#[target_feature(enable = "avx512f,avx512vnni,avx512bw")]
#[allow(clippy::too_many_arguments)]
pub(super) unsafe fn local(
    tiles: &[TileQ8_0],
    lhs: &[BlockQ8K],
    kb: usize,
    nt: usize,
    m0: usize,
    rows: usize,
    dst: *mut f32,
) {
    match rows {
        1 => kernel::<1>(tiles, lhs, kb, nt, m0, dst, 32, 0),
        2 => kernel::<2>(tiles, lhs, kb, nt, m0, dst, 32, 0),
        3 => kernel::<3>(tiles, lhs, kb, nt, m0, dst, 32, 0),
        4 => kernel::<4>(tiles, lhs, kb, nt, m0, dst, 32, 0),
        5 => six_rows::<5, false>(tiles, lhs, kb, nt, m0, dst, 32, 0, 0..kb),
        6 => six_rows::<6, false>(tiles, lhs, kb, nt, m0, dst, 32, 0, 0..kb),
        _ => unreachable!("invalid local Q8 rows"),
    }
}

/// Sweep several activation groups while each packed weight window fits L1.
/// The scratch matrix stores the same F32 accumulators between windows; it
/// does not change the sequence of scaled 32-value dots for any output.
#[target_feature(enable = "avx512f,avx512vnni,avx512bw")]
pub(super) unsafe fn slab(
    tiles: &[TileQ8_0],
    lhs: &[BlockQ8K],
    kb: usize,
    nt: usize,
    rows: usize,
    scratch: &mut [f32; SLAB_ROWS * 2 * TILE_N],
) {
    debug_assert!((1..=SLAB_ROWS).contains(&rows));
    let dst = scratch.as_mut_ptr();
    for start in (0..kb).step_by(K_WINDOW) {
        let end = (start + K_WINDOW).min(kb);
        for m0 in (0..rows).step_by(6) {
            macro_rules! compute {
                ($rows:literal) => {
                    six_rows::<$rows, true>(tiles, lhs, kb, nt, m0, dst, 2 * TILE_N, 0, start..end)
                };
            }
            match (rows - m0).min(6) {
                1 => compute!(1),
                2 => compute!(2),
                3 => compute!(3),
                4 => compute!(4),
                5 => compute!(5),
                6 => compute!(6),
                _ => unreachable!(),
            }
        }
    }
}
