<p align="center">
  <img src="icon.png" width="128" alt="Chomp">
</p>

<h1 align="center">Chomp</h1>

<p align="center">A cute little Windows explorer for your 3DS, over Wi-Fi 🍪</p>

Chomp is mainly here to make my workflow faster while I develop 3DS homebrew apps: build on
the PC, push the new `.3dsx` or CIA in one command, pull the logs back, repeat.

## Use it

1. Run [**Cement**](https://github.com/Synaelle/cement) on your 3DS
2. Open `Chomp.exe` and type the address Cement shows (asked only once)

```
chomp-cli status                   is the 3DS there? busy? SD space, battery
chomp-cli ls sd:/3ds               list a folder
chomp-cli put game/ sd:/3ds/app    upload files and folders
chomp-cli get sd:/luma/screenshots shots
chomp-cli --help                   everything else
```

`cement-cli.exe` still works for now: it's a stub that runs `chomp-cli.exe` and will be
removed in a later build.

## Chomp + Cement

Chomp is made for [Cement](https://github.com/Synaelle/cement), the FTP server app for the
3DS. It talks plain FTP plus a few Cement extras (`SITE STATUS`, `SITE BATCH`) for progress,
time left, free space and the battery. Cement works on its own with any FTP client, and Chomp
is just the comfy way to use it. Windows 10/11 only for now.

## Build

Chomp is written in Rust, with its window drawn by WebView2 (built into Windows 10 and 11).
With Rust (MSVC toolchain) and the Visual Studio Build Tools:

```
build.bat
```

Chomp is portable: it keeps its settings (`chomp.ini`) and temp files next to `Chomp.exe`.

## License

GPL-3.0 or later, see [LICENSE](LICENSE). Versions before 1.0.21 were MIT.
