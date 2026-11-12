# Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
"""Explicit derivative manifest generation and read-only integrity checks; no builds."""
import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import subprocess
import tempfile

from export import canonical_bytes, WINDOWS_INPUTS, OFFICIAL

BASE_COMMIT = '676d6233d8601f87f82d5fd6fb3d4c5e04ffa8aa'
MANIFEST = 'CURRENT_SOURCE.json'
DELTA = 'upstream/arcvera-r41.patch'
EXCLUDED = (MANIFEST, DELTA)
LICENSE_LINK = 'codex-rs/vendor/bubblewrap/LICENSE'
R41 = 'Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.'
HISTORICAL = ['SOURCE_BASELINE.json', 'upstream/source-inventory.json',
              'upstream/official-0.167.0.patch', 'upstream/windows-prompt-inputs.json',
              'upstream/LICENSE', 'upstream/NOTICE']
AGPL_SHA256 = '0d96a4ff68ad6d4b6f1f30f713b18d5184912ba8dd389f86aa7710db079abcb0'


def check_original_notices(root, base_documents):
    """An AGPL transition relocates the original license; it never discards it."""
    for path in ['LICENSE', 'NOTICE']:
        if path not in base_documents:
            continue
        data = read_working(root, path, '100644')
        if sha(data) == base_documents[path]:
            continue
        if path == 'LICENSE' and sha(data) == AGPL_SHA256:
            preserved = read_working(root, 'LICENSE-APACHE-2.0', '100644')
            if sha(preserved) == base_documents[path]:
                continue
        raise ValueError(f'Original license/notice changed: {path}')


def sha(data):
    return hashlib.sha256(data).hexdigest()


def encode(value):
    return (json.dumps(value, ensure_ascii=False, indent=2) + '\n').encode('utf-8')


def git(root, *args, data=None, env=None):
    return subprocess.run(['git', '-c', 'core.autocrlf=false', '-c', 'core.safecrlf=false',
                           '-C', str(root), *args], input=data, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, check=True, env=env).stdout


def safe_path(path):
    p = PurePosixPath(path)
    if (not path or '\\' in path or ':' in path or p.is_absolute() or
            p.as_posix() != path or any(x.lower() in ['..', '.', '.git',
            'target', 'node_modules', 'checks', 'moduleanalysiscache', 'microsoft'] for x in p.parts) or
            any(ord(x) < 32 for x in path)):
        raise ValueError(f'Unsafe source path: {path!r}')
    if (p.name.lower() in ['auth.json', '.env'] or p.name.lower().startswith('.env.') or
            p.suffix.lower() in ['.exe', '.dll', '.asar', '.pdb', '.sqlite', '.sqlite3',
                                 '.db', '.log']):
        raise ValueError(f'Excluded runtime or private source path: {path}')
    return p


def tree(root, revision):
    result = {}
    folded = set()
    for row in git(root, 'ls-tree', '-r', '-z', revision).split(b'\0'):
        if not row:
            continue
        metadata, raw_path = row.split(b'\t', 1)
        mode, kind, oid = metadata.decode().split()
        path = raw_path.decode('utf-8')
        safe_path(path)
        if kind != 'blob' or mode not in ['100644', '100755', '120000']:
            raise ValueError(f'Unsupported source tree entry: {path}')
        if path.casefold() in folded:
            raise ValueError(f'Duplicate source path: {path}')
        folded.add(path.casefold())
        result[path] = (mode, oid)
    return result


class Snapshot:
    """Read blobs incrementally; never check out or read other working-tree files."""
    def __init__(self, root, revision):
        self.root = root
        self.entries = tree(root, revision)

    def __enter__(self):
        self.process = subprocess.Popen(['git', '-C', str(self.root), 'cat-file', '--batch'],
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE)
        return self

    def read(self, path):
        safe_path(path)
        _, oid = self.entries[path]
        self.process.stdin.write((oid + '\n').encode())
        self.process.stdin.flush()
        header = self.process.stdout.readline().decode().split()
        if len(header) != 3 or header[0] != oid or header[1] != 'blob':
            raise ValueError(f'Invalid Git blob response: {path}')
        remaining = int(header[2])
        chunks = []
        while remaining:
            data = self.process.stdout.read(remaining)
            if not data:
                raise ValueError(f'Truncated Git blob: {path}')
            chunks.append(data)
            remaining -= len(data)
        if self.process.stdout.read(1) != b'\n':
            raise ValueError(f'Invalid Git blob framing: {path}')
        return b''.join(chunks)

    def __exit__(self, kind, value, traceback):
        self.process.stdin.close()
        self.process.stdout.close()
        self.process.wait(timeout=10)
        if kind is None and self.process.returncode:
            raise ValueError('Git snapshot reader failed')


def working_entries(root):
    """Close the tracked + visible untracked set, rejecting conflict stages first."""
    result = {}
    folded = set()
    for row in git(root, 'ls-files', '--stage', '-z').split(b'\0'):
        if not row:
            continue
        metadata, raw_path = row.split(b'\t', 1)
        mode, _, stage = metadata.decode().split()
        path = raw_path.decode('utf-8')
        safe_path(path)
        if stage != '0' or path.casefold() in folded:
            raise ValueError(f'Unmerged/duplicate source entry: {path}')
        folded.add(path.casefold())
        if mode not in ['100644', '100755', '120000']:
            raise ValueError(f'Unsupported source mode: {path}')
        if (root/path).exists() or (root/path).is_symlink():
            result[path] = mode
    for raw_path in git(root, 'ls-files', '--others', '--exclude-standard', '-z').split(b'\0'):
        if raw_path:
            path = raw_path.decode('utf-8')
            safe_path(path)
            if path.casefold() in folded:
                raise ValueError(f'Duplicate source entry: {path}')
            folded.add(path.casefold())
            result[path] = '120000' if (root/path).is_symlink() else '100644'
    return {p: m for p, m in result.items() if p not in EXCLUDED}


def read_working(root, path, mode):
    # Imported license placeholder handling and parent-link escape checks remain shared.
    from verify import read_source
    safe_path(path)
    raw = read_source(root, {'path': path, 'gitMode': mode})
    data = canonical_bytes(path, raw)
    if path in WINDOWS_INPUTS:
        if sha(data) != WINDOWS_INPUTS[path]['gitBlobSha256']:
            raise ValueError(f'Canonical prompt differs: {path}')
        if os.name == 'nt' and sha(raw) != WINDOWS_INPUTS[path]['windowsSourceSha256']:
            raise ValueError(f'Windows prompt EOL differs: {path}')
    return data


def notice(path, data, old):
    required = path.endswith(('.rs', '.toml', '.py')) or PurePosixPath(path).name == 'Cargo.lock'
    if required and path not in WINDOWS_INPUTS:
        prefix = '// ' if path.endswith('.rs') else '# '
        lines = data.splitlines()
        previous = old.splitlines() if old is not None else []
        if PurePosixPath(path).name == 'Cargo.lock':
            # Cargo moves these two generated warnings before preserved notices.
            preamble = [b'# This file is automatically @generated by Cargo.',
                        b'# It is not intended for manual editing.']
            if lines[:2] == preamble:
                lines = lines[2:]
            if previous[:2] == preamble:
                previous = previous[2:]
        if lines[0:1] != [(prefix + R41).encode()]:
            raise ValueError(f'Missing R41 file notice: {path}')
        # Retain any original leading notice, even when the original first line is R41.
        if previous and b'Modified for ' in previous[0] and previous[0] not in lines[:3]:
            raise ValueError(f'Historical file notice removed: {path}')
        return 'inline-r41'
    return 'external-in-CURRENT_SOURCE.json'


def derivative_notice(root, base, path, mode, oid, data, old):
    required = path.endswith(('.rs', '.toml', '.py')) or PurePosixPath(path).name == 'Cargo.lock'
    if (base == BASE_COMMIT and old is not None and required and
            R41.encode() not in data.split(b'\n', 1)[0]):
        # A byte-for-byte upstream restoration has no derivative inline notice.
        # Query only this safe path; never load unrelated upstream tree entries.
        entry = git(root, 'ls-tree', '-z', OFFICIAL, '--', ':(literal)' + path)
        expected = f'{mode} blob {oid}\t{path}\0'.encode('utf-8')
        if entry == expected:
            return 'unmodified-official:' + OFFICIAL
    return notice(path, data, old)


@contextmanager
def temporary_index(root, scratch=None):
    with tempfile.TemporaryDirectory(prefix='source-integrity-', dir=scratch) as temporary:
        env = os.environ.copy()
        env['GIT_INDEX_FILE'] = str(Path(temporary)/'index')
        # New objects stay in the temporary directory as well as the index.
        object_dir = Path(temporary)/'objects'
        object_dir.mkdir()
        common = git(root, 'rev-parse', '--git-common-dir').decode().strip()
        original_objects = (root/common/'objects').resolve()
        env['GIT_OBJECT_DIRECTORY'] = str(object_dir)
        env['GIT_ALTERNATE_OBJECT_DIRECTORIES'] = str(original_objects)
        yield env


def index_tree(root, items, data_by_path, env, known_blobs=()):
    git(root, 'read-tree', '--empty', env=env)
    rows = []
    for item in items:
        path = item['path']
        oid = item['gitBlob']
        if oid not in known_blobs:
            oid = git(root, 'hash-object', '-w', '--stdin', data=data_by_path(path), env=env).decode().strip()
        if oid != item['gitBlob']:
            raise ValueError(f'Source changed during snapshot: {path}')
        rows.append(f"{item['gitMode']} {oid}\t{path}\0".encode())
    git(root, 'update-index', '-z', '--index-info', data=b''.join(rows), env=env)
    return git(root, 'write-tree', env=env).decode().strip()


def build_manifest(root, base=BASE_COMMIT, scratch=None):
    root = root.resolve()
    entries = working_entries(root)
    files, changes = [], []
    with Snapshot(root, base) as old:
        known_blobs = {oid for mode, oid in old.entries.values()}
        base_documents = {p: sha(old.read(p)) for p in ['LICENSE', 'NOTICE', 'README.md', 'README.ko.md'] if p in old.entries}
        historical = {p: sha(old.read(p)) for p in HISTORICAL if p in old.entries}
        for p in historical:
            if sha(read_working(root, p, '100644')) != historical[p]:
                raise ValueError(f'Historical artifact changed: {p}')
        check_original_notices(root, base_documents)
        for path, mode in sorted(entries.items()):
            data = read_working(root, path, mode)
            oid = hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).hexdigest()
            files.append({'path': path, 'gitMode': mode, 'bytes': len(data), 'sha256': sha(data), 'gitBlob': oid})
            previous = old.entries.get(path)
            if previous != (mode, oid):
                old_data = old.read(path) if previous else None
                changes.append({'path': path, 'status': 'modified' if previous else 'added',
                                'notice': derivative_notice(root, base, path, mode, oid, data, old_data)})
        for path in sorted(set(old.entries)-set(entries)-set(EXCLUDED)):
            changes.append({'path': path, 'status': 'deleted', 'notice': 'external-in-CURRENT_SOURCE.json'})
    if changes:
        modifications = read_working(root, 'MODIFICATIONS.md', entries['MODIFICATIONS.md'])
        if MANIFEST.encode() not in modifications or b'R41' not in modifications:
            raise ValueError('MODIFICATIONS.md must link the R41 CURRENT_SOURCE.json notice inventory')
    with temporary_index(root, scratch) as env:
        result_tree = index_tree(root, files, lambda p: read_working(root, p, entries[p]), env, known_blobs)
        patch = git(root, 'diff', '--binary', '--no-ext-diff', '--no-textconv', '--full-index',
                    '--no-renames', base, result_tree, '--', '.', ':(exclude)'+MANIFEST,
                    ':(exclude)'+DELTA, env=env)
    document = {'schema': 1, 'baseCommit': base, 'historicalArtifacts': historical,
                'publicBaseRepositoryDocuments': base_documents,
                'selfReferenceExclusions': list(EXCLUDED), 'sourceTreeSha256': sha(encode(files)),
                'delta': {'path': DELTA, 'sha256': sha(patch)}, 'files': files,
                'changes': sorted(changes, key=lambda x: x['path'])}
    return document, patch


def replay(root, document, patch, scratch=None):
    """Apply to a temporary index, then compare the complete resulting Git tree."""
    if sha(patch) != document['delta']['sha256']:
        raise ValueError('Derivative patch digest differs')
    with temporary_index(root, scratch) as env:
        git(root, 'read-tree', document['baseCommit'], env=env)
        # Both generated artifacts are outside the derivative source tree.
        for path in EXCLUDED:
            git(root, 'update-index', '--force-remove', '--', path, env=env)
        if patch:
            git(root, 'apply', '--cached', '--binary', '--whitespace=nowarn', '-', data=patch, env=env)
        actual = tree_with_env(root, git(root, 'write-tree', env=env).decode().strip(), env)
    expected = {i['path']: (i['gitMode'], i['gitBlob']) for i in document['files']}
    if len(expected) != len(document['files']) or actual != expected:
        raise ValueError('Derivative patch replay tree differs from current inventory')


def tree_with_env(root, revision, env):
    result = {}
    for row in git(root, 'ls-tree', '-r', '-z', revision, env=env).split(b'\0'):
        if row:
            metadata, path = row.split(b'\t', 1)
            mode, kind, oid = metadata.decode().split()
            if kind != 'blob':
                raise ValueError('Unexpected replay tree entry')
            result[path.decode('utf-8')] = (mode, oid)
    return result


def verify_current(root, scratch=None, expected_base=BASE_COMMIT):
    document = json.loads(read_working(root, MANIFEST, '100644'))
    if document['baseCommit'] != expected_base or document['selfReferenceExclusions'] != list(EXCLUDED):
        raise ValueError('Unexpected derivative base or self-reference exclusions')
    expected, expected_patch = build_manifest(root, expected_base, scratch)
    if document != expected:
        raise ValueError('Current derivative manifest differs (files/modes/notices/closure); regenerate explicitly after review')
    patch = read_working(root, DELTA, '100644')
    if patch != expected_patch:
        raise ValueError('Current derivative patch differs')
    replay(root, document, patch, scratch)
    return document


def generate(root, scratch=None):
    from verify import check_historical, check_current_payload
    from export import checked_target
    # Validate both destinations before reads or writes; metadata links are never followed.
    delta_target = checked_target(root, DELTA)
    manifest_target = checked_target(root, MANIFEST)
    check_historical(root)
    document, patch = build_manifest(root, BASE_COMMIT, scratch)
    check_current_payload(root, document)
    replay(root, document, patch, scratch)
    # Writes are explicit, after every read/check/replay has succeeded.
    delta_target.write_bytes(patch)
    manifest_target.write_bytes(encode(document))
    return document


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--generate', action='store_true', required=True,
                        help='Explicitly create derivative manifest and delta after source review')
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument('--scratch-root', type=Path)
    args = parser.parse_args()
    result = generate(args.root.resolve(), args.scratch_root)
    print(json.dumps({'status': 'GENERATED', 'files': len(result['files']),
                      'changes': len(result['changes']), 'sourceTreeSha256': result['sourceTreeSha256'],
                      'build': 'NOT_RUN', 'runtime': 'NOT_RUN'}))


if __name__ == '__main__':
    main()
