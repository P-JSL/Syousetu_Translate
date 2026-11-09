//! Opt-in lexical output projection. Full-vocabulary decoding remains default.
//! Rows are copied byte-for-byte from prepared Q8 weights, without requantizing.
use candle_core::{
    quantized::{QMatMul, QStorage, QTensor},
    Device, Result,
};
use std::{borrow::Cow, sync::Arc};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VocabularyPolicy {
    #[default]
    Full,
    KoreanShortlist,
}

pub(super) struct Head {
    pub(super) ids: Vec<u32>,
    pub(super) matrix: QMatMul,
}
impl Head {
    pub(super) fn gather(shared: &QMatMul, ids: Vec<u32>) -> Result<Arc<Self>> {
        if ids.is_empty() || !ids.len().is_multiple_of(16) {
            candle_core::bail!("고속 어휘는 16행 단위여야 합니다");
        }
        let QMatMul::QTensor(source) = shared else {
            candle_core::bail!("고속 어휘 모드는 준비된 Q8 모델을 사용해야 합니다");
        };
        let (rows, width) = source.shape().dims2()?;
        let dtype = source.dtype();
        if !width.is_multiple_of(dtype.block_size()) {
            candle_core::bail!("어휘 행의 블록 크기가 맞지 않습니다");
        }
        let stride = width / dtype.block_size() * dtype.type_size();
        let original = source.data()?;
        let mut selected = Vec::with_capacity(ids.len() * stride);
        for &id in &ids {
            if id as usize >= rows {
                candle_core::bail!("어휘 ID 범위 초과");
            }
            let start = id as usize * stride;
            selected.extend_from_slice(&original[start..start + stride]);
        }
        let storage = QStorage::from_data(Cow::Owned(selected), &Device::Cpu, dtype)?;
        let matrix = QMatMul::from_qtensor(QTensor::new(storage, (ids.len(), width))?)?;
        Ok(Arc::new(Self { ids, matrix }))
    }
}

pub(super) fn base_piece(piece: &str) -> bool {
    let mut latin = 0;
    let mut hangul = false;
    for ch in piece.chars() {
        if matches!(ch, '\u{1100}'..='\u{11ff}'|'\u{3130}'..='\u{318f}'|'\u{a960}'..='\u{a97f}'|'\u{ac00}'..='\u{d7af}'|'\u{d7b0}'..='\u{d7ff}')
        {
            hangul = true;
        } else if ch.is_ascii_alphabetic()
            || matches!(ch, '\u{00c0}'..='\u{024f}'|'\u{1e00}'..='\u{1eff}')
        {
            latin += 1;
        } else if ch.is_alphabetic() {
            return false;
        }
    }
    hangul || latin <= 1
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn base_has_korean_punctuation_and_short_latin_while_source_ids_are_added_separately() {
        for piece in ["▁학교", "했습니다", "▁A", "123", "。", "😀"] {
            assert!(base_piece(piece));
        }
        for piece in ["▁図書館", "カクヨム", "▁intercontinental", "Привет"] {
            assert!(!base_piece(piece));
        }
    }
    #[test]
    fn gathered_q8_rows_keep_the_original_projection_scores_and_id_mapping() {
        use candle_core::{quantized::GgmlDType, Module, Tensor};
        let values = (0..64 * 256)
            .map(|i| ((i * 17 % 101) as f32 - 50.0) / 31.0)
            .collect::<Vec<_>>();
        let weights = Tensor::from_vec(values, (64, 256), &Device::Cpu).unwrap();
        let shared =
            QMatMul::from_qtensor(QTensor::quantize(&weights, GgmlDType::Q8_0).unwrap()).unwrap();
        let mut ids = vec![2, 4, 7, 9, 50];
        ids.resize(16, 0);
        let head = Head::gather(&shared, ids.clone()).unwrap();
        let input = Tensor::from_vec(
            (0..256)
                .map(|i| (i % 7) as f32 / 7.0 - 0.5)
                .collect::<Vec<_>>(),
            (1, 256),
            &Device::Cpu,
        )
        .unwrap();
        let full = shared.forward(&input).unwrap().to_vec2::<f32>().unwrap();
        let selected = head
            .matrix
            .forward(&input)
            .unwrap()
            .to_vec2::<f32>()
            .unwrap();
        for (index, &id) in ids.iter().enumerate() {
            assert_eq!(selected[0][index], full[0][id as usize]);
        }
    }
}
