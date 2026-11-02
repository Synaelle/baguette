@echo off
rem Lets you type "baguette" anywhere to run baguette-cli.exe.
rem It writes bin\baguette.bat next to this file (it runs ..\baguette-cli.exe) and adds
rem that bin folder to your user PATH. The Baguette folder itself is not added:
rem "baguette" would open Baguette.exe instead.
rem   add-to-path.bat          adds it
rem   add-to-path.bat remove   takes it out again (bin\ stays, delete it if you like)
rem Open a new terminal afterwards: running ones keep their old PATH.
setlocal
set "BIN=%~dp0bin"
rem tests only change this window's copy of PATH, never your saved one
set "SCOPE=User"
if defined BAGUETTE_PATH_TEST set "SCOPE=Process"

if /i "%~1"=="remove" goto remove

if not exist "%~dp0baguette-cli.exe" (
    echo baguette-cli.exe isn't next to this script: run build.bat first, or keep them together.
    exit /b 1
)
if not exist "%BIN%" mkdir "%BIN%" || exit /b 1
rem chomp.bat: what this script wrote before Baguette was renamed
if exist "%BIN%\chomp.bat" del "%BIN%\chomp.bat"
> "%BIN%\baguette.bat" (
    echo @echo off
    echo rem baguette = baguette-cli.exe, made by add-to-path.bat
    echo "%%~dp0..\baguette-cli.exe" %%*
)
powershell -NoProfile -ExecutionPolicy Bypass -Command ^
  "$bin = $env:BIN; $old = [Environment]::GetEnvironmentVariable('Path', $env:SCOPE); $parts = @($old -split ';' | Where-Object { $_ });" ^
  "if ($parts -contains $bin) { Write-Host 'baguette is already on your PATH:' $bin; exit 0 }" ^
  "[Environment]::SetEnvironmentVariable('Path', (($parts + $bin) -join ';'), $env:SCOPE);" ^
  "Write-Host 'Added to your PATH:' $bin; Write-Host 'Open a new terminal and type: baguette --help'"
exit /b %errorlevel%

:remove
powershell -NoProfile -ExecutionPolicy Bypass -Command ^
  "$bin = $env:BIN; $old = [Environment]::GetEnvironmentVariable('Path', $env:SCOPE); $parts = @($old -split ';' | Where-Object { $_ });" ^
  "if ($parts -notcontains $bin) { Write-Host 'baguette was not on your PATH'; exit 0 }" ^
  "[Environment]::SetEnvironmentVariable('Path', (($parts | Where-Object { $_ -ne $bin }) -join ';'), $env:SCOPE);" ^
  "Write-Host 'Removed from your PATH:' $bin"
exit /b %errorlevel%
