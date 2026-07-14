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

No `Python` setup, no separate `yt-dlp` install, no manual POT provider setup, and no command-line guessing for routine downloads.

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

On macOS `ffmpeg` and `ffprobe` need to be installed separately with [Homebrew](https://brew.sh/):

```sh
brew install ffmpeg
```

### Linux

Use the package for your distribution from the latest release:

```sh
sudo apt install ./quiver_*.deb
```

```sh
sudo dnf install ./quiver-*.rpm
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

Run the Tauri app in development mode:

```sh
bun tauri dev
```

Build the frontend:

```sh
bun run build
```

Build the desktop app:

```sh
bun tauri build --no-sign
```

For Rust changes, also run:

```sh
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features
```

During local development, `QUIVER_YT_DLP_BINARY` can point Quiver at a custom `yt-dlp` binary. Release builds are intended to use the bundled sidecar.

## FAQ

### What does the `--cookies` flag do and how do I obtain the file?

I would advise you read through the yt-dlp wiki, especially [this section](https://github.com/yt-dlp/yt-dlp-wiki/blob/master/FAQ.md#how-do-i-pass-cookies-to-yt-dlp).

### Why do you need to download ffmpeg separately on macOS?

It is practically impossible to download verifiable static ffmpeg executables for macOS. While there is [this](https://evermeet.cx/ffmpeg/), the link to get the signatures are broken and they don't provide builds for macOS running on arm devices.

### Can I edit the Command Preview to customize it how I want?

Currently not. You can use the Input Fields to specify save directory and cookies file. Trying to support custom commands mixed in with the preview is a UX disaster, error prone when it comes to parsing on different OSs and generally a headache.

### Why is the installer file so big?

Quiver makes use of 6 (4 on macOS) individual binaries that are packaged alongside it to provide the capability it needs. They are built/fetched during build time, that way it is always verifiable from where a binary comes from.

### How to verify your builds?

All builds are built with Github actions for verifiable CI builds. Furthermore on Linux and Windows you can check the signatures. 

For Linux, you can download my public gpg key from [https://keys.openpgp.org/](https://keys.openpgp.org/search?q=dev%40velcore.net). 

For Windows, given that I self-signed the application, there is not really that much value in confirming my signature. If you still want to, you can download the public certificate called certificate.cer.

Then you'd first have to install certificate into your Trusted Root Certification Authorities ([Please understand the implications of this!](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/trusted-root-certification-authorities-certificate-store))

```pwsh
Import-Certificate -FilePath ".\certificate.cer" -CertStoreLocation Cert:\LocalMachine\Root   
```

And then you can check for the correct signature, by typing this command, the Status should display valid:

```pwsh
Get-AuthenticodeSignature -FilePath .\Quiver_x.y.z-setup.exe
```

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

Quiver makes use of 6 (on macOS 4) individual sidecars each with their own license (I linked the repo from where i get either the source code in the case of building them from source, or where I got the binaries from the release pipelines):
 - [bgutil-ytdlp-pot-provider](https://github.com/Brainicism/bgutil-ytdlp-pot-provider.git):  GPL-3.0 license
 - [Deno](https://github.com/denoland/deno/): MIT License
 - [ffmpeg and ffprobe](https://github.com/yt-dlp/FFmpeg-Builds): The Build Scripts are MIT License, but the compiled binary is made up of LGPL 2.1+ and GPL 3.0 code
 - [yt-dlp](https://github.com/yt-dlp/yt-dlp): Unlicense license
 - [YTSubConverter](https://github.com/arcusmaximus/YTSubConverter): MIT license

As for the Rust Crates and npm Packages used, they can be seen in src-tauri/Cargo.toml and package.json. For Rust I also provide a deeper Licenses Analysis inside the App with a 'Licenses' Button or alternatively the text for that can be looked up in src-tauri/license.html.