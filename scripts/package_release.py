#!/usr/bin/env python3
"""Assemble and verify one cargo-binstall archive from Cargo's sidecar manifest."""  # noqa: EXE001

import argparse
import hashlib
import json
import os
import tarfile
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MANIFEST = tomllib.loads((ROOT / "Cargo.toml").read_text())
VERSION = MANIFEST["package"]["version"]
BINARIES = {binary["name"] for binary in MANIFEST["bin"]}


def digest(path):
    hasher = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            hasher.update(chunk)
    return hasher.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True)
    parser.add_argument("--profile", choices=("debug", "release"), default="release")
    parser.add_argument("--output-dir", type=Path, default=ROOT / "dist")
    args = parser.parse_args()

    target_dir = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    if not target_dir.is_absolute():
        target_dir = ROOT / target_dir
    build_dir = target_dir / args.target / args.profile
    suffix = ".exe" if "-windows-" in args.target else ""
    candidates = sorted(
        build_dir.glob("build/quiver-*/out/sidecar-manifest.json"),
        key=lambda path: path.stat().st_mtime,
        reverse=True,
    )
    entries = None
    for candidate in candidates:
        candidate_data = json.loads(candidate.read_text())
        if candidate_data["target"] == args.target:
            entries = candidate_data["sidecars"]
            break
    if entries is None:
        parser.error(f"no sidecar manifest for {args.target} under {build_dir}")

    files = {"quiver": build_dir / f"quiver{suffix}"}
    for entry in entries:
        name = entry["bin"]
        if name in files:
            parser.error(f"duplicate sidecar {name}")
        if entry["source"] == "staged":
            path = Path(entry["path"])
            if not path.is_file() or digest(path) != entry["sha256"]:
                parser.error(f"missing or modified staged sidecar: {path}")
        elif entry["source"] == "cargo-bin":
            path = build_dir / f"{name}{suffix}"
        else:
            parser.error(f"unknown sidecar source for {name}: {entry['source']}")
        files[name] = path

    if files.keys() != BINARIES:
        parser.error(f"binary set mismatch: missing {BINARIES - files.keys()}, extra {files.keys() - BINARIES}")
    for name, path in files.items():
        if not path.is_file():
            parser.error(f"missing {name} executable: {path}")

    args.output_dir.mkdir(parents=True, exist_ok=True)
    debug_suffix = "-debug" if args.profile == "debug" else ""
    archive = args.output_dir / f"quiver-{VERSION}-{args.target}{debug_suffix}.txz"
    with tarfile.open(archive, "w:xz") as package:
        for name, path in sorted(files.items()):
            info = tarfile.TarInfo(f"{name}{suffix}")
            info.size = path.stat().st_size
            info.mode = 0o755
            with path.open("rb") as source:
                package.addfile(info, source)

    with tarfile.open(archive, "r:xz") as package:
        actual = {member.name for member in package.getmembers()}
        expected = {f"{name}{suffix}" for name in files}
        if actual != expected:
            parser.error(f"archive contents mismatch: {actual ^ expected}")
    print(archive)


if __name__ == "__main__":
    main()
