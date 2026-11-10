use super::QMatMul;
use crate::{Result, Tensor};
use std::collections::HashSet;
impl QMatMul {
    /// Exact M2M-style greedy top-one over CPU Q8 tiles. Returns `None` on
    /// unsupported backends/CPUs; callers retain ordinary logits selection.
    pub fn forward_top1(
        &self,
        input: &Tensor,
        vocab: usize,
        eos: u32,
        blocked: &[HashSet<u32>],
    ) -> Result<Option<Vec<u32>>> {
        #[cfg(target_arch = "x86_64")]
        {
            use super::{repack_x86, GgmlDType, QStorage};
            let Self::QTensor(weight) = self else {
                return Ok(None);
            };
            let QStorage::Cpu(storage) = &weight.storage else {
                return Ok(None);
            };
            let (n, k) = weight.shape.dims2()?;
            let (m, input_k) = input.dims2()?;
            if blocked.len() != m || input_k != k {
                crate::bail!("top-one shape mismatch");
            }
            if m < 2
                || storage.dtype() != GgmlDType::Q8_0
                || !repack_x86::select(storage.dtype(), m, n, k)
                || repack_x86::level() != Some(repack_x86::X86Level::Avx512Vnni)
            {
                return Ok(None);
            }
            let (source, layout) = input.storage_and_layout();
            let crate::Storage::Cpu(crate::CpuStorage::F32(values)) = &*source else {
                return Ok(None);
            };
            let Some((start, end)) = layout.contiguous_offsets() else {
                return Ok(None);
            };
            let packed = weight
                .repacked_qs
                .x86_get_or_init(|| repack_x86::pack(storage.dtype(), storage.as_ref(), n, k));
            let repack_x86::PackedX86::Q8_0(tiles) = packed else {
                unreachable!()
            };
            let lhs = repack_x86::quantize_lhs(&values[start..end], m, k);
            return Ok(Some(repack_x86::top1::run(
                tiles,
                &lhs,
                m,
                k,
                n,
                vocab.min(n),
                eos,
                blocked,
            )?));
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            let _ = (self, input, vocab, eos, blocked);
            Ok(None)
        }
    }
}
