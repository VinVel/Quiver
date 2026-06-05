# Quiver

Dead-simple `yt-dlp` downloads for people who do not want to build a command by hand.

| ![Quiver main screen](Screenshots/Screenshot%202026-06-05%20223411.png) | ![Quiver command preview](Screenshots/Screenshot%202026-06-05%20223511.png)            |
| ----------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| ![Quiver output view](Screenshots/Screenshot%202026-06-05%20223533.png) | ![Quiver advanced subtitle pipeline](Screenshots/Screenshot%202026-06-05%20223618.png) |

Quiver is a small desktop GUI for `yt-dlp`. Paste a link, choose the closest preset, press `Prepare`, then `Download`. It exists because `yt-dlp` is powerful, but the common path should not require memorizing format selectors, subtitle flags, cookie options, or YouTube workaround arguments.

## Features

- One-window download flow: URL, preset, output folder, optional cookies file, command preview, and live output.
- Eight built-in presets for YouTube/general downloads, audio/video, with/without cookies.
- YouTube-specific POT support through a bundled local `bgutil` provider.
- Bundled `yt-dlp`, `Deno`, POT provider, and `YTSubConverter`, with bundled `ffmpeg` and `ffprobe` for Windows and Linux release builds.
- Command preview before execution, so you can see exactly what Quiver will run.
- Live stdout/stderr output while `yt-dlp` is running.
- Persistent save directory, cookies path, theme mode, and color preset settings.
- Optional Advanced Subtitle Pipeline for YouTube video downloads, intended to make visual subtitles closer to YouTube's rendering.
- Light/dark mode, color themes, native file/folder pickers, and an in-app third-party license viewer.

## Why Quiver?

Most `yt-dlp` interfaces either expose a large settings surface or hide too much. Quiver takes the opposite approach: it turns the download patterns that I use into clear presets and keeps the rest visible in the command preview.

The default workflow is:

1. Paste a URL.
2. Pick audio or video.
3. Choose whether cookies are needed.
4. Prepare the command.
5. Download.

No `Python` setup, no separate `yt-dlp` install, no manual POT provider setup, and no command-line spelunking for routine downloads.

## Install

Download the latest release from:

https://github.com/VinVel/Quiver/releases/latest

### Windows

Download the Windows installer from the latest release and run it.

### macOS

Download the macOS build from the latest release, open it, and move Quiver into Applications if your package format asks you to.

The macOS `.dmg` is not signed or notarized. If macOS blocks it after download, remove the quarantine attribute before opening it:

```sh
xattr -d com.apple.quarantine ~/Downloads/Quiver-*.dmg
```

On macOS `ffmpeg` and `ffprobe` need to be installed seperately with [Homebrew](https://brew.sh/):

```sh
brew install ffmpeg
```

### Linux

Use the package for your distribution from the latest release:



```sh
sudo apt install ./quiver_*.deb
sudo dnf install ./quiver-*.rpm
```

Alternatively, download and run the AppImage from the same release page:

```sh
chmod +x Quiver-*.AppImage
./Quiver-*.AppImage
```

## Advanced Subtitle Pipeline

The Advanced Subtitle Pipeline is optional and only applies to YouTube video presets. It downloads YouTube `srv3` subtitles, converts them through `YTSubConverter`, and remuxes the result into the downloaded media. Windows and Linux release builds use bundled `ffmpeg` and `ffprobe`; macOS uses the tools installed through Homebrew.

## Development

Since this is a Tauri project, it is advisable to first checkout the [Tauri prerequisites](https://tauri.app/start/prerequisites/).

Other prerequisites: 
 - [Bun](https://bun.com/)
 - [.NET 8 (Linux/MacOS)](https://dotnet.microsoft.com/en-us/learn/dotnet/hello-world-tutorial/install)

This repo contains submodules, therefore you should clone the repo with the following command:

```sh
git clone https://github.com/VinVel/Quiver.git --recurse-submodules && cd ./Quiver
```

Install frontend dependencies:

```sh
bun install
```

Run the frontend dev server:

```sh
bun run dev
```

Run the full Tauri app:

```sh
bun tauri dev
```

Build the frontend:

```sh
bun run build
```

Build the desktop app:

```sh
bun tauri build
```

For Rust changes, also run:

```sh
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features
```

During local development, `QUIVER_YT_DLP_BINARY` can point Quiver at a custom `yt-dlp` binary. Release builds are intended to use the bundled sidecar.

## License

```
    Quiver, a simple YT-DLP GUI
    Copyright (C) 2026  VinVel


    This program is free software: you can redistribute it and/or modify
    it under the terms of the GNU General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    This program is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU General Public License for more details.

    You should have received a copy of the GNU General Public License
    along with this program. If not, see <https://www.gnu.org/licenses/>.
```
