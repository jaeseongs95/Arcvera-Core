# Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
# Modified for OpenDIY Codex R33 (2026-11-02); see MODIFICATIONS.md.
"""Prepare a separate Windows CLI package; never replace an existing installation."""

import argparse
import hashlib
import json
import re
from pathlib import Path
import shutil
import subprocess


def digest(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def checked_child(root, relative):
    relative = Path(relative)
    child = (root / relative).resolve(strict=True)
    if relative.is_absolute() or not child.is_relative_to(root) or not child.is_file():
        raise ValueError(f"Not a package file: {relative}")
    return child


def semver(version):
    pattern = r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
    match = re.fullmatch(pattern, version)
    if not match or (match[4] and any(part.isdigit() and len(part) > 1 and part[0] == "0"
                                    for part in match[4].split("."))):
        raise ValueError(f"Expected a SemVer version: {version}")
    return version


def package(source, freeze_file, binary, vendor, output, expected_version, *,
            upstream_version=None, app_server=None, code_mode_host=None):
    semver(expected_version)
    triple = app_server is not None or code_mode_host is not None
    if triple and (app_server is None or code_mode_host is None):
        raise ValueError("Triple packaging requires both app-server and code-mode-host candidates")
    if triple and upstream_version is None:
        raise ValueError("Triple packaging requires explicit upstream_version")
    # Preserve legacy callers while keeping a fork's SemVer independent of official resources.
    upstream_version = semver(upstream_version or expected_version.split("+", 1)[0])
    source, vendor = source.resolve(strict=True), vendor.resolve(strict=True)
    binary, freeze_file = binary.resolve(strict=True), freeze_file.resolve(strict=True)
    output = output.resolve()
    if output.exists() or output.is_relative_to(vendor):
        raise ValueError("Output must be a new directory outside the vendor package")
    freeze = json.loads(freeze_file.read_text(encoding="utf-8"))
    commit = subprocess.check_output(
        ["git", "-C", str(source), "rev-parse", "HEAD"], text=True
    ).strip()
    if commit != freeze["candidateCommit"]:
        raise ValueError("Source HEAD differs from the frozen candidate")
    frozen = set()
    for entry in freeze["files"]:
        relative = entry["path"]
        if relative in frozen:
            raise ValueError(f"Duplicate source freeze entry: {relative}")
        frozen.add(relative)
        if digest(checked_child(source, entry["path"])) != entry["sha256"]:
            raise ValueError(f"Source changed after freeze: {entry['path']}")
    scope = ["--", ".", ":!codex-rs/Microsoft/**"]
    tracked = set(filter(None, subprocess.check_output(
        ["git", "-C", str(source), "ls-files", "--cached", "-z", *scope]
    ).decode("utf-8").split("\0")))
    if frozen != tracked:
        raise ValueError("Source file set differs from the complete frozen tracked set")
    untracked = subprocess.check_output(
        ["git", "-C", str(source), "ls-files", "--others", "--exclude-standard", "-z", *scope]
    )
    if untracked:
        raise ValueError("Source contains untracked files outside the frozen commit")
    dirty = subprocess.run(["git", "-C", str(source), "diff", "--quiet", "HEAD", *scope])
    if dirty.returncode:
        raise ValueError(f"Source differs from its commit (git diff exit {dirty.returncode})")
    metadata = json.loads((vendor / "codex-package.json").read_text(encoding="utf-8"))
    if metadata.get("entrypoint") != "bin/codex.exe":
        raise ValueError("Expected the official Windows CLI package layout")
    if metadata.get("version") != upstream_version:
        raise ValueError("Official helper package differs from the CLI base version")
    candidates = {"cli": (binary, "bin/codex.exe", "codex-cli")}
    if triple:
        candidates.update({
            "app-server": (app_server.resolve(strict=True), "bin/codex-app-server.exe", "codex-app-server"),
            "code-mode-host": (code_mode_host.resolve(strict=True), "bin/codex-code-mode-host.exe", "codex-code-mode-host"),
        })
    attestations = {}
    for name, (candidate, relative, prefix) in candidates.items():
        candidate_hash = digest(candidate)
        if triple:
            manifest = freeze.get("executables")
            entry = manifest.get(name) if isinstance(manifest, dict) else None
            if not isinstance(entry, dict):
                raise ValueError(f"Missing frozen executable entry: {name}")
            if entry.get("sourceCommit") != commit:
                raise ValueError(f"Executable source commit differs from frozen candidate: {name}")
            if entry.get("sha256") != candidate_hash:
                raise ValueError(f"Executable hash differs from frozen candidate: {name}")
        observed = subprocess.check_output([str(candidate), "--version"], text=True, timeout=30).strip()
        if observed != f"{prefix} {expected_version}":
            label = "CLI" if name == "cli" else name
            raise ValueError(f"Unexpected {label} version: {observed}")
        if digest(candidate) != candidate_hash:
            raise ValueError(f"Executable changed during version check: {name}")
        attestations[name] = {"path": relative, "sourceCommit": commit,
                              "sha256": candidate_hash, "version": observed}
    binary_hash = attestations["cli"]["sha256"]
    # Check before creating output: a failed version check cannot look installed.
    version = attestations["cli"]["version"]
    replacements = {entry["path"] for entry in attestations.values()}
    helpers = []
    for item in sorted(vendor.rglob("*")):
        if not item.is_file():
            continue
        relative = item.relative_to(vendor)
        if relative.as_posix() in replacements | {"codex-package.json"}:
            continue
        helpers.append((relative, checked_child(vendor, relative), digest(item)))
    output.mkdir(parents=True)
    for relative, item, expected_hash in helpers:
        target = output / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(item, target)
        if digest(target) != expected_hash:
            raise ValueError(f"Helper changed during copy: {relative}")
    for name, (candidate, relative, _prefix) in candidates.items():
        target = output / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(candidate, target)
        if digest(target) != attestations[name]["sha256"]:
            raise ValueError(f"Executable changed during copy: {name}")
        if triple:
            observed = subprocess.check_output([str(target), "--version"], text=True, timeout=30).strip()
            if observed != attestations[name]["version"] or digest(target) != attestations[name]["sha256"]:
                raise ValueError(f"Packaged executable identity changed: {name}")
    metadata["version"] = expected_version
    metadata["opendiyUpdate"] = {"schema": 1, "minLauncher": 1}
    (output / "codex-package.json").write_text(
        json.dumps(metadata, indent=2) + "\n", encoding="utf-8", newline="\n"
    )
    (output / "codex-patched.cmd").write_text(
        '@echo off\n"%~dp0bin\\codex.exe" %*\nexit /b %errorlevel%\n',
        encoding="utf-8", newline="\r\n",
    )
    (output / "codex-local.cmd").write_text(
        '@echo off\n"%~dp0bin\\codex.exe" --no-daemon %*\nexit /b %errorlevel%\n',
        encoding="utf-8", newline="\r\n",
    )
    # Written last. Presence records a complete copy, not audit or runtime success.
    receipt = {
        "sourceCommit": commit,
        "sourceFreezeSha256": digest(freeze_file),
        "binarySha256": binary_hash,
        "version": version,
        "upstreamVersion": upstream_version,
        "executables": attestations,
        "files": [
            {"path": item.relative_to(output).as_posix(), "sha256": digest(item)}
            for item in sorted(output.rglob("*")) if item.is_file()
        ],
    }
    (output / "package-receipt.json").write_text(
        json.dumps(receipt, indent=2) + "\n", encoding="utf-8", newline="\n"
    )
    return receipt


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("source", "freeze", "binary", "vendor", "output"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--upstream-version")
    parser.add_argument("--app-server", type=Path)
    parser.add_argument("--code-mode-host", type=Path)
    args = parser.parse_args()
    result = package(args.source, args.freeze, args.binary, args.vendor, args.output, args.version,
                     upstream_version=args.upstream_version, app_server=args.app_server,
                     code_mode_host=args.code_mode_host)
    print(json.dumps({"sourceCommit": result["sourceCommit"], "version": result["version"]}))
