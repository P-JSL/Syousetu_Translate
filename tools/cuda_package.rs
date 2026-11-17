//! Optional Windows CUDA overlay staging. Std-only; never enters the app graph.
//! Reuses the CPU package's PE parser without relaxing its production gates.
#[allow(dead_code)]
#[path = "release_audit.rs"]
mod audit;

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    path::{Path, PathBuf},
};
type Result<T> = std::result::Result<T, String>;

fn system_dll(name: &str) -> bool {
    name.starts_with("api-ms-win-")
        || name.starts_with("ext-ms-win-")
        || matches!(
            name,
            "advapi32.dll"
                | "bcrypt.dll"
                | "bcryptprimitives.dll"
                | "comctl32.dll"
                | "comdlg32.dll"
                | "concrt140.dll"
                | "crypt32.dll"
                | "dbghelp.dll"
                | "dwmapi.dll"
                | "gdi32.dll"
                | "imm32.dll"
                | "iphlpapi.dll"
                | "kernel32.dll"
                | "msvcrt.dll"
                | "msvcp140.dll"
                | "msvcp140_1.dll"
                | "msvcp140_2.dll"
                | "msvcp140_atomic_wait.dll"
                | "msvcp140_codecvt_ids.dll"
                | "mswsock.dll"
                | "ncrypt.dll"
                | "netapi32.dll"
                | "ntdll.dll"
                | "nvcuda.dll"
                | "ole32.dll"
                | "oleaut32.dll"
                | "opengl32.dll"
                | "powrprof.dll"
                | "propsys.dll"
                | "secur32.dll"
                | "setupapi.dll"
                | "shell32.dll"
                | "shlwapi.dll"
                | "user32.dll"
                | "userenv.dll"
                | "ucrtbase.dll"
                | "uxtheme.dll"
                | "version.dll"
                | "vcruntime140.dll"
                | "vcruntime140_1.dll"
                | "winmm.dll"
                | "winspool.drv"
                | "ws2_32.dll"
        )
}

// CUDA 12.8 redistributable library names. A match permits a library only when
// it occurs in the import closure, not merely because it exists in the toolkit.
fn redistributable(name: &str) -> bool {
    matches!(
        name,
        "cublas64_12.dll"
            | "cublaslt64_12.dll"
            | "cudart64_12.dll"
            | "curand64_10.dll"
            | "nvrtc64_120_0.dll"
            | "nvrtc-builtins64_128.dll"
            | "nvjitlink_120_0.dll"
            | "nvfatbin_120_0.dll"
    )
}

fn pe_imports(path: &Path) -> Result<Vec<String>> {
    let data = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let pe = data
        .get(0x3c..0x40)
        .ok_or("Truncated PE offset")?
        .try_into()
        .map(u32::from_le_bytes)
        .map_err(|_| "Invalid PE offset")? as usize;
    let at = pe.checked_add(24 + 68).ok_or("PE offset overflow")?;
    let subsystem = u16::from_le_bytes(
        data.get(at..at + 2)
            .ok_or("Truncated PE subsystem")?
            .try_into()
            .map_err(|_| "Invalid PE subsystem")?,
    );
    if !matches!(subsystem, 2 | 3) {
        return Err("Expected a Windows GUI/console PE image".into());
    }
    audit::imports(&data, subsystem)
}

fn dependency_closure(
    initial: Vec<String>,
    available: &BTreeMap<String, PathBuf>,
    mut imports: impl FnMut(&Path) -> Result<Vec<String>>,
) -> Result<BTreeSet<String>> {
    let mut pending = VecDeque::from(initial);
    let mut copied = BTreeSet::new();
    while let Some(name) = pending.pop_front() {
        let name = name.to_ascii_lowercase();
        if system_dll(&name) || copied.contains(&name) {
            continue;
        }
        if !redistributable(&name) {
            return Err(format!("Unexpected non-system DLL: {name}"));
        }
        let path = available
            .get(&name)
            .ok_or_else(|| format!("CUDA toolkit is missing required DLL: {name}"))?;
        pending.extend(imports(path)?);
        copied.insert(name.clone());
        // NVRTC loads its matching builtins library dynamically, so it need
        // not appear in the PE import table. It is needed only with NVRTC.
        if name == "nvrtc64_120_0.dll" {
            pending.push_back("nvrtc-builtins64_128.dll".into());
        }
    }
    Ok(copied)
}

fn copy(source: &Path, destination: &Path) -> Result<()> {
    if !source.is_file()
        || fs::symlink_metadata(source)
            .map_err(|e| e.to_string())?
            .is_symlink()
    {
        return Err(format!("Expected a regular file: {}", source.display()));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::copy(source, destination).map_err(|e| format!("{}: {e}", source.display()))?;
    Ok(())
}

#[cfg(any(windows, test))]
fn host_version(name: &str) -> Option<Vec<u32>> {
    let version = name
        .split('.')
        .map(str::parse::<u32>)
        .collect::<std::result::Result<Vec<_>, _>>()
        .ok()?;
    (version.len() >= 2 && version[0] == 14 && (29..40).contains(&version[1])).then_some(version)
}

#[cfg(windows)]
fn prepare_host_compiler() -> Result<()> {
    use std::{io::Write, process::Command};
    let base =
        PathBuf::from(std::env::var_os("ProgramFiles(x86)").ok_or("ProgramFiles(x86) is not set")?);
    let installer = base.join("Microsoft Visual Studio/Installer");
    let result = Command::new(installer.join("vswhere.exe"))
        .args([
            "-latest",
            "-products",
            "*",
            "-requires",
            "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
            "-property",
            "installationPath",
        ])
        .output()
        .map_err(|e| e.to_string())?;
    if !result.status.success() {
        return Err("vswhere could not locate Visual Studio C++ tools".into());
    }
    let install = String::from_utf8(result.stdout).map_err(|e| e.to_string())?;
    let install = install.trim();
    if install.is_empty() || install.contains(['\r', '\n']) {
        return Err("Expected one Visual Studio installation path".into());
    }
    let tools = Path::new(install).join("VC/Tools/MSVC");
    let find = || -> Result<Option<PathBuf>> {
        let mut versions = Vec::new();
        for entry in fs::read_dir(&tools).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if let Some(version) = host_version(&entry.file_name().to_string_lossy()) {
                let compiler = entry.path().join("bin/Hostx64/x64/cl.exe");
                if compiler.is_file() {
                    versions.push((version, compiler));
                }
            }
        }
        versions.sort_unstable();
        Ok(versions.pop().map(|(_, p)| p))
    };
    if find()?.is_none() {
        let status = Command::new(installer.join("setup.exe"))
            .args([
                "modify",
                "--installPath",
                install,
                "--add",
                "Microsoft.VisualStudio.Component.VC.14.38.17.8.x86.x64",
                "--quiet",
                "--norestart",
                "--nocache",
            ])
            .status()
            .map_err(|e| e.to_string())?;
        if !matches!(status.code(), Some(0 | 3010)) {
            return Err(format!("MSVC 14.38 installation failed: {status}"));
        }
    }
    let compiler = find()?.ok_or("CUDA 12.8 needs a compatible MSVC 14.2x/14.3x host toolset")?;
    let environment = std::env::var_os("GITHUB_ENV").ok_or("GITHUB_ENV is not set")?;
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(environment)
        .map_err(|e| e.to_string())?;
    writeln!(file, "NVCC_CCBIN={}", compiler.display()).map_err(|e| e.to_string())?;
    println!("CUDA host compiler: {}", compiler.display());
    Ok(())
}

#[cfg(not(windows))]
fn prepare_host_compiler() -> Result<()> {
    Err("host-compiler setup is for the optional Windows Actions job".into())
}

fn stage(executables: &Path, toolkit: &Path, output: &Path) -> Result<()> {
    if output.exists() {
        return Err("Use a new destination; existing packages are never overwritten".into());
    }
    let mut available = BTreeMap::new();
    for entry in fs::read_dir(toolkit.join("bin")).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().map_err(|e| e.to_string())?.is_file() {
            available.insert(
                entry.file_name().to_string_lossy().to_ascii_lowercase(),
                entry.path(),
            );
        }
    }
    let mut initial = Vec::new();
    for executable in ["syousetu-gui.exe", "syousetu-translate.exe"] {
        initial.extend(pe_imports(&executables.join(executable))?);
    }
    let libraries = dependency_closure(initial, &available, pe_imports)?;
    if libraries.is_empty() {
        return Err("No CUDA runtime imports found; refusing to label a CPU build as CUDA".into());
    }
    // The minimal Windows component installer omits EULA.txt. Embed NVIDIA's
    // unchanged CUDA 12.8 license from the hash-verified official archive;
    // packaging must not depend on optional SDK documentation components.
    let eula = include_bytes!("../licenses/NVIDIA-CUDA-12.8-EULA.txt");
    fs::create_dir(output).map_err(|e| e.to_string())?;
    for executable in ["syousetu-gui.exe", "syousetu-translate.exe"] {
        copy(&executables.join(executable), &output.join(executable))?;
    }
    for name in &libraries {
        copy(&available[name], &output.join(name))?;
    }
    for name in [
        "웹소설 번역기.bat",
        "THIRD_PARTY_NOTICES.md",
        "licenses/Apache-2.0.txt",
        "licenses/MIT-SMaLL100.txt",
        "licenses/MIT-gacha-pickup-calculator.txt",
    ] {
        copy(Path::new(name), &output.join(name))?;
    }
    copy(Path::new("docs/gpu-rtx40.md"), &output.join("README.md"))?;
    fs::write(output.join("licenses/NVIDIA-CUDA-EULA.txt"), eula).map_err(|e| e.to_string())?;
    fs::write(
        output.join("CUDA_RUNTIME_DLLS.txt"),
        format!("{}\n", libraries.into_iter().collect::<Vec<_>>().join("\n")),
    )
    .map_err(|e| e.to_string())?;
    println!("CUDA overlay staged; model weights and NVIDIA display driver are not included.");
    Ok(())
}

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let result = if args == ["host-compiler"] {
        prepare_host_compiler()
    } else if let [executables, toolkit, output] = args.as_slice() {
        stage(
            Path::new(executables),
            Path::new(toolkit),
            Path::new(output),
        )
    } else {
        Err("usage: cuda-package EXE_DIRECTORY CUDA_TOOLKIT_ROOT NEW_OUTPUT_DIRECTORY | host-compiler".into())
    };
    if let Err(error) = result {
        eprintln!("ERROR: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn available(names: &[&str]) -> BTreeMap<String, PathBuf> {
        names
            .iter()
            .map(|n| (n.to_string(), PathBuf::from(n)))
            .collect()
    }
    #[test]
    fn host_toolset_filter_rejects_newer_unsupported_compilers() {
        assert!(host_version("14.29.30133").is_some());
        assert!(host_version("14.38.33130").is_some());
        for version in ["14.40.0", "14.44.1", "15.1.0", "14.20.0", "invalid"] {
            assert!(host_version(version).is_none());
        }
    }
    #[test]
    fn follows_transitive_imports_without_copying_unused_toolkit_dlls() {
        let available = available(&["cublas64_12.dll", "cublaslt64_12.dll", "curand64_10.dll"]);
        let closure = dependency_closure(
            vec!["CUBLAS64_12.DLL".into(), "nvcuda.dll".into()],
            &available,
            |p| {
                Ok(if p == Path::new("cublas64_12.dll") {
                    vec!["cublasLt64_12.dll".into(), "kernel32.dll".into()]
                } else {
                    Vec::new()
                })
            },
        )
        .unwrap();
        assert_eq!(
            closure,
            ["cublas64_12.dll".into(), "cublaslt64_12.dll".into()].into()
        );
    }
    #[test]
    fn missing_unknown_and_path_imports_are_rejected() {
        for name in ["cublas64_12.dll", "python312.dll", "../cublas64_12.dll"] {
            assert!(
                dependency_closure(vec![name.into()], &BTreeMap::new(), |_| Ok(Vec::new()))
                    .is_err()
            );
        }
        assert!(!redistributable("nvcuda.dll"));
    }
    #[test]
    fn nvrtc_requires_its_dynamically_loaded_builtins() {
        let runtime = "nvrtc64_120_0.dll";
        let builtins = "nvrtc-builtins64_128.dll";
        assert!(
            dependency_closure(vec![runtime.into()], &available(&[runtime]), |_| Ok(
                Vec::new()
            ))
            .is_err()
        );
        let closure = dependency_closure(
            vec![runtime.into()],
            &available(&[runtime, builtins]),
            |_| Ok(Vec::new()),
        )
        .unwrap();
        assert_eq!(closure, [runtime.into(), builtins.into()].into());
    }
}
