#!/usr/bin/env python3
"""Require a release tag and changelog matching the Cargo package version."""  # noqa: EXE001

import sys
import tomllib
from pathlib import Path

root = Path(__file__).resolve().parent.parent
version = tomllib.loads((root / "Cargo.toml").read_text())["package"]["version"]
expected = f"v{version}"
actual = sys.argv[1] if len(sys.argv) == 2 else ""
if actual != expected:
    raise SystemExit(f"release tag {actual!r} does not match Cargo version {expected!r}")
changelog = root / "changelog" / f"{actual}.md"
if not changelog.is_file():
    raise SystemExit(f"missing release changelog: {changelog}")
print(f"Release tag and changelog match Cargo version {version}")
