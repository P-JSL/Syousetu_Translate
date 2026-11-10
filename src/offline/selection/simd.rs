//! CPU-gated exact finite argmax using the signed keys from f32::total_cmp.
//! Repetition-blocked winners fall back to the scalar selector in the parent.
#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

pub(super) fn maximum(scores: &[f32], vocab: usize, eos: u32) -> Option<u32> {
    let count = scores.len().min(vocab);
    if count < 64 || count >= i32::MAX as usize || !is_x86_feature_detected!("avx2") {
        return None;
    }
    // SAFETY: AVX2 is detected, all full blocks stay inside this slice, and
    // original token IDs fit the signed vector lanes. Unaligned loads are used.
    unsafe { scan(&scores[..count], eos) }
}

fn key(score: f32) -> i32 {
    let bits = score.to_bits() as i32;
    bits ^ (((bits >> 31) as u32 >> 1) as i32)
}

#[target_feature(enable = "avx2")]
unsafe fn scan(scores: &[f32], eos: u32) -> Option<u32> {
    let mut keys = _mm256_set1_epi32(i32::MIN);
    let mut ids = _mm256_set1_epi32(i32::MAX);
    let mut current = _mm256_setr_epi32(4, 5, 6, 7, 8, 9, 10, 11);
    let exponent = _mm256_set1_epi32(0x7f800000);
    let end = 4 + (scores.len() - 4) / 8 * 8;
    for start in (4..end).step_by(8) {
        let bits = _mm256_castps_si256(_mm256_loadu_ps(scores.as_ptr().add(start)));
        let invalid = _mm256_cmpeq_epi32(_mm256_and_si256(bits, exponent), exponent);
        let invert = _mm256_srli_epi32::<1>(_mm256_srai_epi32::<31>(bits));
        let next = _mm256_xor_si256(bits, invert);
        let greater = _mm256_andnot_si256(invalid, _mm256_cmpgt_epi32(next, keys));
        keys = _mm256_blendv_epi8(keys, next, greater);
        ids = _mm256_blendv_epi8(ids, current, greater);
        current = _mm256_add_epi32(current, _mm256_set1_epi32(8));
    }
    let mut lane_keys = [0i32; 8];
    let mut lane_ids = [0i32; 8];
    _mm256_storeu_si256(lane_keys.as_mut_ptr().cast(), keys);
    _mm256_storeu_si256(lane_ids.as_mut_ptr().cast(), ids);
    let mut best: Option<(u32, i32)> = None;
    let mut consider = |id: u32, candidate: i32| {
        if best.is_none_or(|(old_id, old)| candidate > old || (candidate == old && id < old_id)) {
            best = Some((id, candidate));
        }
    };
    for (&id, &candidate) in lane_ids.iter().zip(&lane_keys) {
        if id != i32::MAX {
            consider(id as u32, candidate);
        }
    }
    for (id, &score) in scores.iter().enumerate().take(4) {
        if id as u32 == eos && score.is_finite() {
            consider(id as u32, key(score));
        }
    }
    for (id, &score) in scores.iter().enumerate().skip(end) {
        if score.is_finite() {
            consider(id as u32, key(score));
        }
    }
    best.map(|(id, _)| id)
}
