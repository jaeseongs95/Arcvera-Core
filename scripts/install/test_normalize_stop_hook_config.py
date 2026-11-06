# Modified for OpenDIY Codex R38 (2026-11-06); see MODIFICATIONS.md.
#!/usr/bin/env python3

import codecs
import errno
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch


HELPER_PATH = Path(__file__).with_name("normalize_stop_hook_config.py")
spec = importlib.util.spec_from_file_location("stop_hook_normalizer", HELPER_PATH)
normalizer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(normalizer)
ORIGINAL = b'{"hooks":{"Stop":[{"hooks":[{"command":"keep","additionalContextLimit":8192}]}]}}\r\n'
EXPECTED = b'{"hooks":{"Stop":[{"hooks":[{"command":"keep"}]}]}}\r\n'


class NormalizeStopHookConfigTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(
            prefix="r38-stop-hook-", dir="D:/codex/build"
        )
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.path = self.root / "fixture.json"
        self.path.write_bytes(ORIGINAL)

    def backups(self):
        return list(self.root.glob("*.bak"))

    def apply(self, **kwargs):
        return normalizer.normalize_stop_hook_config(self.path, apply=True, **kwargs)

    def test_sh01_default_preview_never_writes(self):
        result = normalizer.normalize_stop_hook_config(self.path)
        self.assertEqual(result["removed"], 1)
        self.assertFalse(result["applied"])
        self.assertEqual(result["sha256"], hashlib.sha256(ORIGINAL).hexdigest())
        self.assertEqual(self.path.read_bytes(), ORIGINAL)
        self.assertEqual(self.backups(), [])

    def test_sh02_only_stop_handler_members_are_removed(self):
        raw = {
            "env": {"VALUE": "private-env"}, "unknown": [1, 2],
            "hooks": {
                "SessionStart": [{"hooks": [{"additionalContextLimit": 8192}]}],
                "Stop": [{"matcher": "*", "additionalContextLimit": 12, "hooks": [
                    {"type": "command", "command": "private-command", "env": {"A": "B"},
                     "timeout": 60, "unknown": {"additionalContextLimit": 1},
                     "additionalContextLimit": 8192},
                    {"command": "second", "additionalContextLimit": 0},
                    {"command": "third"},
                ]}, {"hooks": [{"additionalContextLimit": 15, "timeoutSec": 5}]}],
            },
        }
        self.path.write_text(json.dumps(raw), encoding="utf-8")
        original = self.path.read_bytes()
        expected = json.loads(original)
        del expected["hooks"]["Stop"][0]["hooks"][0]["additionalContextLimit"]
        del expected["hooks"]["Stop"][0]["hooks"][1]["additionalContextLimit"]
        del expected["hooks"]["Stop"][1]["hooks"][0]["additionalContextLimit"]
        result = self.apply()
        self.assertEqual(result["removed"], 3)
        self.assertEqual(json.loads(self.path.read_bytes()), expected)
        self.assertEqual(Path(result["backup"]).read_bytes(), original)

    def test_sh03_member_positions_and_escaped_key_preserve_raw_text(self):
        fixtures = [
            ('{"additionalContextLimit":8192,"command":"x"}', '{"command":"x"}'),
            ('{"a":1,"additionalContextLimit":8192,"b":2}', '{"a":1,"b":2}'),
            ('{"command":"x","additionalContextLimit":8192}', '{"command":"x"}'),
            ('{"additionalContextLimit":8192}', '{}'),
            ('{\n "additionalContextLimit": 8192,\n "command": "x"\n}',
             '{\n \n "command": "x"\n}'),
            ('{"additionalContext\\u004cimit":8192,"command":"\\u0061"}',
             '{"command":"\\u0061"}'),
            ('{"command":"x","additionalContextLimit":{"x":[1,2]},"n":-0.00e+1}',
             '{"command":"x","n":-0.00e+1}'),
        ]
        for index, (source, expected) in enumerate(fixtures):
            with self.subTest(index=index):
                prefix, suffix = '{"hooks":{"Stop":[{"hooks":[', ']}]}}'
                self.path.write_bytes((prefix + source + suffix).encode())
                self.apply()
                self.assertEqual(self.path.read_bytes(), (prefix + expected + suffix).encode())

    def test_sh04_encodings_bom_and_mixed_newlines(self):
        text = '{\r\n"hooks":{\n"Stop":[{"hooks":[{"additionalContextLimit":8192,"command":"한글"}]}]}}\r\n'
        expected = '{\r\n"hooks":{\n"Stop":[{"hooks":[{"command":"한글"}]}]}}\r\n'
        formats = [
            ("utf-8", b""), ("utf-8", codecs.BOM_UTF8),
            ("utf-16-le", b""), ("utf-16-le", codecs.BOM_UTF16_LE),
            ("utf-16-be", b""), ("utf-16-be", codecs.BOM_UTF16_BE),
            ("utf-32-le", b""), ("utf-32-le", codecs.BOM_UTF32_LE),
            ("utf-32-be", b""), ("utf-32-be", codecs.BOM_UTF32_BE),
        ]
        for encoding, bom in formats:
            with self.subTest(encoding=encoding, bom=bool(bom)):
                original = bom + text.encode(encoding)
                self.path.write_bytes(original)
                result = self.apply()
                self.assertEqual(self.path.read_bytes(), bom + expected.encode(encoding))
                self.assertEqual(Path(result["backup"]).read_bytes(), original)

    def test_sh05_unrelated_or_absent_targets_are_byte_identical(self):
        for raw in (b'{}', b'{"hooks":{"Stop":[]}}',
                    b'{"hooks":{"SessionStart":[{"hooks":[{"additionalContextLimit":3}]}]}}'):
            with self.subTest(raw=raw):
                self.path.write_bytes(raw)
                self.assertEqual(self.apply()["removed"], 0)
                self.assertEqual(self.path.read_bytes(), raw)
                self.assertEqual(self.backups(), [])

    def test_sh06_invalid_json_and_encoding_never_write(self):
        for raw in (b'{', b'{"hooks":{},"hooks":{}}', b'{"n":NaN}',
                    b'{"n":Infinity}', b'\xff', b'{"hooks":{"Stop":[],}}'):
            with self.subTest(raw=raw):
                self.path.write_bytes(raw)
                with self.assertRaises(ValueError):
                    self.apply()
                self.assertEqual(self.path.read_bytes(), raw)
                self.assertEqual(self.backups(), [])

    def test_sh07_invalid_shapes_never_write(self):
        for value in ([], {"hooks": []}, {"hooks": {"Stop": {}}},
                      {"hooks": {"Stop": [None]}},
                      {"hooks": {"Stop": [{"hooks": {}}]}},
                      {"hooks": {"Stop": [{"hooks": [None]}]}}):
            with self.subTest(value=value):
                raw = json.dumps(value).encode()
                self.path.write_bytes(raw)
                with self.assertRaises(ValueError):
                    self.apply()
                self.assertEqual(self.path.read_bytes(), raw)
                self.assertEqual(self.backups(), [])

    def test_sh08_preview_digest_rejects_new_content(self):
        preview = normalizer.normalize_stop_hook_config(self.path)
        self.path.write_bytes(ORIGINAL + b" ")
        with self.assertRaisesRegex(ValueError, "changed since preview"):
            self.apply(expected_sha256=preview["sha256"])
        self.assertEqual(self.path.read_bytes(), ORIGINAL + b" ")
        self.assertEqual(self.backups(), [])

    def test_sh09_snapshot_race_and_same_content_replacement_are_rejected(self):
        original_open = normalizer._open_for_update
        for replace in (False, True):
            with self.subTest(replace=replace):
                self.path.write_bytes(ORIGINAL)

                def raced_open(path):
                    if replace:
                        replacement = self.root / "replacement.json"
                        replacement.write_bytes(ORIGINAL)
                        os.replace(replacement, path)
                    else:
                        path.write_bytes(ORIGINAL + b" ")
                    return original_open(path)

                with patch.object(normalizer, "_open_for_update", raced_open):
                    with self.assertRaisesRegex(ValueError, "changed before apply"):
                        self.apply()
                self.assertEqual(self.path.read_bytes(), ORIGINAL if replace else ORIGINAL + b" ")
                self.assertEqual(self.backups(), [])

    def test_sh10_existing_writer_causes_actual_sharing_violation(self):
        with normalizer._open_for_update(self.path):
            with self.assertRaises(PermissionError) as caught:
                self.apply()
            self.assertEqual(caught.exception.errno, errno.EACCES)
            with self.assertRaises(OSError) as caught:
                normalizer._open_for_update(self.path)
            self.assertEqual(caught.exception.winerror, 32)
        self.assertEqual(self.path.read_bytes(), ORIGINAL)
        self.assertEqual(self.backups(), [])

    def test_sh11_backup_failure_never_changes_original(self):
        with patch.object(normalizer, "_write_backup", side_effect=OSError("fixture backup error")):
            with self.assertRaisesRegex(OSError, "fixture backup error"):
                self.apply()
        self.assertEqual(self.path.read_bytes(), ORIGINAL)
        self.assertEqual(self.backups(), [])

    def test_sh12_cli_defaults_to_preview_and_apply_is_explicit(self):
        argv = [sys.executable, "-I", "-S", "-B", "-X", "utf8", str(HELPER_PATH), str(self.path)]
        preview = subprocess.run(argv, capture_output=True, text=True, check=False)
        self.assertEqual(preview.returncode, 0, preview.stderr)
        self.assertFalse(json.loads(preview.stdout)["applied"])
        self.assertEqual(self.path.read_bytes(), ORIGINAL)
        self.assertEqual(self.backups(), [])
        applied = subprocess.run(argv + ["--apply"], capture_output=True, text=True, check=False)
        self.assertEqual(applied.returncode, 0, applied.stderr)
        self.assertTrue(json.loads(applied.stdout)["applied"])
        self.assertEqual(self.path.read_bytes(), EXPECTED)
        self.assertEqual(len(self.backups()), 1)
        for output in (preview.stdout, preview.stderr, applied.stdout, applied.stderr):
            self.assertNotIn('"command"', output)
            self.assertNotIn('"keep"', output)

    def test_sh13_missing_directory_symlink_and_bad_cli_are_safe(self):
        missing = self.root / "missing.json"
        for path in (missing, self.root):
            with self.subTest(path=path):
                with self.assertRaises(OSError):
                    normalizer.normalize_stop_hook_config(path, apply=True)
        self.assertFalse(missing.exists())
        linked = self.root / "link.json"
        # Windows symlink privilege is not assumed: the rejection branch is
        # component-tested; live symlink creation is reported separately.
        with patch.object(Path, "is_symlink", return_value=True):
            with self.assertRaisesRegex(ValueError, "symbolic links"):
                normalizer.normalize_stop_hook_config(linked, apply=True)
        self.assertFalse(linked.exists())
        result = subprocess.run(
            [sys.executable, "-I", "-S", "-B", "-X", "utf8", str(HELPER_PATH), str(missing), "--apply"],
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("FileNotFoundError", result.stderr)
        self.assertEqual(self.path.read_bytes(), ORIGINAL)
        self.assertEqual(self.backups(), [])

    def test_sh14_repeated_apply_preserves_the_original_backup(self):
        first = self.apply()
        self.assertEqual(self.path.read_bytes(), EXPECTED)
        second = self.apply()
        self.assertEqual(second["removed"], 0)
        self.assertFalse(second["applied"])
        self.assertEqual(self.backups(), [Path(first["backup"])])
        self.assertEqual(Path(first["backup"]).read_bytes(), ORIGINAL)

    def test_sh15_new_writer_is_refused_throughout_apply(self):
        write_backup = normalizer._write_backup

        def check_locked(path, raw):
            with self.assertRaises(OSError) as caught:
                with normalizer._open_for_update(path):
                    self.fail("a second writer unexpectedly acquired the file")
            self.assertEqual(caught.exception.winerror, 32)
            return write_backup(path, raw)

        with patch.object(normalizer, "_write_backup", check_locked):
            result = self.apply()
        self.assertEqual(self.path.read_bytes(), EXPECTED)
        self.assertEqual(Path(result["backup"]).read_bytes(), ORIGINAL)

    def test_sh16_createfile_contract_opens_the_reparse_point_exclusively(self):
        create = Mock(return_value=normalizer.ctypes.c_void_p(-1).value)
        kernel = SimpleNamespace(CreateFileW=create)
        with patch.object(normalizer.ctypes, "WinDLL", return_value=kernel):
            with patch.object(normalizer.ctypes, "get_last_error", return_value=32):
                with self.assertRaises(OSError):
                    normalizer._open_for_update(self.path)
        args = create.call_args.args
        self.assertEqual(args[0], str(self.path))
        self.assertEqual(args[1], 0xC0000000)  # GENERIC_READ | GENERIC_WRITE
        self.assertEqual(args[2], 0)  # No read/write/delete sharing.
        self.assertEqual(args[4], 3)  # OPEN_EXISTING: never create a config.
        self.assertEqual(args[5] & 0x00200000, 0x00200000)  # OPEN_REPARSE_POINT

    def test_sh17_live_symlink_race_cannot_write_the_referent(self):
        victim = self.root / "victim.json"
        victim_raw = ORIGINAL + b" "
        victim.write_bytes(victim_raw)
        probe = self.root / "probe-link.json"
        try:
            os.symlink(victim, probe)
        except OSError as error:
            if error.winerror == 1314:
                self.skipTest("live symlink NOT_RUN: Windows privilege unavailable (1314)")
            raise
        probe.unlink()
        original_check = Path.is_symlink
        original_file = self.root / "original.json"

        def replace_after_check(path):
            result = original_check(path)
            if path == self.path:
                path.rename(original_file)
                os.symlink(victim, path)
            return result

        with patch.object(Path, "is_symlink", replace_after_check):
            with patch.object(normalizer, "_normalize", wraps=normalizer._normalize) as decoded:
                with self.assertRaisesRegex(ValueError, "changed before apply"):
                    self.apply()
                # Characterize the remaining initial-read gap separately from
                # the no-follow update handle; this does not assert read safety.
                self.assertEqual(decoded.call_args.args[0], victim_raw)
        self.assertEqual(victim.read_bytes(), victim_raw)
        self.assertEqual(original_file.read_bytes(), ORIGINAL)
        self.assertEqual(self.backups(), [])


if __name__ == "__main__":
    unittest.main()
