# TUI release archives

The six native GitHub Actions jobs build Quiver and its sidecars on their matching
Windows, macOS, or GNU/Linux runner. Each job creates a root-level `.txz` archive,
signs it with Minisign-compatible `rsign`, verifies a signed cargo-binstall
installation into an empty Cargo home, and uploads the archive and `.sig` to a
draft GitHub Release.
uv supplies Python 3.11 from `.python-version` for the build and packaging scripts.

Before the first TUI release, add the complete contents of the locally generated,
ignored `.secrets/quiver-release.key` file as the repository Actions secret
`QUIVER_MINISIGN_SECRET_KEY`. Back up that key outside the repository. The public
key is already in the root `Cargo.toml`; replacing the private key requires
updating that public key before publishing another release. The workflow fails
closed if the secret is missing or does not match.

For every release, set the root Cargo version to the new tag version and add
`changelog/vVERSION.md`. Push the corresponding `vVERSION` tag. After all six
archives have uploaded and passed validation, publish the draft release.
When Rust dependencies change, regenerate `licenses/third-party.html` with
`cargo about generate --locked --config licenses/about.toml --output-file licenses/third-party.html licenses/about.hbs` before tagging.
Keep the root Cargo version equal to the newest published release so the
unversioned installation command resolves an available archive:

```sh
cargo binstall --git https://github.com/VinVel/Quiver --only-signed quiver
```

The same command upgrades Quiver when the root manifest points to a newer
published release. Cargo and cargo-binstall are prerequisites; macOS media
processing also requires Homebrew FFmpeg and ffprobe.

For a local native release check:

```sh
cargo build --release --locked --target "$(rustc -vV | sed -n 's/^host: //p')" --bins
uv run --no-project python scripts/package_release.py --target "$(rustc -vV | sed -n 's/^host: //p')"
```

The release workflow signs and tests the archive. CI uses a fresh temporary
signing key to exercise signature verification on pull requests without exposing
the production key.
