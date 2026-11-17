//! Maintainer-only timing and sampled Linux process high-water RSS.
use anyhow::Result;
use std::{
    fs,
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};

pub(super) struct Measurement {
    pub seconds: f64,
    pub status: ExitStatus,
    pub sampled_peak_rss_kib: Option<u64>,
}

struct RunningChild(Child);
impl Drop for RunningChild {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

#[cfg(target_os = "linux")]
fn parse_high_water(text: &str) -> Option<u64> {
    let mut parts = text
        .lines()
        .find(|line| line.starts_with("VmHWM:"))?
        .split_whitespace();
    parts.next()?;
    let value = parts.next()?.parse().ok()?;
    (parts.next()? == "kB").then_some(value)
}

fn high_water(pid: u32) -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        fs::read_to_string(format!("/proc/{pid}/status"))
            .ok()
            .and_then(|text| parse_high_water(&text))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        None
    }
}

pub(super) fn run(command: &mut Command, log: &Path) -> Result<Measurement> {
    command.stdout(Stdio::null()).stderr(fs::File::create(log)?);
    let start = Instant::now();
    let mut child = RunningChild(command.spawn()?);
    let mut peak = None;
    let status = loop {
        if let Some(rss) = high_water(child.0.id()) {
            peak = Some(peak.unwrap_or(0).max(rss));
        }
        if let Some(status) = child.0.try_wait()? {
            break status;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    Ok(Measurement {
        seconds: start.elapsed().as_secs_f64(),
        status,
        sampled_peak_rss_kib: peak,
    })
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    #[test]
    fn resident_size_and_unknown_units_cannot_masquerade_as_peak_kib() {
        assert_eq!(
            parse_high_water("VmRSS: 20 kB\nVmHWM:\t320\tkB\n"),
            Some(320)
        );
        for text in [
            "VmRSS: 320 kB",
            "VmHWM: 320 B",
            "VmHWM: 18446744073709551616 kB",
            "",
        ] {
            assert_eq!(parse_high_water(text), None);
        }
    }
}
