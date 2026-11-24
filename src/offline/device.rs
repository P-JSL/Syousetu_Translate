//! Explicit device policy; automatic fallback is allowed only before output starts.
use anyhow::{bail, Context, Result};
use candle_core::Device;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InferenceDevice {
    #[default]
    Auto,
    Cpu,
    Cuda,
}
impl std::str::FromStr for InferenceDevice {
    type Err = anyhow::Error;
    fn from_str(value: &str) -> Result<Self> {
        match value {
            "auto" => Ok(Self::Auto),
            "cpu" => Ok(Self::Cpu),
            "cuda" => Ok(Self::Cuda),
            _ => bail!("연산 장치는 auto|cpu|cuda여야 합니다"),
        }
    }
}
impl InferenceDevice {
    pub(super) fn resolve(self, ordinal: usize) -> Result<Device> {
        anyhow::ensure!(ordinal <= 31, "GPU 번호는 0~31이어야 합니다");
        self.resolve_with(|| initialize_cuda(ordinal))
    }
    fn resolve_with(self, initialize: impl FnOnce() -> Result<Device>) -> Result<Device> {
        if self == Self::Cpu {
            return Ok(Device::Cpu);
        }
        match initialize() {
            Ok(device) => Ok(device),
            Err(error) if self == Self::Auto => {
                #[cfg(feature = "cuda")]
                eprintln!("GPU를 준비할 수 없어 CPU로 시작합니다: {error:#}");
                let _ = error;
                Ok(Device::Cpu)
            }
            Err(error) => {
                Err(error).context("CUDA 시작 실패; CPU 사용은 --device cpu를 지정해 주세요")
            }
        }
    }
}
#[cfg(feature = "cuda")]
fn initialize_cuda(ordinal: usize) -> Result<Device> {
    // cuda_if_available checks build features, not actual device availability.
    Ok(Device::new_cuda(ordinal)?)
}
#[cfg(not(feature = "cuda"))]
fn initialize_cuda(_ordinal: usize) -> Result<Device> {
    bail!("이 실행 파일은 CPU 전용입니다. CUDA 기능 빌드가 필요합니다")
}

#[cfg(feature = "cuda")]
pub(super) fn cuda_free_bytes(device: &Device) -> Option<u64> {
    let Device::Cuda(device) = device else {
        return None;
    };
    let (free, _) = device.cuda_stream().context().mem_get_info().ok()?;
    Some(free as u64)
}

#[cfg(not(feature = "cuda"))]
pub(super) fn cuda_free_bytes(_device: &Device) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cpu_never_initializes_cuda_and_auto_handles_missing_driver() {
        let cpu = InferenceDevice::Cpu.resolve_with(|| panic!("CPU must not initialize CUDA"));
        assert!(cpu.unwrap().is_cpu());
        assert!(InferenceDevice::Auto
            .resolve_with(|| bail!("missing driver"))
            .unwrap()
            .is_cpu());
        assert!(InferenceDevice::Cuda
            .resolve_with(|| bail!("missing driver"))
            .is_err());
    }
    #[test]
    fn invalid_requests_are_rejected_without_opening_a_model() {
        assert!("gpu".parse::<InferenceDevice>().is_err());
        assert!(InferenceDevice::Cpu.resolve(32).is_err());
    }
}
