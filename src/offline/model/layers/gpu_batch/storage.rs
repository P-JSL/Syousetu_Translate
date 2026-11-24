//! F32 self-attention K/V ring. Each row belongs to one (step, slot).
use candle_core::{DType, Device, Result, Tensor};

#[derive(Default)]
pub(super) struct Kv {
    pub(super) history: Option<Tensor>,
    pub(super) sources: Option<Tensor>,
}

pub(super) fn history<'a>(
    bank: &'a mut Option<Tensor>,
    step: usize,
    capacity: usize,
    slots: usize,
    width: usize,
    device: &Device,
) -> Result<&'a Tensor> {
    if bank
        .as_ref()
        .is_none_or(|current| current.dims()[0] != capacity)
    {
        let next = Tensor::zeros((capacity, slots, width), DType::F32, device)?;
        if let Some(current) = bank.as_ref() {
            let old_capacity = current.dim(0)?;
            // Carry over the live tail in chronological order, even if it
            // wraps in either ring. Growth is geometric and only occurs when
            // a live context is about to exceed the old history capacity.
            let mut position = step.saturating_sub(old_capacity);
            while position < step {
                let source = position % old_capacity;
                let target = position % capacity;
                let length = (step - position)
                    .min(old_capacity - source)
                    .min(capacity - target);
                next.slice_set(&current.narrow(0, source, length)?, 0, target)?;
                position += length;
            }
        }
        *bank = Some(next);
    }
    Ok(bank.as_ref().expect("history was allocated"))
}
