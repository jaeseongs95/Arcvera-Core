# Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
# Modified for Arcvera-Core 1.0.1 (2026-11-13); see MODIFICATIONS.md.
# Modified for OpenDIY Codex R33 (2026-11-02); see MODIFICATIONS.md.
"""Local package tests. These are not tests of the patched CLI's runtime."""

import json
import os
from pathlib import Path
import subprocess
import shutil
import tempfile
import unittest

from package_cli import digest, package


class PackageTests(unittest.TestCase):
    def setUp(self):
        fixture = os.environ.get("PACKAGE_FIXTURE_EXE")
        if not fixture:
            self.skipTest("Set PACKAGE_FIXTURE_EXE to the local argv fixture executable")
        self.temporary = tempfile.TemporaryDirectory(prefix="codex-package-한글 ")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.source = self.root / "source"
        self.source.mkdir()
        self.asset = self.source / "asset.md"
        self.asset.write_bytes(b"frozen prompt\r\n")
        (self.source / "other.txt").write_bytes(b"tracked but not in changed-file freeze")
        for args in (
            ["init", "-q"], ["add", "asset.md", "other.txt"],
            ["-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
             "commit", "-qm", "fixture"],
        ):
            subprocess.run(["git", "-C", str(self.source), *args], check=True,
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        commit = subprocess.check_output(
            ["git", "-C", str(self.source), "rev-parse", "HEAD"], text=True
        ).strip()
        self.freeze = self.root / "freeze.json"
        self.freeze.write_text(json.dumps({
            "candidateCommit": commit,
            "files": [{"path": name, "sha256": digest(self.source / name)}
                      for name in ("asset.md", "other.txt")],
        }), encoding="utf-8")
        self.binary = Path(fixture)
        self.vendor = self.root / "vendor"
        (self.vendor / "bin").mkdir(parents=True)
        (self.vendor / "bin/codex.exe").write_bytes(b"official binary must be replaced")
        (self.vendor / "helper.dat").write_bytes(b"preserved helper")
        self.metadata = self.vendor / "codex-package.json"
        self.metadata.write_text(json.dumps({
            "layoutVersion": 1, "version": "0.165.0", "entrypoint": "bin/codex.exe",
        }), encoding="utf-8")
        self.output = self.root / "패치 package"

    def build(self, version="0.165.0+test"):
        return package(self.source, self.freeze, self.binary, self.vendor, self.output, version)

    def test_copies_helpers_replaces_binary_and_records_relative_paths(self):
        receipt = self.build()
        self.assertEqual(digest(self.output / "bin/arcvera.exe"), digest(self.binary))
        self.assertEqual((self.output / "helper.dat").read_bytes(), b"preserved helper")
        self.assertEqual(self.asset.read_bytes(), b"frozen prompt\r\n")
        self.assertEqual(receipt, json.loads((self.output / "package-receipt.json").read_text()))
        metadata = json.loads((self.output / "codex-package.json").read_text())
        self.assertEqual(metadata["entrypoint"], "bin/arcvera.exe")
        self.assertNotIn("opendiyUpdate", metadata)
        self.assertFalse((self.output / "bin/codex.exe").exists())
        for item in receipt["files"]:
            self.assertFalse(Path(item["path"]).is_absolute())
            self.assertEqual(item["sha256"], digest(self.output / item["path"]))

    def test_inherited_update_contract_is_removed_without_changing_vendor_metadata(self):
        metadata = json.loads(self.metadata.read_text())
        metadata["opendiyUpdate"] = {"schema": 2, "minLauncher": "2"}
        self.metadata.write_text(json.dumps(metadata), encoding="utf-8")
        self.build()
        generated = json.loads((self.output / "codex-package.json").read_text())
        self.assertNotIn("opendiyUpdate", generated)
        self.assertEqual(json.loads(self.metadata.read_text()), metadata)

    def test_launcher_uses_adjacent_binary_and_preserves_arguments_exit(self):
        self.build()
        captured = self.root / "argv.json"
        env = dict(os.environ, PACKAGE_FIXTURE_ARGV=str(captured))
        result = subprocess.run(
            'cmd.exe /d /c arcvera.cmd --literal "two words" 한글',
            cwd=self.output, env=env,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )
        self.assertEqual(result.returncode, 17, result.stderr)
        self.assertEqual(captured.read_text(encoding="utf-8").splitlines(),
                         ["--literal", "two words", "한글"])

    def test_remote_command_and_global_options_are_not_rewritten(self):
        self.build()
        captured = self.root / "remote-argv.txt"
        env = dict(os.environ, PACKAGE_FIXTURE_ARGV=str(captured))
        result = subprocess.run(
            'cmd.exe /d /c arcvera.cmd -c "key=value" remote-control status --json',
            cwd=self.output, env=env,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )
        self.assertEqual(result.returncode, 17, result.stderr)
        self.assertEqual(captured.read_text(encoding="utf-8").splitlines(),
                         ["-c", "key=value", "remote-control", "status", "--json"])

    def test_local_launcher_explicitly_selects_embedded_mode(self):
        self.build()
        captured = self.root / "local-argv.txt"
        env = dict(os.environ, PACKAGE_FIXTURE_ARGV=str(captured))
        result = subprocess.run(
            'cmd.exe /d /c arcvera-local.cmd --literal "two words" 한글',
            cwd=self.output, env=env,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )
        self.assertEqual(result.returncode, 17, result.stderr)
        self.assertEqual(captured.read_text(encoding="utf-8").splitlines(),
                         ["--no-daemon", "--literal", "two words", "한글"])

    def test_existing_output_is_not_overwritten(self):
        self.output.mkdir()
        sentinel = self.output / "keep.txt"
        sentinel.write_bytes(b"do not replace")
        with self.assertRaises(ValueError):
            self.build()
        self.assertEqual(sentinel.read_bytes(), b"do not replace")

    def test_changed_frozen_source_is_rejected_before_output(self):
        self.asset.write_bytes(b"different source")
        with self.assertRaisesRegex(ValueError, "Source changed"):
            self.build()
        self.assertFalse(self.output.exists())

    def test_source_head_mismatch_is_rejected(self):
        data = json.loads(self.freeze.read_text())
        data["candidateCommit"] = "0" * 40
        self.freeze.write_text(json.dumps(data), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "Source HEAD"):
            self.build()
        self.assertFalse(self.output.exists())

    def test_changes_outside_changed_file_freeze_are_rejected(self):
        (self.source / "other.txt").write_bytes(b"changed build input")
        with self.assertRaisesRegex(ValueError, "Source"):
            self.build()
        self.assertFalse(self.output.exists())

    def test_missing_tracked_freeze_entry_is_rejected(self):
        data = json.loads(self.freeze.read_text())
        data["files"] = [entry for entry in data["files"] if entry["path"] != "other.txt"]
        self.freeze.write_text(json.dumps(data), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "Source file set"):
            self.build()
        self.assertFalse(self.output.exists())

    def test_duplicate_freeze_entry_is_rejected(self):
        data = json.loads(self.freeze.read_text())
        data["files"].append(data["files"][0])
        self.freeze.write_text(json.dumps(data), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "Duplicate"):
            self.build()
        self.assertFalse(self.output.exists())

    def test_untracked_source_is_rejected(self):
        (self.source / "untracked.rs").write_bytes(b"// not part of the frozen commit\n")
        with self.assertRaisesRegex(ValueError, "untracked"):
            self.build()
        self.assertFalse(self.output.exists())

    def test_helper_base_version_mismatch_is_rejected(self):
        data = json.loads(self.metadata.read_text())
        data["version"] = "0.164.0"
        self.metadata.write_text(json.dumps(data), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "helper package"):
            self.build()
        self.assertFalse(self.output.exists())

    def test_binary_version_mismatch_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "Unexpected CLI version"):
            self.build("0.165.0+different")
        self.assertFalse(self.output.exists())

    def test_vendor_output_recursion_is_rejected(self):
        self.output = self.vendor / "nested-output"
        with self.assertRaises(ValueError):
            self.build()
        self.assertFalse(self.output.exists())

    def test_frozen_path_cannot_escape_source(self):
        outside = self.root / "outside.txt"
        outside.write_bytes(b"outside")
        data = json.loads(self.freeze.read_text())
        data["files"] = [{"path": "../outside.txt", "sha256": digest(outside)}]
        self.freeze.write_text(json.dumps(data), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "Not a package file"):
            self.build()
        self.assertFalse(self.output.exists())

    def triple(self):
        binaries = {}
        for name, filename in (("cli", "arcvera.exe"), ("app-server", "codex-app-server.exe"),
                               ("code-mode-host", "codex-code-mode-host.exe")):
            target = self.root / "candidate" / filename
            target.parent.mkdir(exist_ok=True)
            shutil.copy2(self.binary, target)
            binaries[name] = target
            (self.vendor / "bin" / filename).write_bytes(f"official {name}".encode())
        data = json.loads(self.freeze.read_text())
        data["executables"] = {name: {"sourceCommit": data["candidateCommit"], "sha256": digest(path)}
                               for name, path in binaries.items()}
        self.freeze.write_text(json.dumps(data), encoding="utf-8")
        metadata = json.loads(self.metadata.read_text())
        metadata["version"] = "0.167.0"
        self.metadata.write_text(json.dumps(metadata), encoding="utf-8")
        return binaries

    def build_triple(self, binaries, **overrides):
        arguments = dict(upstream_version="0.167.0", app_server=binaries["app-server"],
                         code_mode_host=binaries["code-mode-host"])
        arguments.update(overrides)
        return package(self.source, self.freeze, binaries["cli"], self.vendor, self.output,
                       "0.168.0", **arguments)

    def test_semver_triple_preserves_separate_upstream_resources_and_attests_all_executables(self):
        binaries = self.triple()
        originals = {path.relative_to(self.vendor).as_posix(): digest(path)
                     for path in self.vendor.rglob("*") if path.is_file()}
        receipt = self.build_triple(binaries)
        self.assertEqual(receipt["upstreamVersion"], "0.167.0")
        self.assertEqual(set(receipt["executables"]), set(binaries))
        for name, path in binaries.items():
            attestation = receipt["executables"][name]
            self.assertEqual(attestation["sourceCommit"], receipt["sourceCommit"])
            self.assertEqual(attestation["sha256"], digest(path))
            self.assertEqual(digest(self.output / attestation["path"]), digest(path))
            self.assertTrue(attestation["version"].endswith(" 0.168.0"))
        self.assertEqual((self.output / "helper.dat").read_bytes(), b"preserved helper")
        self.assertEqual(originals, {path.relative_to(self.vendor).as_posix(): digest(path)
                                     for path in self.vendor.rglob("*") if path.is_file()})

    def test_triple_requires_explicit_upstream_version(self):
        with self.assertRaisesRegex(ValueError, "upstream_version"):
            self.build_triple(self.triple(), upstream_version=None)
        self.assertFalse(self.output.exists())

    def test_triple_requires_both_helper_candidates(self):
        with self.assertRaisesRegex(ValueError, "both"):
            self.build_triple(self.triple(), code_mode_host=None)
        self.assertFalse(self.output.exists())

    def test_triple_requires_frozen_executable_manifest_and_every_entry(self):
        binaries = self.triple()
        for missing in ("executables", "app-server"):
            data = json.loads(self.freeze.read_text())
            if missing == "executables":
                saved = data.pop("executables")
            else:
                data["executables"] = saved
                data["executables"].pop(missing)
            self.freeze.write_text(json.dumps(data), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "executable"):
                self.build_triple(binaries)
            self.assertFalse(self.output.exists())

    def test_triple_rejects_foreign_executable_source_commit(self):
        binaries = self.triple()
        data = json.loads(self.freeze.read_text())
        data["executables"]["app-server"]["sourceCommit"] = "a" * 40
        self.freeze.write_text(json.dumps(data), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "source commit"):
            self.build_triple(binaries)
        self.assertFalse((self.output / "package-receipt.json").exists())

    def test_triple_rejects_missing_or_mismatched_executable_hash(self):
        binaries = self.triple()
        for expected in (None, "0" * 64):
            data = json.loads(self.freeze.read_text())
            data["executables"]["code-mode-host"]["sha256"] = expected
            self.freeze.write_text(json.dumps(data), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "hash"):
                self.build_triple(binaries)
            self.assertFalse((self.output / "package-receipt.json").exists())

    def test_triple_rejects_mismatched_or_failing_helper_native_version(self):
        binaries = self.triple()
        for filename, error in (("wrong-server.exe", ValueError), ("failing-version.exe", subprocess.CalledProcessError)):
            helper = self.root / filename
            shutil.copy2(self.binary, helper)
            with self.assertRaises(error):
                self.build_triple(binaries, app_server=helper)
            self.assertFalse((self.output / "package-receipt.json").exists())

    def test_triple_cli_option_path_records_three_executables(self):
        binaries = self.triple()
        subprocess.run(["python", str(Path(__file__).with_name("package_cli.py")),
                        "--source", str(self.source), "--freeze", str(self.freeze),
                        "--binary", str(binaries["cli"]), "--vendor", str(self.vendor),
                        "--output", str(self.output), "--version", "0.168.0",
                        "--upstream-version", "0.167.0", "--app-server", str(binaries["app-server"]),
                        "--code-mode-host", str(binaries["code-mode-host"])],
                       check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.assertEqual(len(json.loads((self.output / "package-receipt.json").read_text())["executables"]), 3)

    def test_triple_rejects_non_semver_release_versions(self):
        binaries = self.triple()
        for version in ("R41", "01.168.0", "0.168.0-01"):
            with self.assertRaisesRegex(ValueError, "SemVer"):
                package(self.source, self.freeze, binaries["cli"], self.vendor, self.output,
                        version, upstream_version="0.167.0", app_server=binaries["app-server"],
                        code_mode_host=binaries["code-mode-host"])
            self.assertFalse(self.output.exists())


if __name__ == "__main__":
    unittest.main()
