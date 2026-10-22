@echo off
rem Builds Baguette.exe (the explorer) and baguette-cli.exe (command line, for
rem scripts and automation), and copies them next to this file.
rem Needs Rust (MSVC toolchain) and the Visual Studio Build Tools.
cd /d "%~dp0"

rem version: the VERSION file (major.minor.micro, bumped by hand for each
rem release), plus -<short commit>-dev for a build with uncommitted changes
set /p BASEVER=<VERSION
set COMMIT=
set DIRTY=
for /f %%h in ('git rev-parse --short HEAD 2^>nul') do set COMMIT=%%h
for /f "delims=" %%d in ('git status --porcelain 2^>nul') do set DIRTY=-dev
if "%COMMIT%"=="" (set BAGUETTE_VERSION=%BASEVER%-dev) else if "%DIRTY%"=="" (set BAGUETTE_VERSION=%BASEVER%) else (set BAGUETTE_VERSION=%BASEVER%-%COMMIT%-dev)
echo version %BAGUETTE_VERSION%

set PACKAGES=-p baguette -p baguette-cli
set FILES=Baguette.exe baguette-cli.exe
cargo build --release %PACKAGES% || exit /b 1
for %%f in (%FILES%) do copy /y target\release\%%f %%f >nul || exit /b 1
echo built %FILES% v%BAGUETTE_VERSION%
