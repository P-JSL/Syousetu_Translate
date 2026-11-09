use anyhow::{bail, Result};
#[derive(Clone, Copy, Debug)]
pub(super) enum Command {
    Worker,
    Models,
    Memory,
    Translate,
    InspectConfig,
    WriteConfig,
    Inspect,
    Download,
    File,
    #[cfg(feature = "legacy")]
    InspectDat,
    #[cfg(feature = "legacy")]
    InspectRootDll,
    #[cfg(feature = "legacy")]
    InspectEhnd,
    #[cfg(feature = "legacy")]
    AuditEhndRegex,
    #[cfg(feature = "legacy")]
    AuditEhndControl,
    #[cfg(feature = "legacy")]
    CompareEngines,
}
impl std::str::FromStr for Command {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        Ok(match s {
            "worker" => Self::Worker,
            "models" => Self::Models,
            "memory" => Self::Memory,
            "translate" => Self::Translate,
            "inspect-config" => Self::InspectConfig,
            "write-config" => Self::WriteConfig,
            "inspect" => Self::Inspect,
            "download" => Self::Download,
            "file" => Self::File,
            #[cfg(feature = "legacy")]
            "inspect-dat" => Self::InspectDat,
            #[cfg(feature = "legacy")]
            "inspect-root-dll" => Self::InspectRootDll,
            #[cfg(feature = "legacy")]
            "inspect-ehnd" => Self::InspectEhnd,
            #[cfg(feature = "legacy")]
            "audit-ehnd-regex" => Self::AuditEhndRegex,
            #[cfg(feature = "legacy")]
            "audit-ehnd-control" => Self::AuditEhndControl,
            #[cfg(feature = "legacy")]
            "compare-engines" => Self::CompareEngines,
            _ => bail!("알 수 없는 명령: {s}"),
        })
    }
}
