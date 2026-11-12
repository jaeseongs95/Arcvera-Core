# Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
# Modified for OpenDIY Codex R33 (2026-11-02); see MODIFICATIONS.md.
"""Prepare a separate Windows CLI package; never replace an existing installation."""

import argparse
import io
import hashlib
import json
import re
from pathlib import Path
import shutil
import subprocess
import zipfile

AGPL_SHA256 = '0d96a4ff68ad6d4b6f1f30f713b18d5184912ba8dd389f86aa7710db079abcb0'
LICENSE_FILES = ('LICENSE', 'LICENSE-APACHE-2.0', 'NOTICE', 'LICENSING.md',
                 'MODIFICATIONS.md', 'upstream/LICENSE', 'upstream/NOTICE')


def licensing_payload(source, commit, frozen):
    """Return notices and exact committed source; legacy Apache callers stay supported."""
    if not any((source/name).exists() for name in ('LICENSE-APACHE-2.0', 'LICENSING.md')):
        return None
    notices = {name: checked_child(source, name).read_bytes() for name in LICENSE_FILES}
    if hashlib.sha256(notices['LICENSE']).hexdigest() != AGPL_SHA256:
        raise ValueError('AGPL license text differs from the complete pinned official text')
    # Git archive applies checkout conversion; native Windows EOL must not alter blobs.
    archive = subprocess.check_output(['git', '-C', str(source), '-c', 'core.autocrlf=false',
                                       '-c', 'core.eol=lf', 'archive', '--format=zip', commit])
    # Detect export-ignore/export-subst, omitted inputs or an altered Git archive.
    entries = {}
    for row in subprocess.check_output(['git', '-C', str(source), 'ls-tree', '-r', '-z', commit]).split(b'\0'):
        if row:
            metadata, name = row.split(b'\t', 1)
            mode, kind, oid = metadata.decode().split()
            if kind != 'blob' or mode not in ('100644', '100755', '120000'):
                raise ValueError('Unsupported corresponding-source Git entry')
            entries[name.decode('utf-8')] = oid
    with zipfile.ZipFile(io.BytesIO(archive)) as zipped:
        files = [item for item in zipped.infolist() if not item.is_dir()]
        if len(files) != len(entries) or {item.filename for item in files} != frozen or set(entries) != frozen:
            raise ValueError('Corresponding-source archive differs from complete frozen Git tree')
        for item in files:
            data = zipped.read(item)
            oid = hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).hexdigest()
            if oid != entries[item.filename]:
                raise ValueError(f'Corresponding-source Git blob differs: {item.filename}')
        if any(zipped.read(name) != data for name, data in notices.items()):
            raise ValueError('License/notice bytes differ from committed corresponding source')
    return notices, archive


def source_offer(commit):
    return (f'# Corresponding source / 대응소스\n\n'
            f'Arcvera-Core: AGPL-3.0-or-later. Source commit / 소스 커밋: `{commit}`.\n\n'
            'The exact project source is included at [source/arcvera-core-source.zip](source/arcvera-core-source.zip). '
            'Extract it and consult `docs/source-import.md`, `tools/windows-cli/README.md` and `LICENSING.md` for build, packaging and redistribution requirements. '
            f'Public source: <https://github.com/jaeseongs95/Arcvera-Core/tree/{commit}>. '
            'Original Apache and third-party terms remain applicable; see `licenses/arcvera-core/`.\n\n'
            '정확한 프로젝트 소스는 위 압축파일에 포함됩니다. 압축을 풀고 위 문서의 빌드·패키징·재배포 조건을 확인하십시오. '
            '원 Apache·제삼자 라이선스와 고지는 `licenses/arcvera-core/` 및 소스 안에 유지됩니다.\n\n'
            'Network operators must prominently offer remote users their deployed version\'s corresponding source at no charge in the interface they use (AGPL section 13). '
            '서비스 운영자는 원격 사용자가 실제로 이용하는 인터페이스에 운영 버전의 대응소스를 무료로 받을 수 있다는 안내를 잘 보이게 제공해야 합니다(AGPL 제13조).\n')


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
    licensing = licensing_payload(source, commit, frozen)
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
    if licensing:
        for relative, _item, _hash in helpers:
            path = relative.as_posix().casefold()
            if any(path == reserved or path.startswith(reserved+'/') or reserved.startswith(path+'/')
                   for reserved in ('source', 'source.md', 'licenses/arcvera-core')):
                raise ValueError(f'Vendor file collides with source/license delivery: {relative}')
    output.mkdir(parents=True)
    for relative, item, expected_hash in helpers:
        target = output / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(item, target)
        if digest(target) != expected_hash:
            raise ValueError(f"Helper changed during copy: {relative}")
    if licensing:
        notices, archive = licensing
        for relative, data in notices.items():
            target = output/'licenses/arcvera-core'/relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
        (output/'source').mkdir()
        (output/'source/arcvera-core-source.zip').write_bytes(archive)
        (output/'SOURCE.md').write_text(source_offer(commit), encoding='utf-8', newline='\n')
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
