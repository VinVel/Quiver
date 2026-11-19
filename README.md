# Quiver

Quiver is a terminal interface for `yt-dlp`. Paste a URL, choose one of eight
presets, review the generated command, and run the download. It keeps the
common format, cookie, subtitle, and YouTube POT options available without
requiring you to write a `yt-dlp` command by hand.

## Features

- Eight presets: YouTube or general sites, video or audio, with or without cookies.
- Editable save directory and cookies file, with saved paths reused between runs.
- Automatic command preview, streaming output, and download cancellation.
- Optional advanced subtitle conversion for YouTube video downloads.
- Four color themes, light and dark modes, help, and third-party license notices.
- Bundled `yt-dlp`, Deno, POT provider, and YTSubConverter sidecars. Windows and
  Linux archives also contain FFmpeg and ffprobe. macOS uses Homebrew FFmpeg.

## Install or upgrade

Install [Rust and Cargo](https://rustup.rs/) and
[cargo-binstall](https://github.com/cargo-bins/cargo-binstall), then run the same
command on Windows, macOS, and Linux:

```sh
cargo binstall --git https://github.com/VinVel/Quiver --only-signed quiver
```

The command installs Quiver and all six sidecars from a signed release archive.
Run it again to upgrade. The first TUI release must be published before this
command can install the new app; the existing `v0.2.1` release contains the old
desktop packages.

On macOS, install FFmpeg and ffprobe with Homebrew as well:

```sh
brew install ffmpeg
```

Windows and Linux archives include both tools, so they do not need a separate
system installation. 

## Use

Run `quiver` in a terminal. Paste a URL, pick a preset, adjust the save directory
and cookies file if needed, then press Enter to prepare and Ctrl+R to run.
The command preview updates when inputs change. Press F1 for the full key list.

| Key | Action |
| --- | --- |
| Tab / Shift+Tab | Move between fields and panels |
| F2 / F3 | Edit save directory / cookies file |
| Ctrl+↑ / Ctrl+↓ | Change preset |
| Ctrl+← / Ctrl+→ | Change color theme |
| Ctrl+Space | Toggle advanced subtitles |
| Ctrl+P | Prepare the command |
| Ctrl+R | Run the previewed command |
| Ctrl+X | Cancel the current download |
| Ctrl+L | View third-party licenses |
| Ctrl+C | Quit |

The advanced subtitle pipeline applies to YouTube video presets. It downloads
YouTube `srv3` subtitles, converts them with YTSubConverter, then remuxes the
result using FFmpeg. See `quiver --help` for command-line options.

## Build from source

Clone the repository with its submodules:

```sh
git clone --recurse-submodules https://github.com/VinVel/Quiver.git
cd Quiver
```

Building requires stable Rust, [uv](https://docs.astral.sh/uv/), Node.js as
specified in `.node-version`, npm, and the .NET 10 SDK for YTSubConverter.
On Linux, also install a C/C++ toolchain, OpenSSL development headers, and
`pkg-config`. uv uses `.python-version` (Python 3.11) for the yt-dlp build.
The build script prepares the sidecars; it can download upstream tools.

```sh
cargo build --locked --bins
cargo run --locked --bin quiver
```

For local development, `QUIVER_YT_DLP_BINARY` can point to a custom yt-dlp
executable. A normal installation resolves sidecars beside the `quiver`
executable.

Run the project checks with:

```sh
cargo fmt --all -- --check
cargo check --locked --all-targets
cargo test --locked
cargo clippy --locked --all-targets --all-features
```

For archive creation, signing, and GitHub release steps, see
[the release guide](docs/releasing.md).

## Licenses

Quiver is licensed under GPL-3.0-or-later; see [LICENSE](LICENSE). The TUI's
license viewer displays [Rust crate license notices](licenses/third-party.html).
The build uses the [yt-dlp](https://github.com/yt-dlp/yt-dlp),
[bgutil POT provider](https://github.com/Brainicism/bgutil-ytdlp-pot-provider),
and [YTSubConverter](https://github.com/arcusmaximus/YTSubConverter) submodules.
Release archives also contain Deno and, on Windows and Linux, FFmpeg and
ffprobe. Each tool retains its own license.
