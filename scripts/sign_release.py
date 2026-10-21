#!/usr/bin/env python3
"""Sign a release archive and verify it against Quiver's published key."""  # noqa: EXE001

import argparse
import os
import subprocess
import tempfile
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PUBKEY = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["metadata"]["binstall"]["signing"]["pubkey"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--secret-key-file", type=Path)
    args = parser.parse_args()
    archive = args.archive.resolve()
    if not archive.is_file():
        parser.error(f"archive is missing: {archive}")
    signature = Path(f"{archive}.sig")

    if args.secret_key_file:
        secret_key = args.secret_key_file.resolve()
        if not secret_key.is_file():
            parser.error(f"secret key is missing: {secret_key}")
        sign(secret_key, archive, signature)
    else:
        value = os.environ.get("QUIVER_MINISIGN_SECRET_KEY")
        if not value:
            parser.error("QUIVER_MINISIGN_SECRET_KEY is required")
        with tempfile.TemporaryDirectory(prefix="quiver-sign-") as directory:
            secret_key = Path(directory) / "release.key"
            secret_key.write_text(value)
            secret_key.chmod(0o600)
            sign(secret_key, archive, signature)

    subprocess.run(
        ["rsign", "verify", "-q", "-P", PUBKEY, "-x", str(signature), str(archive)],
        check=True,
    )
    print(signature)


def sign(secret_key, archive, signature):
    subprocess.run(
        ["rsign", "sign", "-W", "-s", str(secret_key), "-x", str(signature), str(archive)],
        check=True,
    )


if __name__ == "__main__":
    main()
