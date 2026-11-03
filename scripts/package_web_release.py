#!/usr/bin/env python3
"""Package a verified Windows web-client artifact without vendor engine files."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import struct
import subprocess
import time
import tomllib
import zipfile

EXPECTED = {"syousetu-gui.exe", "syousetu-translate.exe", "웹소설 번역기.bat", "README.md"}


def inspect_pe(data: bytes, subsystem: int) -> list[str]:
    assert data[:2] == b"MZ", "Not an executable"
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    assert data[pe:pe + 4] == b"PE\0\0", "Not a PE image"
    assert struct.unpack_from("<H", data, pe + 4)[0] == 0x14C, "Expected Windows x86"
    opt = pe + 24
    assert struct.unpack_from("<H", data, opt)[0] == 0x10B, "Expected PE32"
    assert struct.unpack_from("<H", data, opt + 68)[0] == subsystem, "Unexpected subsystem"
    count = struct.unpack_from("<H", data, pe + 6)[0]
    start = opt + struct.unpack_from("<H", data, pe + 20)[0]
    sections = []
    for i in range(count):
        _, address, size, pointer = struct.unpack_from("<IIII", data, start + 40 * i + 8)
        sections.append((address, size, pointer))

    def offset(rva):
        for address, size, pointer in sections:
            if address <= rva < address + size:
                return pointer + rva - address
        raise ValueError(f"Unmapped RVA: {rva:x}")

    imports = []
    rva = struct.unpack_from("<I", data, opt + 104)[0]
    if rva:
        at = offset(rva)
        for _ in range(256):
            descriptor = struct.unpack_from("<IIIII", data, at)
            if not any(descriptor):
                break
            name = offset(descriptor[3])
            imports.append(data[name:data.index(b"\0", name)].decode("ascii"))
            at += 20
        else:
            raise ValueError("Unexpected import table length")
    return imports


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifact-dir", type=Path, required=True)
    parser.add_argument("--metadata", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--source-commit", required=True)
    parser.add_argument("--run-id", type=int, required=True)
    args = parser.parse_args()
    if not re.fullmatch(r"v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?", args.version):
        raise ValueError("Expected a version tag such as v0.1.0")
    if not re.fullmatch(r"[0-9a-f]{40}", args.source_commit):
        raise ValueError("Expected the full source commit SHA")

    repo = Path(__file__).resolve().parent.parent

    def source(name):
        return subprocess.check_output(["git", "-C", str(repo), "show", f"{args.source_commit}:{name}"])

    manifest = tomllib.loads(source("Cargo.toml").decode())
    assert manifest["package"]["version"] == args.version[1:], "Tag/package version mismatch"
    metadata = json.loads(args.metadata.read_text())
    assert metadata["name"] == "SyousetuTranslate-Web-Windows-x86"
    assert metadata["workflow_run"]["id"] == args.run_id
    assert metadata["workflow_run"]["head_sha"] == args.source_commit
    assert not metadata["expired"]
    assert {p.name for p in args.artifact_dir.iterdir()} == EXPECTED, "Unexpected artifact contents"
    payloads = {}
    for name in EXPECTED:
        path = args.artifact_dir / name
        assert path.is_file() and not path.is_symlink(), name
        payloads[name] = path.read_bytes()
    for name, original in [("README.md", "docs/windows-web-client.md"), ("웹소설 번역기.bat", "웹소설 번역기.bat")]:
        assert payloads[name].decode("utf-8").replace("\r\n", "\n") == source(original).decode("utf-8").replace("\r\n", "\n"), name

    imports = []
    for name, subsystem in [("syousetu-gui.exe", 2), ("syousetu-translate.exe", 3)]:
        imports.extend(inspect_pe(payloads[name], subsystem))
    if "vcruntime140.dll" in {name.lower() for name in imports}:
        appendix = "\n\n## Windows 실행 런타임\n\n이 빌드는 Microsoft Visual C++ v14 x86 런타임을 사용합니다.\nVCRUNTIME140.dll 관련 오류가 표시되면 Microsoft 공식 안내에서 x86 런타임을 설치해 주세요.\nhttps://learn.microsoft.com/cpp/windows/latest-supported-vc-redist\n"
        payloads["README.md"] = payloads["README.md"].decode("utf-8").replace("\r\n", "\n").encode("utf-8") + appendix.encode("utf-8")

    args.output_dir.mkdir(parents=True, exist_ok=True)
    package = args.output_dir / f"SyousetuTranslate-Web-{args.version}-Windows-x86.zip"
    timestamp = int(subprocess.check_output(["git", "-C", str(repo), "show", "-s", "--format=%ct", args.source_commit]))
    date = tuple(time.gmtime(max(timestamp, 315532800))[:6])
    with zipfile.ZipFile(package, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for name in sorted(payloads):
            info = zipfile.ZipInfo(name, date_time=date)
            info.create_system = 3
            info.external_attr = (0o100755 if name.endswith((".exe", ".bat")) else 0o100644) << 16
            archive.writestr(info, payloads[name], compress_type=zipfile.ZIP_DEFLATED, compresslevel=9)
    with zipfile.ZipFile(package) as archive:
        assert set(archive.namelist()) == EXPECTED
        assert archive.testzip() is None
    digest = hashlib.sha256(package.read_bytes()).hexdigest()
    (args.output_dir / "SHA256SUMS.txt").write_text(f"{digest}  {package.name}\n", encoding="ascii")
    print(f"{package.name}: {package.stat().st_size} bytes, SHA-256 {digest}")


if __name__ == "__main__":
    main()
