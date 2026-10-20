@echo off
rem Builds Chomp.exe (the explorer) and chomp-cli.exe (command line, for
rem scripts and automation), and copies them next to this file.
rem "build.bat stub" also builds cement-cli.exe, chomp-cli's old name (a stub
rem that runs chomp-cli.exe), for scripts that still call it. It is no longer
rem part of the releases.
rem Needs Rust (MSVC toolchain) and the Visual Studio Build Tools.
cd /d "%~dp0"

rem version: the VERSION file (major.minor.micro, bumped by hand for each
rem release), plus -<short commit>-dev for a build with uncommitted changes
set /p BASEVER=<VERSION
set COMMIT=
set DIRTY=
for /f %%h in ('git rev-parse --short HEAD 2^>nul') do set COMMIT=%%h
for /f "delims=" %%d in ('git status --porcelain 2^>nul') do set DIRTY=-dev
if "%COMMIT%"=="" (set CHOMP_VERSION=%BASEVER%-dev) else if "%DIRTY%"=="" (set CHOMP_VERSION=%BASEVER%) else (set CHOMP_VERSION=%BASEVER%-%COMMIT%-dev)
echo version %CHOMP_VERSION%

set PACKAGES=-p chomp -p chomp-cli
set FILES=Chomp.exe chomp-cli.exe
if /i "%~1"=="stub" (
    set PACKAGES=-p chomp -p chomp-cli -p cement-cli
    set FILES=Chomp.exe chomp-cli.exe cement-cli.exe
)
cargo build --release %PACKAGES% || exit /b 1
for %%f in (%FILES%) do copy /y target\release\%%f %%f >nul || exit /b 1
echo built %FILES% v%CHOMP_VERSION%
