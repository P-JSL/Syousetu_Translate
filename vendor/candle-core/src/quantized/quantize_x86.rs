//! Q8K activation quantization with exact scalar-compatible rounding.
use super::k_quants::{quantize_q8k_reference, BlockQ8K, QK_K};
#[cfg(target_arch = "x86")]
use core::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

#[target_feature(enable = "avx2")]
unsafe fn max_abs(xs: &[f32]) -> Option<f32> {
    let sign = _mm256_set1_ps(-0.0);
    let mut maximum = _mm256_setzero_ps();
    for offset in (0..xs.len()).step_by(8) {
        let value = _mm256_andnot_ps(sign, _mm256_loadu_ps(xs.as_ptr().add(offset)));
        if _mm256_movemask_ps(_mm256_cmp_ps::<_CMP_LE_OQ>(value, _mm256_set1_ps(f32::MAX))) != 255 {
            return None;
        }
        maximum = _mm256_max_ps(maximum, value);
    }
    let mut lanes = [0f32; 8];
    _mm256_storeu_ps(lanes.as_mut_ptr(), maximum);
    Some(lanes.into_iter().fold(0f32, f32::max))
}

#[target_feature(enable = "avx2")]
unsafe fn round_away(value: __m256) -> __m256i {
    let truncated = _mm256_round_ps::<{ _MM_FROUND_TO_ZERO | _MM_FROUND_NO_EXC }>(value);
    let sign = _mm256_set1_ps(-0.0);
    let fraction = _mm256_andnot_ps(sign, _mm256_sub_ps(value, truncated));
    let away = _mm256_cmp_ps::<_CMP_GE_OQ>(fraction, _mm256_set1_ps(0.5));
    let signed_one = _mm256_or_ps(_mm256_and_ps(sign, value), _mm256_set1_ps(1.0));
    let rounded = _mm256_add_ps(truncated, _mm256_and_ps(away, signed_one));
    let integers = _mm256_cvttps_epi32(rounded);
    _mm256_min_epi32(
        _mm256_set1_epi32(127),
        _mm256_max_epi32(_mm256_set1_epi32(-128), integers),
    )
}

/// Caller checks AVX2. Slices have the same block boundaries as the scalar
/// implementation; nonfinite and underflow/overflow cases use that reference.
#[target_feature(enable = "avx2")]
pub(super) unsafe fn quantize_q8k(xs: &[f32], ys: &mut [BlockQ8K]) {
    debug_assert_eq!(xs.len(), ys.len() * QK_K);
    for (index, y) in ys.iter_mut().enumerate() {
        let block = &xs[index * QK_K..(index + 1) * QK_K];
        let Some(amax) = max_abs(block) else {
            quantize_q8k_reference(block, std::slice::from_mut(y));
            continue;
        };
        if amax == 0.0 {
            y.d = 0.0;
            y.qs.fill(0);
            y.bsums.fill(0);
            continue;
        }
        // Retain the first signed maximum, including equal ± magnitude ties.
        let mut maximum = 0.0;
        for offset in (0..QK_K).step_by(8) {
            let value = _mm256_loadu_ps(block.as_ptr().add(offset));
            let absolute = _mm256_andnot_ps(_mm256_set1_ps(-0.0), value);
            let matches =
                _mm256_movemask_ps(_mm256_cmp_ps::<_CMP_EQ_OQ>(absolute, _mm256_set1_ps(amax)))
                    as u32;
            if matches != 0 {
                maximum = block[offset + matches.trailing_zeros() as usize];
                break;
            }
        }
        let scale = -127.0f32 / maximum;
        if !scale.is_finite() {
            quantize_q8k_reference(block, std::slice::from_mut(y));
            continue;
        }
        for offset in (0..QK_K).step_by(16) {
            let a = round_away(_mm256_mul_ps(
                _mm256_loadu_ps(block.as_ptr().add(offset)),
                _mm256_set1_ps(scale),
            ));
            let b = round_away(_mm256_mul_ps(
                _mm256_loadu_ps(block.as_ptr().add(offset + 8)),
                _mm256_set1_ps(scale),
            ));
            let a16 = _mm_packs_epi32(_mm256_castsi256_si128(a), _mm256_extracti128_si256::<1>(a));
            let b16 = _mm_packs_epi32(_mm256_castsi256_si128(b), _mm256_extracti128_si256::<1>(b));
            let bytes = _mm_packs_epi16(a16, b16);
            _mm_storeu_si128(y.qs.as_mut_ptr().add(offset) as *mut __m128i, bytes);
            let pairs = _mm_maddubs_epi16(_mm_set1_epi8(1), bytes);
            let sums = _mm_madd_epi16(pairs, _mm_set1_epi16(1));
            let sums = _mm_hadd_epi32(sums, sums);
            y.bsums[offset / 16] = _mm_cvtsi128_si32(_mm_hadd_epi32(sums, sums)) as i16;
        }
        y.d = 1.0 / scale;
    }
}
