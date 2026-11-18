// SPDX-License-Identifier: Apache-2.0
//! Rust launcher adapted from Candle NN 0.11.1 for Candle's existing F32
//! softmax kernel. The kernel performs max, exp, sum and scaling in one launch.
use candle_core::{
    cuda_backend::{
        cudarc::driver::{LaunchConfig, PushKernelArg},
        kernels, WrapErr,
    },
    CudaStorage, Error, Layout, Result, Shape,
};

pub(super) fn softmax(storage: &CudaStorage, layout: &Layout) -> Result<(CudaStorage, Shape)> {
    let width = *layout
        .dims()
        .last()
        .ok_or_else(|| Error::Msg("softmax needs a dimension".into()))?;
    let shape = layout.shape();
    let elements = shape.elem_count();
    if width == 0 || elements == 0 || elements > i32::MAX as usize {
        candle_core::bail!("invalid CUDA softmax shape {shape:?}");
    }
    let (start, end) = layout
        .contiguous_offsets()
        .ok_or(Error::RequiresContiguous { op: "CUDA softmax" })?;
    let source = storage.as_cuda_slice::<f32>()?.slice(start..end);
    let device = &storage.device;
    let kernel = device.get_or_load_func("softmax_f32", &kernels::REDUCE)?;
    // SAFETY: The launch assigns one 32-thread warp to each row, writing
    // every output element. Width and total elements fit the kernel's i32
    // indexing, and the contiguous F32 source view includes exactly those rows.
    let mut output = unsafe { device.alloc::<f32>(elements)? };
    let columns = width as i32;
    let config = LaunchConfig {
        grid_dim: ((elements / width) as u32, 1, 1),
        block_dim: (1, 32, 1),
        shared_mem_bytes: 0,
    };
    let mut launch = kernel.builder();
    launch.arg(&source).arg(&mut output).arg(&columns);
    // SAFETY: Candle's softmax_f32 ABI takes (F32 pointer, F32 pointer, i32).
    unsafe { launch.launch(config) }.w()?;
    Ok((
        CudaStorage::wrap_cuda_slice(output, device.clone()),
        shape.clone(),
    ))
}
