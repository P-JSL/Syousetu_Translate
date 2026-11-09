use anyhow::{bail, Result};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParallelPolicy {
    Auto,
    Serial,
    Parallel,
}
impl std::str::FromStr for ParallelPolicy {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "auto" => Ok(Self::Auto),
            "serial" => Ok(Self::Serial),
            "parallel" => Ok(Self::Parallel),
            _ => bail!("병렬 모드는 auto|serial|parallel이어야 합니다"),
        }
    }
}
