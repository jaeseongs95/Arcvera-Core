# Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
"""Real Git/package licensing boundaries; fixture binaries are not Core runtime."""
import hashlib
import json
from pathlib import Path
import subprocess
import unittest
import zipfile

import test_package_cli
from package_cli import digest, package


class LicensingTests(unittest.TestCase):
    def setUp(self):
        test_package_cli.PackageTests.setUp(self)
        repository = Path(__file__).resolve().parents[2]
        self.notices = ['LICENSE', 'LICENSE-APACHE-2.0', 'NOTICE', 'LICENSING.md',
                        'MODIFICATIONS.md', 'upstream/LICENSE', 'upstream/NOTICE']
        for name in self.notices:
            target = self.source/name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes((repository/name).read_bytes())
        (self.vendor/'NOTICE').write_bytes(b'vendor original notice\n')
        self.commit_and_freeze()

    def commit_and_freeze(self):
        for args in [['add', '--', '.'], ['-c', 'commit.gpgsign=false', '-c', 'user.name=Fixture',
                     '-c', 'user.email=fixture@example.invalid', 'commit', '-qm', 'license fixture']]:
            subprocess.run(['git', '-C', str(self.source), *args], check=True, capture_output=True)
        commit = subprocess.check_output(['git', '-C', str(self.source), 'rev-parse', 'HEAD'], text=True).strip()
        names = subprocess.check_output(['git', '-C', str(self.source), 'ls-files', '-z']).decode().split('\0')
        self.freeze.write_text(json.dumps({'candidateCommit':commit,
            'files':[{'path':name,'sha256':digest(self.source/name)} for name in names if name]}), encoding='utf-8')

    def build(self):
        return package(self.source, self.freeze, self.binary, self.vendor, self.output, '0.165.0+test')

    def test_exact_source_and_notices_are_delivered_without_changing_vendor(self):
        receipt = self.build()
        for name in self.notices:
            self.assertEqual((self.output/'licenses/arcvera-core'/name).read_bytes(), (self.source/name).read_bytes())
        self.assertEqual((self.output/'NOTICE').read_bytes(), b'vendor original notice\n')
        offer = (self.output/'SOURCE.md').read_text(encoding='utf-8')
        self.assertIn(receipt['sourceCommit'], offer)
        self.assertIn('AGPL-3.0-or-later', offer)
        archive = self.output/'source/arcvera-core-source.zip'
        with zipfile.ZipFile(archive) as zipped:
            names = {x.filename for x in zipped.infolist() if not x.is_dir()}
            frozen = json.loads(self.freeze.read_text())
            self.assertEqual(names, {x['path'] for x in frozen['files']})
            for name in self.notices:
                self.assertEqual(zipped.read(name), (self.source/name).read_bytes())
            # The source archive uses actual Git blob bytes, including CRLF fixture content.
            self.assertEqual(zipped.read('asset.md'), b'frozen prompt\r\n')
        self.assertIn('source/arcvera-core-source.zip', {x['path'] for x in receipt['files']})

    def test_missing_preserved_notice_is_rejected_before_output(self):
        (self.source/'upstream/NOTICE').unlink()
        self.commit_and_freeze()
        with self.assertRaises((ValueError, FileNotFoundError)):
            self.build()
        self.assertFalse(self.output.exists())

    def test_source_archive_uses_git_blobs_with_windows_native_text(self):
        (self.source/'.gitattributes').write_bytes(b'asset.md -text\ntext-input.md text\n')
        (self.source/'text-input.md').write_bytes(b'canonical Git text\n')
        subprocess.run(['git', '-C', str(self.source), 'config', 'core.autocrlf', 'true'],
                       check=True, capture_output=True)
        self.commit_and_freeze()
        self.build()
        with zipfile.ZipFile(self.output/'source/arcvera-core-source.zip') as zipped:
            self.assertEqual(zipped.read('text-input.md'), b'canonical Git text\n')
            self.assertEqual(zipped.read('asset.md'), b'frozen prompt\r\n')

    def test_corrupted_agpl_is_rejected_before_output(self):
        (self.source/'LICENSE').write_bytes(b'corrupt AGPL')
        self.commit_and_freeze()
        with self.assertRaisesRegex(ValueError, 'AGPL'):
            self.build()
        self.assertFalse(self.output.exists())

    def test_vendor_collision_is_rejected_without_overwrite(self):
        (self.vendor/'SOURCE.md').write_bytes(b'vendor content must survive')
        with self.assertRaisesRegex(ValueError, 'collid'):
            self.build()
        self.assertFalse(self.output.exists())
        self.assertEqual((self.vendor/'SOURCE.md').read_bytes(), b'vendor content must survive')

    def test_tampered_frozen_notice_is_rejected(self):
        (self.source/'NOTICE').write_bytes(b'changed after freeze')
        with self.assertRaisesRegex(ValueError, 'Source changed'):
            self.build()
        self.assertFalse(self.output.exists())


if __name__ == '__main__':
    unittest.main()
