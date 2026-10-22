<p align="center">
  <img src="meta/readme.jpg" height="160" alt="Teto with a baguette">
</p>

<h1 align="center">Baguette</h1>

<p align="center">A cute little FTP, FTPS and SFTP client for Windows 🥖</p>

> The names and the design are a work in progress, nothing here is final yet.

## Use it

Open `Baguette.exe`, add a site (or type an address in Quick connect) and drag files between your
PC and the server.

- Tabs, saved sites, drag and drop to and from Explorer, open and edit files in place
- `--simple` opens a window with just the server's files
- Portable: settings stay next to `Baguette.exe`, passwords are encrypted for your Windows account
- An option saves `.bmp` pictures as `.png` when they arrive

## With a 3DS (optional)

Run [**Cement**](https://github.com/Synaelle/cement) on the 3DS and connect to the address it shows.
Baguette then also shows the battery, the SD card, Send to PC and the CIAs to install.

`baguette-cli.exe` talks to Cement from scripts:

```
baguette-cli status                          battery, SD space, busy or not
baguette-cli put game/ sd:/3ds/app           upload
baguette-cli --png get sd:/luma/screenshots  download, BMP saved as PNG
baguette-cli --help                          everything else
```

`add-to-path.bat` lets you type `baguette` in any terminal.

## Build

On Windows 10 or 11, install:

- [Rust](https://rustup.rs) (rustup picks the stable MSVC toolchain from `rust-toolchain.toml`)
- [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with
  "Desktop development with C++"

Then run `build.bat` in this folder. It builds `Baguette.exe` and `baguette-cli.exe` in release mode
and copies them next to it. The version comes from the `VERSION` file, with `-<commit>-dev` added
when there are uncommitted changes.

## Planned

- Updating from within the app
- Tools for PS3, Xbox 360 (Aurora) and Switch
- Full support for macOS, Linux and Android

iOS isn't planned until I find a tester with an iPhone.

## Contributing

Contributions are welcome, as long as they stay within what Baguette is for and don't break how
Baguette and Cement talk to each other.

## License

GPL-3.0 or later, see [LICENSE](LICENSE).
