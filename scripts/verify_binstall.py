#!/usr/bin/env python3
"""Install a locally served signed release archive into an empty Cargo home."""  # noqa: EXE001

import argparse
import os
import ssl
import subprocess
import tempfile
import threading
import tomllib
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PACKAGE = tomllib.loads((ROOT / "Cargo.toml").read_text())
BINARIES = [entry["name"] for entry in PACKAGE["bin"]]


class QuietHandler(SimpleHTTPRequestHandler):
    def log_message(self, _format, *_args):  # ty: ignore[invalid-method-override]
        pass

    def copyfile(self, source, output):  # ty: ignore[invalid-method-override]
        try:
            super().copyfile(source, output)
        except (BrokenPipeError, ConnectionResetError):
            pass


def make_local_tls(directory):
    ca_config = directory / "ca.cnf"
    ca_config.write_text(
        "[req]\nprompt=no\ndistinguished_name=dn\nx509_extensions=v3_ca\n"
        "[dn]\nCN=Quiver local test CA\n"
        "[v3_ca]\nbasicConstraints=critical,CA:TRUE\nkeyUsage=keyCertSign,cRLSign\n"
    )
    server_config = directory / "server.cnf"
    server_config.write_text("[req]\nprompt=no\ndistinguished_name=dn\n[dn]\nCN=127.0.0.1\n")
    server_extensions = directory / "server.ext"
    server_extensions.write_text(
        "subjectAltName=IP:127.0.0.1\n"
        "basicConstraints=critical,CA:FALSE\n"
        "keyUsage=digitalSignature,keyEncipherment\n"
        "extendedKeyUsage=serverAuth\n"
    )
    ca_key = directory / "ca.key"
    ca_cert = directory / "ca.crt"
    server_key = directory / "server.key"
    server_request = directory / "server.csr"
    server_cert = directory / "server.crt"
    commands = [
        ["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", str(ca_key), "-out", str(ca_cert), "-days", "1", "-config", str(ca_config), "-extensions", "v3_ca"],
        ["openssl", "req", "-newkey", "rsa:2048", "-nodes", "-keyout", str(server_key), "-out", str(server_request), "-config", str(server_config)],
        ["openssl", "x509", "-req", "-in", str(server_request), "-CA", str(ca_cert), "-CAkey", str(ca_key), "-CAcreateserial", "-out", str(server_cert), "-days", "1", "-extfile", str(server_extensions)],
    ]
    for command in commands:
        subprocess.run(command, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return ca_cert, server_cert, server_key


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--public-key-file", type=Path)
    args = parser.parse_args()
    archive = args.archive.resolve()
    signature = Path(f"{archive}.sig")
    if not archive.is_file() or not signature.is_file():
        parser.error(f"archive and signature are required: {archive}, {signature}")
    suffix = ".exe" if "-windows-" in args.target else ""

    with tempfile.TemporaryDirectory(prefix="quiver-binstall-") as temp_name:
        temp = Path(temp_name)
        cargo_home = temp / "cargo"
        cargo_home.mkdir()
        install_dir = temp / "installed"
        install_dir.mkdir()
        manifest_path = ROOT / "Cargo.toml"
        if args.public_key_file:
            pubkey = args.public_key_file.read_text().splitlines()[-1].strip()
            manifest_dir = temp / "manifest"
            manifest_dir.mkdir()
            binary_declarations = "\n".join(
                f'[[bin]]\nname = "{name}"\npath = "{name}.rs"\n'
                for name in BINARIES
            )
            for name in BINARIES:
                (manifest_dir / f"{name}.rs").write_text("fn main() {}\n")
            manifest_path = manifest_dir / "Cargo.toml"
            manifest_path.write_text(
                f'[package]\nname = "quiver"\nversion = "{PACKAGE["package"]["version"]}"\n'
                'edition = "2024"\n'
                '[package.metadata.binstall]\npkg-fmt = "txz"\n'
                'bin-dir = "{ bin }{ binary-ext }"\n'
                'disabled-strategies = ["quick-install", "compile"]\n'
                '[package.metadata.binstall.signing]\nalgorithm = "minisign"\n'
                f'pubkey = "{pubkey}"\n{binary_declarations}'
            )
        ca_cert, server_cert, server_key = make_local_tls(temp)
        server = ThreadingHTTPServer(
            ("127.0.0.1", 0),
            partial(QuietHandler, directory=str(archive.parent)),
        )
        tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        tls.load_cert_chain(server_cert, server_key)
        server.socket = tls.wrap_socket(server.socket, server_side=True)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            url = f"https://127.0.0.1:{server.server_port}/{archive.name}"
            environment = os.environ.copy()
            environment["CARGO_HOME"] = str(cargo_home)
            environment["SSL_CERT_FILE"] = str(ca_cert)
            subprocess.run(
                [
                    "cargo-binstall",
                    "--manifest-path",
                    str(manifest_path),
                    "--pkg-url",
                    url,
                    "--targets",
                    args.target,
                    "--only-signed",
                    "--no-confirm",
                    "--disable-telemetry",
                    "--install-path",
                    str(install_dir),
                    "quiver",
                ],
                env=environment,
                check=True,
            )
        finally:
            server.shutdown()
            thread.join()

        actual = {path.name for path in install_dir.iterdir() if path.is_file()}
        expected = {f"{name}{suffix}" for name in BINARIES}
        if actual != expected:
            raise RuntimeError(f"installed binary set mismatch: {actual ^ expected}")
        run_checked(install_dir / f"quiver{suffix}", "--help")
        run_checked(install_dir / f"quiver-yt-dlp{suffix}", "--version")
        run_checked(install_dir / f"quiver-deno{suffix}", "--version")
        run_checked(install_dir / f"quiver-bgutil-pot{suffix}", "--help")
        if "-apple-" not in args.target:
            run_checked(install_dir / f"quiver-ffmpeg{suffix}", "-version")
            run_checked(install_dir / f"quiver-ffprobe{suffix}", "-version")
        if "-linux-" in args.target:
            run_checked(install_dir / f"quiver-ytsubconverter{suffix}", "--help")
        print(f"Verified signed installation of {len(expected)} binaries for {args.target}")


def run_checked(binary, argument):
    result = subprocess.run(
        [str(binary), argument], capture_output=True, text=True, timeout=20, check=False
    )
    if result.returncode:
        raise RuntimeError(f"{binary.name} failed: {result.stdout}{result.stderr}")


if __name__ == "__main__":
    main()
