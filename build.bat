@echo off
rem Builds Chomp.exe (the explorer), chomp-cli.exe (command line, for scripts
rem and automation) and cement-cli.exe (its old name: a stub that runs
rem chomp-cli.exe, to be removed later), and copies them next to this file.
rem Needs Rust (MSVC toolchain) and the Visual Studio Build Tools.
cd /d "%~dp0"

rem version: <VERSION>.<commit count>-<short commit>, plus -dev with uncommitted changes
set /p BASEVER=<VERSION
set BUILDNO=
set COMMIT=
set DIRTY=
for /f %%c in ('git rev-list --count HEAD 2^>nul') do set BUILDNO=%%c
for /f %%h in ('git rev-parse --short HEAD 2^>nul') do set COMMIT=%%h
for /f "delims=" %%d in ('git status --porcelain 2^>nul') do set DIRTY=-dev
if "%BUILDNO%"=="" (set CHOMP_VERSION=%BASEVER%-dev) else (set CHOMP_VERSION=%BASEVER%.%BUILDNO%-%COMMIT%%DIRTY%)
echo version %CHOMP_VERSION%

cargo build --release || exit /b 1
for %%f in (Chomp.exe chomp-cli.exe cement-cli.exe) do copy /y target\release\%%f %%f >nul || exit /b 1
echo built Chomp.exe, chomp-cli.exe and cement-cli.exe v%CHOMP_VERSION%
