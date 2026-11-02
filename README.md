<p align="center">
  <img src="meta/readme.jpg" height="160" alt="Teto with a baguette">
</p>

<h1 align="center">Baguette</h1>

<p align="center">FTP, FTPS and SFTP client for Windows and Linux</p>

## Use it

Download the latest release. On Windows open `Baguette.exe`, on Linux run `Baguette-x86_64.AppImage`.
Add a site or type an address in Quick connect, then drag files between your PC and the server.

- Tabs, saved sites, drag and drop with your file manager, files opened and edited in place
- Lite mode shows only the server's files
- Portable on Windows, with settings next to `Baguette.exe` and passwords encrypted for your account
- One AppImage on Linux, with settings in `~/.config/baguette`
- Updates itself in one click when a new version is out

## With a 3DS

Run [**Cement**](https://github.com/Synaelle/cement) on the 3DS and connect to the address it shows.
Baguette then also shows the battery, the SD card, Send to PC and the CIAs to install, and offers to
update an older Cement.

`baguette-cli` does the same from scripts. Run `baguette-cli --help` to see its commands. On Windows,
`add-to-path.bat` lets you type `baguette` in any terminal.

## Build

Install [Rust](https://rustup.rs), then run the script for your system. It builds both programs and
copies them next to it.

- On Windows, add the [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
  with "Desktop development with C++" and run `build.bat`.
- On Linux or WSL, add `cargo install tauri-cli --version "^2" --locked` and the WebKitGTK headers
  (`libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libxdo-dev patchelf file` on Debian or Ubuntu)
  and run `build.sh`.

The version comes from `VERSION`, with `-<commit>-dev` added when there are uncommitted changes.

## Planned

Tools for PS3, Xbox 360 (Aurora) and Switch, Android, custom skins, and plugins shared with Cement.

## Contributing

Contributions are welcome if they fit what Baguette is for. A change to how Baguette talks to Cement
has to be made in both apps.

## License

AGPL-3.0 or later, see [LICENSE](LICENSE).
