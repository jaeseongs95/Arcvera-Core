# Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
# Modified for Arcvera-Core 1.0.1 (2026-11-13); see MODIFICATIONS.md.
"""Real isolated Git/index tests for derivative provenance failure boundaries."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import current
import verify


class CurrentSourceChecks(unittest.TestCase):
    def test_agpl_preserves_original_notices(self):
        root, _ = self.fixture()
        original = b'original Apache license\n'
        (root/'LICENSE').write_bytes(original)
        (root/'NOTICE').write_bytes(b'original notice\n')
        current.git(root, 'add', '--', '.')
        current.git(root, '-c', 'commit.gpgsign=false', '-c', 'user.name=Fixture',
                    '-c', 'user.email=fixture@example.invalid', 'commit', '-qm', 'original notices')
        base = current.git(root, 'rev-parse', 'HEAD').decode().strip()
        agpl = (Path(__file__).resolve().parents[2]/'LICENSE').read_bytes()
        self.assertEqual(hashlib.sha256(agpl).hexdigest(),
                         '0d96a4ff68ad6d4b6f1f30f713b18d5184912ba8dd389f86aa7710db079abcb0')
        (root/'LICENSE').write_bytes(agpl)
        (root/'LICENSE-APACHE-2.0').write_bytes(original)
        doc, delta = current.build_manifest(root, base)
        current.replay(root, doc, delta)
        self.assertEqual(doc['publicBaseRepositoryDocuments']['LICENSE'], current.sha(original))
        for path, bad in [('LICENSE-APACHE-2.0', b'changed original'),
                          ('LICENSE', agpl+b'corruption'), ('NOTICE', b'changed notice')]:
            target = root/path
            good = target.read_bytes()
            with self.subTest(path=path):
                target.write_bytes(bad)
                with self.assertRaisesRegex(ValueError, 'Original license/notice changed'):
                    current.build_manifest(root, base)
                target.write_bytes(good)
        (root/'LICENSE-APACHE-2.0').unlink()
        with self.assertRaises((ValueError, FileNotFoundError)):
            current.build_manifest(root, base)

    def fixture(self):
        temporary = tempfile.TemporaryDirectory(prefix='derivative-fixture-')
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        for path, data in {'.gitattributes': b'* -text\n',
                           'MODIFICATIONS.md': b'R41 external notice inventory: CURRENT_SOURCE.json\n',
                           'old.rs': b'// Modified for Old R39; retained.\nfn old() {}\n',
                           'file.bin': b'\x00old\xff\n', 'retired.txt': b'retired\n'}.items():
            (root/path).write_bytes(data)
        current.git(root, 'init', '-q')
        current.git(root, 'add', '--', '.')
        current.git(root, '-c', 'commit.gpgsign=false', '-c', 'user.name=Fixture',
                    '-c', 'user.email=fixture@example.invalid', 'commit', '-qm', 'base')
        base = current.git(root, 'rev-parse', 'HEAD').decode().strip()
        (root/'old.rs').write_bytes(b'// '+current.R41.encode()+b'\n// Modified for Old R39; retained.\nfn changed() {}\n')
        (root/'file.bin').write_bytes(b'\x00new\xff\r\n')
        (root/'retired.txt').unlink()
        (root/'new.txt').write_bytes(b'new\n')
        return root, base

    def freeze(self, root, base):
        document, delta = current.build_manifest(root, base)
        (root/current.MANIFEST).write_bytes(current.encode(document))
        (root/current.DELTA).parent.mkdir(parents=True, exist_ok=True)
        (root/current.DELTA).write_bytes(delta)
        return document, delta

    def test_binary_added_deleted_replay_and_manual_inventory(self):
        root, base = self.fixture()
        doc, delta = self.freeze(root, base)
        expected = {'file.bin': 'modified', 'new.txt': 'added', 'old.rs': 'modified', 'retired.txt': 'deleted'}
        self.assertEqual({x['path']: x['status'] for x in doc['changes']}, expected)
        self.assertIn(b'GIT binary patch', delta)
        items = {x['path']: x for x in doc['files']}
        self.assertEqual(items['file.bin']['sha256'], hashlib.sha256(b'\x00new\xff\r\n').hexdigest())
        self.assertEqual(items['new.txt']['bytes'], 4)
        self.assertNotIn(current.MANIFEST, items)
        self.assertNotIn(current.DELTA, items)
        current.replay(root, doc, delta)
        self.assertEqual(current.verify_current(root, expected_base=base), doc)

    def test_verifier_never_regenerates_corrupt_source(self):
        root, base = self.fixture()
        self.freeze(root, base)
        before = (root/current.MANIFEST).read_bytes(), (root/current.DELTA).read_bytes()
        (root/'new.txt').write_bytes(b'changed without manifest\n')
        with self.assertRaisesRegex(ValueError, 'manifest differs'):
            current.verify_current(root, expected_base=base)
        self.assertEqual(before, ((root/current.MANIFEST).read_bytes(), (root/current.DELTA).read_bytes()))

    def test_additional_untracked_file_is_not_hidden_by_git_diff(self):
        root, base = self.fixture()
        self.freeze(root, base)
        (root/'surprise.txt').write_bytes(b'extra\n')
        with self.assertRaisesRegex(ValueError, 'manifest differs'):
            current.verify_current(root, expected_base=base)

    def test_deleted_current_file_is_rejected(self):
        root, base = self.fixture()
        self.freeze(root, base)
        (root/'new.txt').unlink()
        with self.assertRaisesRegex(ValueError, 'manifest differs'):
            current.verify_current(root, expected_base=base)

    def test_duplicate_manifest_and_exclusion_change_are_rejected(self):
        root, base = self.fixture()
        doc, _ = self.freeze(root, base)
        doc['files'].append(dict(doc['files'][0]))
        (root/current.MANIFEST).write_bytes(current.encode(doc))
        with self.assertRaisesRegex(ValueError, 'manifest differs'):
            current.verify_current(root, expected_base=base)
        doc, _ = self.freeze(root, base)
        doc['selfReferenceExclusions'].append('old.rs')
        (root/current.MANIFEST).write_bytes(current.encode(doc))
        with self.assertRaisesRegex(ValueError, 'exclusions'):
            current.verify_current(root, expected_base=base)

    def test_changed_tracked_mode_is_rejected(self):
        root, base = self.fixture()
        self.freeze(root, base)
        current.git(root, 'update-index', '--chmod=+x', '--', 'old.rs')
        with self.assertRaisesRegex(ValueError, 'manifest differs'):
            current.verify_current(root, expected_base=base)

    def test_missing_new_notice_is_rejected(self):
        root, base = self.fixture()
        (root/'new.rs').write_bytes(b'fn new() {}\n')
        with self.assertRaisesRegex(ValueError, 'Missing R41 file notice: new.rs'):
            current.build_manifest(root, base)

    def test_historical_notice_must_be_retained(self):
        root, base = self.fixture()
        (root/'old.rs').write_bytes(b'// '+current.R41.encode()+b'\nfn changed() {}\n')
        with self.assertRaisesRegex(ValueError, 'Historical file notice removed'):
            current.build_manifest(root, base)

    def test_101_notice_is_exact_and_preserves_historical_notice(self):
        old = b'// ' + current.R41.encode() + b'\nfn old() {}\n'
        marker = b'// ' + current.V101.encode() + b'\n'
        self.assertEqual(current.notice('old.rs', marker + old, old), 'inline-1.0.1')
        with self.assertRaisesRegex(ValueError, 'Historical file notice removed'):
            current.notice('old.rs', marker + b'fn changed() {}\n', old)
        with self.assertRaisesRegex(ValueError, 'Missing R41 file notice'):
            current.notice('new.rs', marker.replace(b'2026-11-13', b'2026-11-12'), None)

    def test_external_notice_link_required(self):
        root, base = self.fixture()
        (root/'MODIFICATIONS.md').write_bytes(b'R41 no inventory link\n')
        with self.assertRaisesRegex(ValueError, 'must link'):
            current.build_manifest(root, base)

    def test_unsafe_and_runtime_paths_rejected_before_read(self):
        for path in ['../outside', '/absolute', 'a\\b', 'x:drive', 'a/../b',
                     'codex-rs/Microsoft/private', 'auth.json', '.env', '.env.local',
                     'x.sqlite', 'target/x', 'a\nb']:
            with self.subTest(path=path), self.assertRaises(ValueError):
                current.safe_path(path)
        root, base = self.fixture()
        (root/'auth.json').write_bytes(b'fixture only; must not be read')
        with self.assertRaisesRegex(ValueError, 'runtime or private'):
            current.build_manifest(root, base)

    def test_changed_historical_artifact_is_rejected(self):
        root, _ = self.fixture()
        (root/'SOURCE_BASELINE.json').write_bytes(b'{"historical":true}\n')
        current.git(root, 'add', '--', 'SOURCE_BASELINE.json')
        current.git(root, '-c', 'commit.gpgsign=false', '-c', 'user.name=Fixture',
                    '-c', 'user.email=fixture@example.invalid', 'commit', '-qm', 'historical metadata')
        base = current.git(root, 'rev-parse', 'HEAD').decode().strip()
        (root/'SOURCE_BASELINE.json').write_bytes(b'{"historical":false}\n')
        with self.assertRaisesRegex(ValueError, 'Historical artifact changed'):
            current.build_manifest(root, base)

    def test_snapshot_reads_fixed_blob_not_working_file(self):
        root, base = self.fixture()
        with current.Snapshot(root, base) as snapshot:
            self.assertEqual(snapshot.read('file.bin'), b'\x00old\xff\n')
            self.assertEqual(snapshot.read('old.rs'), b'// Modified for Old R39; retained.\nfn old() {}\n')
        self.assertNotEqual((root/'file.bin').read_bytes(), b'\x00old\xff\n')

    def test_corrupt_patch_and_wrong_base_are_rejected(self):
        root, base = self.fixture()
        doc, delta = self.freeze(root, base)
        with self.assertRaisesRegex(ValueError, 'patch digest'):
            current.replay(root, doc, delta+b'corruption')
        corrupt = delta.replace(b'new.txt', b'bad.txt')
        doc['delta']['sha256'] = current.sha(corrupt)
        with self.assertRaises((ValueError, subprocess.CalledProcessError)):
            current.replay(root, doc, corrupt)
        doc['baseCommit'] = '0'*40
        with self.assertRaises(subprocess.CalledProcessError):
            current.replay(root, doc, corrupt)

    def test_excluded_self_artifacts_do_not_change_source_identity(self):
        root, base = self.fixture()
        doc, _ = self.freeze(root, base)
        (root/current.MANIFEST).write_bytes(b'arbitrary excluded metadata')
        (root/current.DELTA).write_bytes(b'arbitrary excluded patch')
        again, _ = current.build_manifest(root, base)
        self.assertEqual(again, doc)
        # Their bytes are separately checked by verification and the external final freeze.
        with self.assertRaises(json.JSONDecodeError):
            current.verify_current(root, expected_base=base)

    def test_new_literal_include_must_be_in_closed_inventory(self):
        with self.assertRaisesRegex(AssertionError, 'missing.txt'):
            verify.check_inputs({'codex-rs/new.rs'}, lambda _: b'include_str!("missing.txt");\n')

    def test_path_dependency_cannot_escape_or_come_from_uninventoried_disk(self):
        with self.assertRaisesRegex(AssertionError, 'missing'):
            verify.check_inputs({'codex-rs/unit/Cargo.toml'}, lambda _: b'[dependencies.x]\npath = "../missing"\n')
        with self.assertRaisesRegex(ValueError, 'Escaping source input'):
            verify.included_path('codex-rs/unit/lib.rs', '../../../outside')

    def test_modified_generated_ts_still_requires_header_at_byte_zero(self):
        path = 'codex-rs/app-server-protocol/schema/typescript/Fixture.ts'
        header = b'// Generated Rust contract\n'
        verify.check_generated_ts_prefix(path, header+b'export type T = string;\n', header)
        with self.assertRaisesRegex(AssertionError, 'prefix differs'):
            verify.check_generated_ts_prefix(path, b'// '+current.R41.encode()+b'\n'+header, header)

    def test_prompt_canonical_and_windows_raw_contracts_remain_distinct(self):
        root, _ = self.fixture()
        path = 'fixture-prompt.md'
        expected = {'gitBlobSha256': current.sha(b'prompt\n'),
                    'windowsSourceSha256': current.sha(b'prompt\r\n')}
        with patch.dict(current.WINDOWS_INPUTS, {path: expected}):
            (root/path).write_bytes(b'prompt\r\n')
            self.assertEqual(current.read_working(root, path, '100644'), b'prompt\n')
            (root/path).write_bytes(b'changed\r\n')
            with self.assertRaisesRegex(ValueError, 'Canonical prompt differs'):
                current.read_working(root, path, '100644')
            if os.name == 'nt':
                (root/path).write_bytes(b'prompt\n')
                with self.assertRaisesRegex(ValueError, 'Windows prompt EOL differs'):
                    current.read_working(root, path, '100644')

    def test_new_verifier_optimized_mode_is_rejected(self):
        import sys
        p = subprocess.run([sys.executable, '-B', '-O', str(Path(verify.__file__))],capture_output=True,text=True)
        self.assertNotEqual(p.returncode, 0)
        self.assertIn('without Python optimization', p.stderr)

    def test_metadata_links_are_rejected_before_read_or_generation(self):
        root, base = self.fixture()
        self.freeze(root, base)
        target = root/current.MANIFEST
        before = target.read_bytes()
        # Only the link capability is synthetic; destination bytes and real guards are used.
        with patch.object(Path, 'is_symlink', lambda p: p == target):
            with self.assertRaisesRegex(ValueError, 'Unexpected source symlink'):
                current.verify_current(root, expected_base=base)
            with self.assertRaisesRegex(ValueError, 'Unexpected export symlink'):
                current.generate(root)
        self.assertEqual(target.read_bytes(), before)


    def test_cargo_generated_preamble_preserves_required_notices(self):
        path = 'codex-rs/Cargo.lock'
        preamble = (b'# This file is automatically @generated by Cargo.\n'
                    b'# It is not intended for manual editing.\n')
        old = b'# Modified for Old R39; retained.\nversion = 4\n'
        marker = b'# ' + current.R41.encode() + b'\n'
        valid = marker + old
        self.assertEqual(current.notice(path, valid, old), 'inline-r41')
        self.assertEqual(current.notice(path, preamble + valid, old), 'inline-r41')
        self.assertEqual(current.notice(path, preamble + valid, preamble + old), 'inline-r41')
        for invalid in [preamble + old, b'# arbitrary preamble\n' + valid,
                        preamble.replace(b'@generated', b'generated') + valid]:
            with self.subTest(data=invalid), self.assertRaisesRegex(ValueError, 'Missing R41 file notice'):
                current.notice(path, invalid, old)
        with self.assertRaisesRegex(ValueError, 'Historical file notice removed'):
            current.notice(path, preamble + marker + b'version = 4\n', preamble + old)
        with self.assertRaisesRegex(ValueError, 'Missing R41 file notice'):
            current.notice('ordinary.rs', preamble + b'// ' + current.R41.encode() + b'\n', None)

    def test_exact_pinned_official_restoration_requires_blob_and_mode(self):
        root, official = self.fixture()
        original = current.git(root, 'show', official + ':old.rs')
        official_mode = current.git(root, 'ls-tree', official, '--', 'old.rs').split()[0]
        self.assertEqual(official_mode, b'100644')
        current.git(root, 'add', '--all')
        current.git(root, '-c', 'commit.gpgsign=false', '-c', 'user.name=Fixture',
                    '-c', 'user.email=fixture@example.invalid', 'commit', '-qm', 'custom import')
        base = current.git(root, 'rev-parse', 'HEAD').decode().strip()
        (root/'old.rs').write_bytes(original)
        # Pin only fixture identities; all blobs, modes and manifest calls are real Git.
        with patch.object(current, 'BASE_COMMIT', base), patch.object(current, 'OFFICIAL', official, create=True):
            doc, delta = current.build_manifest(root, base)
            changes = {item['path']: item for item in doc['changes']}
            self.assertEqual(changes['old.rs']['notice'], 'unmodified-official:' + official)
            self.assertEqual((root/'old.rs').read_bytes(), original)
            current.replay(root, doc, delta)
            (root/'old.rs').write_bytes(original + b'// extra change\n')
            with self.assertRaisesRegex(ValueError, 'Missing R41 file notice'):
                current.build_manifest(root, base)
            (root/'old.rs').write_bytes(original)
            current.git(root, 'update-index', '--chmod=+x', '--', 'old.rs')
            with self.assertRaisesRegex(ValueError, 'Missing R41 file notice'):
                current.build_manifest(root, base)
            current.git(root, 'update-index', '--chmod=-x', '--', 'old.rs')
            with patch.object(current, 'BASE_COMMIT', '0'*40):
                with self.assertRaisesRegex(ValueError, 'Missing R41 file notice'):
                    current.build_manifest(root, base)
            with patch.object(current, 'OFFICIAL', '0'*40):
                with self.assertRaises(subprocess.CalledProcessError):
                    current.build_manifest(root, base)

if __name__ == '__main__':
    unittest.main()
