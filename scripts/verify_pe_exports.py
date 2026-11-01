#!/usr/bin/env python3
"""Validate 32-bit PE DLL exported names without executing the DLL.

Used in Windows x86 CI so the Rust ABI is checked even when original
commercial binaries cannot be distributed to the build agent.
"""
from __future__ import annotations

import pathlib
import struct
import sys


def u16(data: bytes, at: int) -> int:
    return struct.unpack_from("<H", data, at)[0]


def u32(data: bytes, at: int) -> int:
    return struct.unpack_from("<I", data, at)[0]


def exports(path: pathlib.Path) -> set[str]:
    data = path.read_bytes()
    if data[:2] != b"MZ":
        raise ValueError("Not MZ")
    pe = u32(data, 0x3C)
    if data[pe:pe+4] != b"PE\x00\x00":
        raise ValueError("Not PE")
    machine = u16(data, pe + 4)
    opt_size = u16(data, pe + 20)
    opt = pe + 24
    if machine != 0x14C or u16(data, opt) != 0x10B:
        raise ValueError(f"Expected i686 PE32, got machine={machine:04x}")
    export_rva = u32(data, opt + 96)
    section_start = opt + opt_size
    section_count = u16(data, pe + 6)
    sections = []
    for idx in range(section_count):
        off = section_start + idx*40
        virtual_size, virtual_address, raw_size, raw_pointer = struct.unpack_from(
            "<IIII", data, off + 8)
        sections.append((virtual_address,max(virtual_size,raw_size),raw_size,raw_pointer))

    def offset(rva: int) -> int:
        for addr,length,raw_length,ptr in sections:
            if addr <= rva < addr + min(length,raw_length):
                return ptr + rva - addr
        raise ValueError(f"Unmapped RVA {rva:x}")

    def cstr(index: int) -> str:
        end = data.index(b"\x00",index)
        return data[index:end].decode("ascii")

    directory = offset(export_rva)
    count = u32(data,directory + 24)
    names_rva = u32(data,directory + 32)
    if count > 4096:
        raise ValueError(f"Unexpected export count: {count}")
    result = set()
    for idx in range(count):
        name_rva = u32(data,offset(names_rva + idx*4))
        result.add(cstr(offset(name_rva)))
    return result


def main() -> int:
    if len(sys.argv) < 3:
        print("Usage: verify_pe_exports.py file.dll symbol [symbol ...]",file=sys.stderr)
        return 2
    path = pathlib.Path(sys.argv[1])
    actual = exports(path)
    missing = set(sys.argv[2:]) - actual
    print(f"{path.name}: {len(actual)} exported symbols")
    if missing:
        print(f"Missing exports: {sorted(missing)}",file=sys.stderr)
        print(f"Actual exports: {sorted(actual)}",file=sys.stderr)
        return 1
    print("Required compatibility entrypoints are present")
    return 0


if __name__ == "__main__":
    sys.exit(main())
