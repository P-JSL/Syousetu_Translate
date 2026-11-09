#!/usr/bin/env python3
"""Fail CI if unused libraries or unexpected files enter the Windows package."""
from pathlib import Path
import sys
from package_web_release import EXPECTED, NOTICES, inspect_pe


def verify(directory):
    directory = Path(directory)
    actual = {p.relative_to(directory).as_posix() for p in directory.rglob('*') if p.is_file()}
    assert actual == EXPECTED | NOTICES, f'Unexpected package contents: {actual ^ (EXPECTED | NOTICES)}'
    imports = set()
    for name, subsystem in [('syousetu-gui.exe', 2), ('syousetu-translate.exe', 3)]:
        imports.update(n.lower() for n in inspect_pe((directory / name).read_bytes(), subsystem, 'x64'))
    # All imports must come from Windows or the documented MSVC runtime.
    allowed = {'advapi32.dll', 'bcrypt.dll', 'comctl32.dll', 'comdlg32.dll', 'crypt32.dll',
               'dwmapi.dll', 'gdi32.dll', 'imm32.dll', 'kernel32.dll', 'msvcrt.dll',
               'mswsock.dll', 'ncrypt.dll', 'ntdll.dll', 'ole32.dll', 'oleaut32.dll',
               'opengl32.dll', 'propsys.dll', 'secur32.dll', 'shell32.dll', 'shlwapi.dll',
               'user32.dll', 'userenv.dll', 'uxtheme.dll', 'version.dll', 'winmm.dll',
               'winspool.drv', 'ws2_32.dll', 'vcruntime140.dll', 'vcruntime140_1.dll',
               'ucrtbase.dll', 'dbghelp.dll', 'netapi32.dll', 'iphlpapi.dll', 'powrprof.dll'}
    unexpected = {n for n in imports if n not in allowed and not n.startswith(('api-ms-win-', 'ext-ms-win-'))}
    assert not unexpected, f'Undocumented runtime dependency: {unexpected}'
    print('Package contains only the two application executables, launcher, documentation and licenses.')
    print('Windows imports: ' + ', '.join(sorted(imports)))


if __name__ == '__main__':
    verify(sys.argv[1])
