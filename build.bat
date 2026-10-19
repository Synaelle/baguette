@echo off
rem Builds Chomp.exe (the explorer), chomp-cli.exe (command line, for scripts
rem and automation) and cement-cli.exe (its old name: a stub that runs
rem chomp-cli.exe, to be removed later) with MinGW-w64 (g++ and windres on PATH)
cd /d "%~dp0"

rem version: <VERSION>.<commit count>-<short commit>, plus -dev with uncommitted changes
set /p BASEVER=<VERSION
set BUILDNO=
set COMMIT=
set DIRTY=
for /f %%c in ('git rev-list --count HEAD 2^>nul') do set BUILDNO=%%c
for /f %%h in ('git rev-parse --short HEAD 2^>nul') do set COMMIT=%%h
for /f "delims=" %%d in ('git status --porcelain 2^>nul') do set DIRTY=-dev
if "%BUILDNO%"=="" (set APPVER=%BASEVER%-dev) else (set APPVER=%BASEVER%.%BUILDNO%-%COMMIT%%DIRTY%)
echo version %APPVER%

set FLAGS=-std=c++17 -O2 -s -static -municode -DUNICODE -D_UNICODE -D_WIN32_WINNT=0x0A00 -DWINVER=0x0A00 -DAPP_VERSION=\"%APPVER%\"
windres src\Chomp.rc -O coff -o Chomp_res.o || exit /b 1
g++ -specs=src/nomanifest.specs %FLAGS% -mwindows src\main.cpp Chomp_res.o -o Chomp.exe ^
    -lws2_32 -lcomctl32 -lole32 -luuid -lshell32 -lshlwapi -luxtheme -ldwmapi -lgdi32 -luser32 || exit /b 1
del Chomp_res.o
echo built Chomp.exe v%APPVER%
g++ %FLAGS% src\cli.cpp -o chomp-cli.exe -lws2_32 -lole32 -luuid -lshell32 || exit /b 1
echo built chomp-cli.exe v%APPVER%
g++ %FLAGS% src\cement-cli.cpp -o cement-cli.exe || exit /b 1
echo built cement-cli.exe (stub that runs chomp-cli.exe)
