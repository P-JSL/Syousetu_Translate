@echo off
setlocal
cd /d "%~dp0"
set "SYOUSETU_GUI_EXE="
for %%P in ("syousetu-gui.exe" "target\i686-pc-windows-msvc\release\syousetu-gui.exe" "target\i686-pc-windows-gnu\release\syousetu-gui.exe" "target\release\syousetu-gui.exe" "target\debug\syousetu-gui.exe" "target\i686-pc-windows-gnu\debug\syousetu-gui.exe") do (
    if not defined SYOUSETU_GUI_EXE if exist "%%~P" set "SYOUSETU_GUI_EXE=%%~fP"
)
if not defined SYOUSETU_GUI_EXE (
    echo Rust GUI not found. Extract the Windows release here, or build:
    echo cargo build --locked --features gui --bins --target i686-pc-windows-msvc --release
    pause
    exit /b 1
)
start "" "%SYOUSETU_GUI_EXE%"
exit /b 0
