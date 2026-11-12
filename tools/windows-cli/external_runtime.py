# Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
"""Repackage frozen binaries without Microsoft CRTs; never rebuild or install.

Modified for Arcvera-Core 2026-11-13; see MODIFICATIONS.md.
"""

import argparse
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import stat
import subprocess
import zipfile

from package_cli import digest
from distribution_policy import is_microsoft_runtime, require_distributable, OFFICIAL_RUNTIME_URL


def inventory(root):
    """Reject links/reparse points before following any input paths."""
    result = {}
    for directory, dirs, files in os.walk(root, followlinks=False):
        for name in dirs + files:
            item = Path(directory) / name
            info = item.lstat()
            if item.is_symlink() or getattr(info, 'st_file_attributes', 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT:
                raise ValueError(f'Package links are not allowed: {item}')
        for name in files:
            item = Path(directory) / name
            if not item.is_file():
                raise ValueError(f'Not a regular package file: {item}')
            result[item.relative_to(root).as_posix()] = digest(item)
    return result


def checked_inventory(root, receipt):
    expected, names = {}, set()
    for entry in receipt['files']:
        name = entry['path']
        path = PurePosixPath(name)
        if (not name or '\\' in name or ':' in name or path.is_absolute()
                or path.as_posix() != name or '..' in path.parts
                or name.casefold() in names or name == 'package-receipt.json'):
            raise ValueError(f'Invalid or colliding receipt path: {name}')
        names.add(name.casefold())
        expected[name] = entry['sha256']
    actual = inventory(root)
    actual.pop('package-receipt.json', None)
    if actual != expected:
        raise ValueError('Base package inventory differs from its receipt')
    for record in receipt['executables'].values():
        if expected.get(record['path']) != record['sha256']:
            raise ValueError('Executable attestation differs from package inventory')
    return actual


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n', encoding='utf-8', newline='\n')


def packaging_source(source):
    """Record packaging revision separately from the unchanged native build."""
    commit = subprocess.check_output(['git', '-C', str(source), 'rev-parse', 'HEAD'], text=True).strip()
    dirty = subprocess.check_output(['git', '-C', str(source), 'status', '--porcelain', '--untracked-files=all'])
    if dirty:
        raise ValueError('Packaging source must be committed and clean')
    archive = subprocess.check_output(['git', '-C', str(source), '-c', 'core.autocrlf=false',
                                       '-c', 'core.eol=lf', 'archive', '--format=zip', commit])
    blobs = {}
    for row in subprocess.check_output(['git', '-C', str(source), 'ls-tree', '-rz', commit]).split(b'\0'):
        if row:
            metadata, name = row.split(b'\t', 1)
            mode, kind, oid = metadata.decode().split()
            if kind != 'blob':
                raise ValueError('Packaging source contains an unsupported Git entry')
            blobs[name.decode()] = oid
    with zipfile.ZipFile(io.BytesIO(archive)) as zipped:
        files = [entry for entry in zipped.infolist() if not entry.is_dir()]
        if len(files) != len(blobs) or {entry.filename for entry in files} != set(blobs):
            raise ValueError('Packaging source archive omits committed files')
        for entry in files:
            data = zipped.read(entry)
            if hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest() != blobs[entry.filename]:
                raise ValueError('Packaging source archive differs from committed blobs')
    return commit, archive


def repack(base, output, source, expected_receipt_sha256):
    base, source = base.resolve(strict=True), source.resolve(strict=True)
    output = output.resolve()
    if output.exists() or output.is_relative_to(base) or base.is_relative_to(output) or output.is_relative_to(source):
        raise ValueError('Output must be new and outside base and packaging source')
    if digest(base / 'package-receipt.json') != expected_receipt_sha256:
        raise ValueError('Base receipt hash differs from approved input')
    receipt = json.loads((base / 'package-receipt.json').read_text(encoding='utf-8'))
    original = checked_inventory(base, receipt)
    commit, archive = packaging_source(source)
    reserved = {'source/build-package-receipt.json', 'source/packaging-source.zip', 'RUNTIME.md'}
    if reserved & original.keys():
        raise ValueError('Base already contains packaging-policy output')
    removed = sorted(name for name in original if is_microsoft_runtime(name))
    voice = 'codex-resources/voice/'
    obsolete = voice + 'windows-crt.json'
    excluded = set(removed) | {obsolete}
    # Validate before creating output. Failed copies have no completion receipt.
    output.mkdir(parents=True)
    for name, expected in original.items():
        if name in excluded:
            continue
        target = output / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(base / name, target)
        if digest(target) != expected:
            raise ValueError(f'Base file changed during copy: {name}')
    shutil.copy2(base / 'package-receipt.json', output / 'source/build-package-receipt.json')
    (output / 'source/packaging-source.zip').write_bytes(archive)
    for relative in ('RUNTIME.md',):
        shutil.copy2(source / 'tools/windows-cli' / relative, output / relative)
    for name in ('LICENSING.md', 'MODIFICATIONS.md'):
        shutil.copy2(source / name, output / 'licenses/arcvera-core' / name)
    shutil.copy2(source / 'third_party/voice/NOTICE.md', output / voice / 'NOTICE.md')
    runtime_path = output / voice / 'runtime.json'
    runtime = json.loads(runtime_path.read_text(encoding='utf-8'))
    runtime['libraries'] = [entry for entry in runtime['libraries'] if not is_microsoft_runtime(entry['path'])]
    runtime['externalPrerequisites'] = [{
        'name': 'Microsoft Visual C++ v14 Redistributable', 'architecture': 'x64',
        'minimumToolsetVersion': '14.50', 'installation': 'user-managed',
        'officialUrl': OFFICIAL_RUNTIME_URL}]
    write_json(runtime_path, runtime)
    manifest_path = output / voice / 'manifest.json'
    manifest = json.loads(manifest_path.read_text(encoding='utf-8'))
    manifest['sha256'] = {name: digest(output / name) for name in manifest['sha256'] if name not in excluded}
    write_json(manifest_path, manifest)
    with (output / 'SOURCE.md').open('a', encoding='utf-8', newline='\n') as note:
        note.write(f'\n## Packaging policy / 패키징 정책\n\nPackaging revision / 패키징 커밋: `{commit}`. '
                   'Exact packaging source / 패키징 대응소스: `source/packaging-source.zip`. '
                   'Native build identity and source above are unchanged / 위 네이티브 빌드와 소스는 변경되지 않았습니다. '
                   'Original build receipt / 기존 빌드 영수증: `source/build-package-receipt.json`. '
                   'External runtime instructions / 외부 런타임 설치 안내: `RUNTIME.md`.\n')
    final_files = inventory(output)
    for name in final_files:
        require_distributable(name)
    for record in receipt['executables'].values():
        if final_files.get(record['path']) != record['sha256']:
            raise ValueError('Frozen executable changed')
    if inventory(base) != {**original, 'package-receipt.json': expected_receipt_sha256}:
        raise ValueError('Base package changed during repackaging')
    receipt['packaging'] = {
        'policy': 'external-microsoft-runtime-v1', 'sourceCommit': commit,
        'sourceArchiveSha256': hashlib.sha256(archive).hexdigest(),
        'basePackageReceiptSha256': expected_receipt_sha256,
        'removedMicrosoftFiles': removed,
        'nativeRebuilt': False,
    }
    receipt['files'] = [{'path': name, 'sha256': value} for name, value in sorted(final_files.items())]
    write_json(output / 'package-receipt.json', receipt)
    return receipt


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('base', 'output', 'source'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--base-receipt-sha256', required=True)
    args = parser.parse_args()
    result = repack(args.base, args.output, args.source, args.base_receipt_sha256)
    print(json.dumps({'files': len(result['files']), 'packaging': result['packaging']}, indent=2))
