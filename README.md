<p align="center">
  <img src="meta/readme.jpg" height="160" alt="Teto with a baguette">
</p>

<h1 align="center">Baguette</h1>

<p align="center">FTP, FTPS and SFTP client for Windows and Linux</p>

## Use it

Open `Baguette.exe`, add a site (or type an address in Quick connect) and drag files between your
PC and the server.

- Tabs, saved sites, drag and drop to and from Explorer or the Linux file manager, open and edit files in place
- Lite mode shows just the server's files (pick it at first launch or in the settings)
- Portable: settings stay next to `Baguette.exe`, passwords are encrypted for your Windows account
- On Linux it is one AppImage: settings and the passwords' key go to `~/.config/baguette`, deleted
  files to the trash
- An option saves `.bmp` pictures as `.png` when they arrive
- Updates itself from GitHub in one click when a new version is out

## With a 3DS (optional)

Run [**Cement**](https://github.com/Synaelle/cement) on the 3DS and connect to the address it shows.
Baguette then also shows the battery, the SD card, Send to PC and the CIAs to install, and
offers to update an older Cement.

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

Then run `build.bat`. It builds `Baguette.exe` and `baguette-cli.exe` in release mode
and copies them next to it. The version comes from the `VERSION` file, with `-<commit>-dev` added
when there are uncommitted changes.

On Linux, install Rust, `cargo install tauri-cli --version "^2" --locked` and the WebKitGTK headers
(Debian/Ubuntu: `libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libxdo-dev patchelf file`), then run
`build.sh`. It builds `Baguette-x86_64.AppImage` and `baguette-cli` the same way. It also works from WSL.

## Planned

- Tools for PS3, Xbox 360 (Aurora) and Switch
- Android
- Custom skins
- Plugins, shared with Cement

## Contributing

Contributions are welcome, as long as they stay within what Baguette is for and don't break how
Baguette and Cement talk to each other. A change to that protocol needs to be made in both apps.

## License

AGPL-3.0 or later, see [LICENSE](LICENSE).
