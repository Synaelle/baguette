@echo off
rem Builds Baguette.exe (the explorer) and baguette-cli.exe (command line, for
rem scripts and automation), and copies them next to this file.
rem "build.bat fast" is for trying changes: rebuilds take seconds instead of
rem minutes (no LTO, parallel and incremental), the exes are a bit bigger and
rem slower, and their version always ends in -dev. Releases use plain build.bat.
rem Needs Rust (MSVC toolchain) and the Visual Studio Build Tools.
cd /d "%~dp0"

rem version: the VERSION file (major.minor.micro, bumped by hand for each
rem release), plus -<short commit>-dev for a build with uncommitted changes
set /p BASEVER=<VERSION
set COMMIT=
set DIRTY=
for /f %%h in ('git rev-parse --short HEAD 2^>nul') do set COMMIT=%%h
for /f "delims=" %%d in ('git status --porcelain 2^>nul') do set DIRTY=-dev
if "%COMMIT%"=="" (set BAGUETTE_VERSION=%BASEVER%-dev) else if "%DIRTY%%~1"=="" (set BAGUETTE_VERSION=%BASEVER%) else (set BAGUETTE_VERSION=%BASEVER%-%COMMIT%-dev)
echo version %BAGUETTE_VERSION%

set PACKAGES=-p baguette -p baguette-cli
set FILES=Baguette.exe baguette-cli.exe
set PROFILE=release
if /i "%~1"=="fast" set PROFILE=fast
cargo build --profile %PROFILE% %PACKAGES% || exit /b 1
for %%f in (%FILES%) do copy /y target\%PROFILE%\%%f %%f >nul || exit /b 1
echo built %FILES% v%BAGUETTE_VERSION%
