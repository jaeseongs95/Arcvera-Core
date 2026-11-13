# Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
"""Offline historical-import and current-derivative checks; never generate or build."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import posixpath
import re
import sys
import tomllib

if sys.flags.optimize != 0:
    raise RuntimeError('Run source verification without Python optimization; assertion checks are required')

ROOT = Path(__file__).resolve().parents[2]
LICENSE_LINK = 'codex-rs/vendor/bubblewrap/LICENSE'


def sha(data):
    return hashlib.sha256(data).hexdigest()


def read_source(root, item):
    relative = Path(item['path'])
    if relative.is_absolute() or '..' in relative.parts:
        raise ValueError('Unsafe inventory path')
    root = root.resolve()
    path = root/relative
    if path.parent.resolve() != root.joinpath(*relative.parts[:-1]):
        raise ValueError('Unexpected parent symlink or escape')
    if item['gitMode'] == '120000':
        if relative.as_posix() != LICENSE_LINK:
            raise ValueError('Unexpected source symlink')
        value = os.readlink(path).encode() if path.is_symlink() else path.read_bytes()
        if value != b'COPYING':
            raise ValueError('Unexpected license symlink target')
        target = path.parent/'COPYING'
        if target.is_symlink() or not target.is_file():
            raise ValueError('Invalid license target')
        return value
    if path.is_symlink() or not path.resolve().is_relative_to(root):
        raise ValueError('Unexpected source symlink or escape')
    return path.read_bytes()


def header_from_bytes(data):
    match = re.search(r'const GENERATED_TS_HEADER: &str = ("(?:[^"\\]|\\.)*");', data.decode('utf-8'))
    if match is None:
        raise ValueError('Missing actual Rust GENERATED_TS_HEADER contract')
    return json.loads(match.group(1)).encode()


def generated_ts_header(root):
    return header_from_bytes(read_source(root, {'path': 'codex-rs/app-server-protocol/src/export.rs', 'gitMode': '100644'}))


def check_generated_ts_prefix(relative, data, header):
    if relative.startswith('codex-rs/app-server-protocol/schema/typescript/') and relative.endswith('.ts'):
        assert data.replace(b'\r\n', b'\n').startswith(header), f'Generated TypeScript prefix differs: {relative}'


def included_path(relative, target):
    if '\\' in target or PurePosixPath(target).is_absolute() or ':' in target:
        raise ValueError(f'Unsafe source input: {relative}: {target}')
    joined = posixpath.normpath(posixpath.join(posixpath.dirname(relative), target))
    if joined == '..' or joined.startswith('../'):
        raise ValueError(f'Escaping source input: {relative}: {target}')
    return joined


def check_inputs(paths, reader):
    """Check each snapshot's own manifests and includes against its own closed file set."""
    paths = set(paths)
    counts = {'cargoManifests': 0, 'workspaceMembers': 0, 'cargoPathInputs': 0, 'literalRustIncludes': 0}
    for relative in sorted(paths):
        path = PurePosixPath(relative)
        if path.name == 'Cargo.toml':
            doc = tomllib.loads(reader(relative).decode())
            counts['cargoManifests'] += 1
            if relative == 'codex-rs/Cargo.toml':
                for member in doc['workspace']['members']:
                    assert included_path(relative, member+'/Cargo.toml') in paths, member
                    counts['workspaceMembers'] += 1
            def walk(value):
                if isinstance(value, dict):
                    for key, child in value.items():
                        if key == 'path' and isinstance(child, str):
                            target = included_path(relative, child)
                            assert target.startswith('codex-rs/'), (relative, child)
                            assert target in paths or any(p.startswith(target+'/') for p in paths), (relative, child)
                            counts['cargoPathInputs'] += 1
                        else:
                            walk(child)
                elif isinstance(value, list):
                    for child in value:
                        walk(child)
            walk(doc)
        if path.suffix == '.rs':
            for match in re.finditer(r'\binclude(?:_str|_bytes)?!\(\s*"([^"\n]+)"\s*\)', reader(relative).decode()):
                target = match.group(1)
                if '$' not in target:
                    assert included_path(relative, target) in paths, (relative, target)
                    counts['literalRustIncludes'] += 1
    for name in ['bubblewrap.c', 'bind-mount.c', 'network.c', 'utils.c', 'COPYING']:
        assert 'codex-rs/vendor/bubblewrap/'+name in paths, name
    assert LICENSE_LINK in paths and reader(LICENSE_LINK) == b'COPYING'
    assert 'codex-rs/windows-sandbox-rs/codex-windows-sandbox-setup.manifest' in paths
    lock = tomllib.loads(reader('codex-rs/Cargo.lock').decode())
    mio = [p for p in lock['package'] if p['name'] == 'mio']
    assert len(mio) == 1 and mio[0]['version'] == '1.2.4' and mio[0]['source'].startswith('registry+'), 'mio source'
    counts['lockPackages'] = len(lock['package'])
    return counts


def check_historical(root):
    from current import Snapshot, BASE_COMMIT, HISTORICAL
    from export import WINDOWS_INPUTS, WINDOWS_INPUTS_PATH, notice, ATTR_BEGIN, ATTR_END, prompt_attributes
    with Snapshot(root, BASE_COMMIT) as source:
        # Historical metadata and notices belong to the pinned public import, not R41 bytes.
        for path in HISTORICAL:
            assert read_source(root, {'path': path, 'gitMode': '100644'}) == source.read(path), f'Historical artifact changed: {path}'
        baseline = json.loads(source.read('SOURCE_BASELINE.json'))
        inventory_raw = source.read('upstream/source-inventory.json')
        assert sha(inventory_raw) == baseline['sourceInventorySha256'], 'inventory changed'
        inventory = json.loads(inventory_raw)
        assert {i['path'] for i in inventory['excluded']} == {'codex-rs/vendor/bubblewrap/.github/workflows/check.yml'}
        assert not (root/'codex-rs/vendor/bubblewrap/.github/workflows/check.yml').exists()
        assert sha(source.read(WINDOWS_INPUTS_PATH)) == baseline['windowsPromptInputsSha256'], 'Windows prompt catalog changed'
        assert baseline['windowsPromptInputsPath'] == WINDOWS_INPUTS_PATH
        assert json.loads(source.read(WINDOWS_INPUTS_PATH))['sourceCommit'] == baseline['patchedCommit']
        assert len(WINDOWS_INPUTS) == 40
        assert sha(source.read(baseline['patchPath'])) == baseline['patchSha256'], 'patch changed'
        assert baseline['officialCommit'] == '086ccc833bc9cf01024ad2dd6af7dbd421b6bf9a'
        assert baseline['patchedCommit'] == 'c4399dce957bb63fe1fdc276aa4306c06f6cf47f'
        assert baseline['officialVersion'] == '0.167.0'
        assert baseline['previousPublicCommit'] == 'e54da9ee059278403684061bb86e1e7072c16165'
        assert baseline['patchPath'] == 'upstream/official-0.167.0.patch'
        assert inventory['sourceCommit'] == baseline['patchedCommit']
        paths = {i['path'] for i in inventory['files']}
        assert len(paths) == len(inventory['files']) == baseline['sourceFileCount']
        assert len({i['path'] for i in baseline['modifiedFiles']}) == 793
        header = header_from_bytes(source.read('codex-rs/app-server-protocol/src/export.rs'))
        for item in inventory['files']:
            path = item['path']
            data = source.read(path)
            assert source.entries[path][0] == item['gitMode'], path
            assert len(data) == item['bytes'] and sha(data) == item['publishedSha256'], path
            check_generated_ts_prefix(path, data, header)
            if item.get('attributesMerged'):
                prefix = item['repositoryPrefixBytes']
                assert sha(data[:prefix]) == item['repositoryPrefixSha256'], path
                local_header = ATTR_BEGIN+notice(path)
                assert path == '.gitattributes' and data[prefix:].startswith(local_header) and data.endswith(prompt_attributes()+ATTR_END), path
                raw = data[prefix+len(local_header):-len(prompt_attributes()+ATTR_END)]
            elif item['noticeOnly']:
                local_header = notice(path)
                assert local_header and data.startswith(local_header), path
                raw = data[len(local_header):]
            else:
                raw = data
            assert sha(raw) == item['sourceSha256'], path
            assert hashlib.sha1(b'blob '+str(len(raw)).encode()+b'\0'+raw).hexdigest() == item['gitBlob'], path
            assert '/.github/' not in path and not path.startswith('.github/'), path
            if path in WINDOWS_INPUTS:
                assert sha(data) == WINDOWS_INPUTS[path]['gitBlobSha256'], path
        assert len(paths & WINDOWS_INPUTS.keys()) == 40
        legacy_readmes = []
        for path, expected in baseline['preservedRepositoryDocuments'].items():
            actual = sha(source.read(path))
            if path in ['README.md', 'README.ko.md']:
                # These historical capture hashes already differ at the public import base.
                # Keep the captured values intact and expose the gap; current docs are fully
                # identified by publicBaseRepositoryDocuments plus the derivative inventory.
                legacy_readmes.append({'path': path, 'recordedCaptureSha256': expected,
                                       'publicBaseSha256': actual,
                                       'originalCapture': 'PASS' if actual == expected else 'NOT_VERIFIED'})
            else:
                assert path in ['LICENSE', 'NOTICE'] and actual == expected, path
        inputs = check_inputs(paths, source.read)
    return {'sourceFiles': len(paths), 'modifiedFiles': 793,
            'preservedOriginalLicenseAndNotice': 'PASS',
            'legacyReadmeOriginalCapture': {'checkedPublicCommit': BASE_COMMIT,
                                          'documents': legacy_readmes,
                                          'scope': 'Only pinned public-base blobs checked by this command; historical captured README bytes are not claimed preserved'},
            **inputs}


def check_current_payload(root, document):
    from current import read_working, git, check_original_notices
    from export import WINDOWS_INPUTS
    entries = {i['path']: i['gitMode'] for i in document['files']}
    assert entries.get(LICENSE_LINK) == '120000', 'License link mode changed'
    header = generated_ts_header(root)
    for path, mode in entries.items():
        data = read_working(root, path, mode)
        check_generated_ts_prefix(path, data, header)
        if path in WINDOWS_INPUTS:
            assert sha(data) == WINDOWS_INPUTS[path]['gitBlobSha256'], f'Canonical prompt differs: {path}'
            if os.name == 'nt':
                assert sha((root/path).read_bytes()) == WINDOWS_INPUTS[path]['windowsSourceSha256'], f'Windows prompt EOL differs: {path}'
    check_original_notices(root, {name: sha(git(root, 'show', document['baseCommit']+':'+name))
                                 for name in ['LICENSE', 'NOTICE']})
    return check_inputs(entries, lambda p: read_working(root, p, entries[p]))


def verify(root, scratch=None):
    from current import verify_current
    historical = check_historical(root)
    document = verify_current(root, scratch)
    inputs = check_current_payload(root, document)
    return {'status': 'PASS', 'historicalImport': {'status': 'PASS', **historical},
            'currentDerivative': {'status': 'PASS', 'sourceFiles': len(document['files']),
                                  'changes': len(document['changes']), **inputs},
            'derivativePatchReplay': 'PASS', 'historicalOfficialPatchReplay': 'NOT_RUN: private original objects unavailable',
            'embeddedPromptInputs': 40, 'windowsPromptRawHashes': 'PASS' if os.name == 'nt' else 'NOT_RUN: non-Windows host',
            'sourceTreeSha256': document['sourceTreeSha256'], 'build': 'NOT_RUN', 'runtime': 'NOT_RUN'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=ROOT)
    parser.add_argument('--scratch-root', type=Path)
    args = parser.parse_args()
    print(json.dumps(verify(args.root.resolve(), args.scratch_root)))


if __name__ == '__main__':
    main()
