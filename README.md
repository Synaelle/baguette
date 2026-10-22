<p align="center">
  <img src="meta/readme.jpg" height="160" alt="Teto with a baguette">
</p>

<h1 align="center">Baguette</h1>

<p align="center">A cute little file transfer app for Windows, with extra love for the 3DS 🥖</p>

Baguette started as a way to make my workflow faster while I develop 3DS homebrew apps: build on
the PC, push the new `.3dsx` or CIA in one command, pull the logs back, repeat. It now speaks
FTP, FTPS and SFTP too, so the same window works for the Pi, the NAS or a server.

> The names and the design are a work in progress, nothing here is final yet.

## Use it

1. Open `Baguette.exe`, add a site and connect
2. For a 3DS, run [**Cement**](https://github.com/Synaelle/cement) and add a site with the
   address it shows. The 3DS tools appear on the right once it's connected

The command line tool is only for the 3DS:

```
baguette-cli status                   is the 3DS there? busy? SD space, battery
baguette-cli ls sd:/3ds               list a folder
baguette-cli put game/ sd:/3ds/app    upload files and folders
baguette-cli get sd:/luma/screenshots shots
baguette-cli --help                   everything else
```

To type `baguette` instead of the full path in any terminal, run `add-to-path.bat` once
(`add-to-path.bat remove` undoes it).

Baguette was called Chomp until 2.1: scripts that ran `chomp-cli.exe` or `cement-cli.exe` now run
`baguette-cli.exe`, with the same commands. `chomp.ini` is picked up as `baguette.ini` the first time.

## Baguette + Cement

[Cement](https://github.com/Synaelle/cement) is the FTP server app for the 3DS. With it,
Baguette shows the battery, the SD card space, Send to PC and the CIAs waiting to be installed
(and installs them for you),
and uses a few Cement extras (`SITE STATUS`, `SITE BATCH`) for progress and time left. Cement
works on its own with any FTP client, and Baguette is just the comfy way to use it. Baguette can also
give the 3DS a login of your choice, and, as an experiment in the settings, send many small
files as one bundle that Cement unpacks. Windows 10/11
only for now.

## Build

Baguette is written in Rust, with its window drawn by WebView2 (built into Windows 10 and 11).
With Rust (MSVC toolchain) and the Visual Studio Build Tools:

```
build.bat
```

Baguette is portable: it keeps its settings and sites (`baguette.ini`) and temp files next to
`Baguette.exe`.
Start it with `--simple` for a window with just the server's files, as a list or as tiles.
Baguette only reads the Windows folder, never changes it (start it with `--allow-windows-folder`
if you really need to).
Saved passwords are encrypted for your Windows account, so on another PC a site
just asks for its password again.

The Fredoka and Nunito fonts are under the SIL Open Font License (see `app/ui/fonts`).

## Planned

Later:

- Updating from within the app (for now, Baguette tells you when a new version is out)
- Tools for other consoles: PS3, Xbox 360 (with Aurora) and Switch
- Full support for macOS, Linux and Android

## License

GPL-3.0 or later, see [LICENSE](LICENSE). Versions before 1.0.21 were MIT.
