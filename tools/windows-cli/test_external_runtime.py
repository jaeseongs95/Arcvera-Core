# Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
# Modified for Arcvera-Core 1.0.1 (2026-11-13); see MODIFICATIONS.md.
"""Distribution-policy regression cases; no native compilation or installation."""

import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
import zipfile
from unittest.mock import patch

from external_runtime import repack, checked_inventory, packaging_source
from package_cli import digest
from distribution_policy import is_microsoft_runtime, require_distributable
from package_runtime import runtime_files
import package_runtime
import windows_crt
import test_package_cli


class ExternalRuntimeTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.base, self.source, self.output = (self.root / n for n in ('base', 'source', 'out'))
        self.base.mkdir()
        self.source.mkdir()
        self.put(self.source, 'tools/windows-cli/RUNTIME.md', 'English 한국어 official runtime')
        self.put(self.source, 'third_party/voice/NOTICE.md', 'updated notice')
        for name in ('LICENSING.md', 'MODIFICATIONS.md'):
            self.put(self.source, name, 'current ' + name)
        self.git('init', '-q')
        self.git('add', '.')
        self.git('-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '-qm', 'fixture')
        self.commit = self.git('rev-parse', 'HEAD').strip()
        self.voice = 'codex-resources/voice/'
        for name in ('bin/codex.exe', 'source/arcvera-core-source.zip', 'SOURCE.md',
                     'licenses/arcvera-core/LICENSING.md', 'licenses/arcvera-core/MODIFICATIONS.md',
                     self.voice + 'bin/codex-voice-host.exe', self.voice + 'bin/opus.dll',
                     self.voice + 'bin/vcruntime140.dll', self.voice + 'windows-crt.json', self.voice + 'NOTICE.md'):
            self.put(self.base, name, 'frozen ' + name)
        self.put_json(self.base, self.voice + 'runtime.json', {'libraries': [
            {'path': 'bin/opus.dll'}, {'path': 'bin/vcruntime140.dll'}], 'inventorySha256': 'original-build-input'})
        files = {p.relative_to(self.base).as_posix(): digest(p) for p in self.base.rglob('*') if p.is_file()}
        self.put_json(self.base, self.voice + 'manifest.json', {'buildCommit': 'a' * 40, 'sha256': {
            k: v for k, v in files.items() if k.startswith(self.voice)}})
        self.receipt = {'sourceCommit': 'a' * 40, 'sourceFreezeSha256': 'b' * 64,
                        'executables': {'cli': {'path': 'bin/codex.exe', 'sourceCommit': 'a' * 40,
                                               'sha256': digest(self.base / 'bin/codex.exe')}},
                        'files': [{'path': p.relative_to(self.base).as_posix(), 'sha256': digest(p)}
                                  for p in self.base.rglob('*') if p.is_file()]}
        self.save_receipt()

    def git(self, *args):
        return subprocess.check_output(['git', '-C', str(self.source), *args], text=True)

    def put(self, root, name, value):
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(value, encoding='utf-8')

    def put_json(self, root, name, value):
        self.put(root, name, json.dumps(value))

    def save_receipt(self):
        self.put_json(self.base, 'package-receipt.json', self.receipt)
        self.receipt_hash = digest(self.base / 'package-receipt.json')

    def run_repack(self):
        return repack(self.base, self.output, self.source, self.receipt_hash)

    def test_repack_preserves_binary_source_and_provenance(self):
        original_receipt = (self.base / 'package-receipt.json').read_bytes()
        result = self.run_repack()
        self.assertEqual(result['sourceCommit'], 'a' * 40)
        self.assertEqual(result['packaging']['sourceCommit'], self.commit)
        self.assertFalse(result['packaging']['nativeRebuilt'])
        self.assertEqual((self.output / 'source/build-package-receipt.json').read_bytes(), original_receipt)
        for name in ('bin/codex.exe', 'source/arcvera-core-source.zip', self.voice + 'bin/codex-voice-host.exe', self.voice + 'bin/opus.dll'):
            self.assertEqual((self.output / name).read_bytes(), (self.base / name).read_bytes())
        self.assertFalse(any(is_microsoft_runtime(p.name) for p in self.output.rglob('*')))
        self.assertFalse((self.output / self.voice / 'windows-crt.json').exists())
        checked_inventory(self.output, result)
        runtime = json.loads((self.output / self.voice / 'runtime.json').read_text())
        self.assertEqual([r['path'] for r in runtime['libraries']], ['bin/opus.dll'])
        self.assertEqual(runtime['inventorySha256'], 'original-build-input')
        self.assertEqual(runtime['externalPrerequisites'][0]['installation'], 'user-managed')
        manifest = json.loads((self.output / self.voice / 'manifest.json').read_text())
        for name, expected in manifest['sha256'].items():
            self.assertEqual(expected, digest(self.output / name))
        with zipfile.ZipFile(self.output / 'source/packaging-source.zip') as archive:
            self.assertEqual(archive.read('LICENSING.md'), (self.source / 'LICENSING.md').read_bytes())

    def test_invalid_base_is_rejected_before_output(self):
        for name in ('bin/codex.exe', 'unexpected.dll'):
            with self.subTest(name=name):
                path = self.base / name
                old = path.read_bytes() if path.exists() else None
                path.write_bytes(b'tampered')
                with self.assertRaisesRegex(ValueError, 'inventory'):
                    self.run_repack()
                self.assertFalse(self.output.exists())
                if old is None:
                    path.unlink()
                else:
                    path.write_bytes(old)

    def test_bad_hash_and_existing_output_preserved(self):
        with self.assertRaisesRegex(ValueError, 'receipt hash'):
            repack(self.base, self.output, self.source, '0' * 64)
        self.output.mkdir()
        self.put(self.output, 'sentinel', 'preserve')
        with self.assertRaisesRegex(ValueError, 'Output must be new'):
            self.run_repack()
        self.assertEqual((self.output / 'sentinel').read_text(), 'preserve')

    def test_unsafe_colliding_and_self_receipt_paths(self):
        for name in ('../outside', '/absolute', 'BIN/codex.exe', 'bin\\codex.exe', 'bin/./codex.exe', 'package-receipt.json'):
            with self.subTest(name=name):
                broken = dict(self.receipt, files=self.receipt['files'] + [{'path': name, 'sha256': '0' * 64}])
                with self.assertRaisesRegex(ValueError, 'Invalid or colliding'):
                    checked_inventory(self.base, broken)

    def test_changed_executable_attestation(self):
        self.receipt['executables']['cli']['sha256'] = '0' * 64
        self.save_receipt()
        with self.assertRaisesRegex(ValueError, 'attestation'):
            self.run_repack()
        self.assertFalse(self.output.exists())

    def test_dirty_packaging_source(self):
        self.put(self.source, 'LICENSING.md', 'uncommitted')
        with self.assertRaisesRegex(ValueError, 'committed and clean'):
            self.run_repack()
        self.assertFalse(self.output.exists())

    def test_source_archive_cannot_hide_files(self):
        self.put(self.source, '.gitattributes', 'LICENSING.md export-ignore\n')
        self.git('add', '.')
        self.git('-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '-qm', 'hide')
        with self.assertRaisesRegex(ValueError, 'omits committed files'):
            packaging_source(self.source)

    def test_forbidden_names_and_retired_staging(self):
        for name in ('VCRUNTIME140.DLL', 'bin/MSVCP140_1.dll', 'vcruntime140d.dll', 'ucrtbase.dll',
                     'api-ms-win-crt-runtime-l1-1-0.dll', 'VC_REDIST.X64.EXE', 'vcredist_arm64.msi'):
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, 'installed separately'):
                require_distributable(name)
        for name in ('opus.dll', 'codex.exe', 'NOTICE.md'):
            require_distributable(name)
        with patch('urllib.request.urlopen') as network:
            with self.assertRaisesRegex(ValueError, 'staging is disabled'):
                windows_crt.stage(self.root / 'absent', 'x86_64-pc-windows-msvc', self.base / 'bin/codex.exe')
            network.assert_not_called()
        self.assertFalse((self.root / 'absent').exists())

    def test_voice_runtime_rejects_validly_hashed_crt(self):
        runtime = self.root / 'runtime'
        runtime.mkdir()
        self.put(runtime, 'bin/VCRUNTIME140.dll', 'fixture')
        self.put_json(runtime, 'runtime.json', {'schemaVersion': 1, 'developmentOnly': False,
            'distribution': 'publicRelease', 'target': 'x86_64-pc-windows-msvc', 'sourceCommit': 'a' * 40,
            'sourceManifestSha256': digest(Path(package_runtime.__file__).with_name('sources.json')),
            'libraries': [{'path': 'bin/VCRUNTIME140.dll', 'sha256': digest(runtime / 'bin/VCRUNTIME140.dll')}]})
        with self.assertRaisesRegex(ValueError, 'installed separately'):
            runtime_files(runtime, 'x86_64-pc-windows-msvc', public_release=True)

    def test_cli_packager_rejects_reinserted_installer(self):
        # Use the existing fixture builder, with only the candidate's --version
        # mocked. Real Git freeze, inventory and packaging code still run.
        fixture = test_package_cli.PackageTests('test_copies_helpers_replaces_binary_and_records_relative_paths')
        with patch.dict(os.environ, {'PACKAGE_FIXTURE_EXE': str(self.base / 'bin/codex.exe')}):
            fixture.setUp()
        self.addCleanup(fixture.doCleanups)
        self.put(fixture.vendor, 'tools/VC_REDIST.X64.EXE', 'must not ship')
        real_check_output = subprocess.check_output

        def version_or_git(args, **kwargs):
            if args == [str(fixture.binary), '--version']:
                return 'arcvera 0.165.0+test'
            return real_check_output(args, **kwargs)

        with patch('package_cli.subprocess.check_output', side_effect=version_or_git):
            with self.assertRaisesRegex(ValueError, 'installed separately'):
                fixture.build()
        self.assertFalse(fixture.output.exists())


if __name__ == '__main__':
    unittest.main()
