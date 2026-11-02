# Modified for OpenDIY Codex R33 (2026-11-03); see MODIFICATIONS.md.
"""Windows host regression using an explicitly supplied, verified native package.

No real installation, network update, task registration, daemon or user HOME is used.
The package is copied into a fresh D:/codex/build directory; executable hard links
are read-only inputs. Only the copied helper and its synthetic receipt are changed.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def run(argv, output, name):
    completed = subprocess.run(argv, stdin=subprocess.DEVNULL, capture_output=True,
                               creationflags=subprocess.CREATE_NO_WINDOW, timeout=60)
    (output / (name + '.stdout.raw')).write_bytes(completed.stdout)
    (output / (name + '.stderr.raw')).write_bytes(completed.stderr)
    result = {'argv': argv, 'exitCode': completed.returncode,
              'stdoutSha256': hashlib.sha256(completed.stdout).hexdigest(),
              'stderrSha256': hashlib.sha256(completed.stderr).hexdigest()}
    (output / (name + '.receipt.json')).write_text(
        json.dumps(result, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    return completed


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--package', type=Path, required=True)
    parser.add_argument('--updater', type=Path, required=True)
    parser.add_argument('--work-root', type=Path, required=True)
    args = parser.parse_args()
    if os.name != 'nt':
        parser.error('Windows native host required')
    work = args.work_root.resolve()
    build = Path('D:/codex/build').resolve()
    if not work.is_relative_to(build) or work == build or work.exists():
        parser.error('work-root must be a new directory beneath D:/codex/build')
    receipt = json.loads((args.package / 'package-receipt.json').read_text(encoding='utf-8-sig'))
    for entry in receipt['files']:
        source = (args.package / entry['path']).resolve()
        if not source.is_relative_to(args.package.resolve()) or sha(source) != entry['sha256']:
            raise RuntimeError('Supplied package failed verification: ' + entry['path'])
    work.mkdir(parents=True)
    install = work / '한글-설치'
    package = install / 'versions/r33-test-only'
    package.mkdir(parents=True)
    for entry in receipt['files']:
        source = args.package / entry['path']
        target = package / entry['path']
        target.parent.mkdir(parents=True, exist_ok=True)
        if source.suffix.lower() == '.exe':
            os.link(source, target)
        else:
            shutil.copy2(source, target)
    shutil.copy2(args.updater, package / 'updater/update.ps1')
    for entry in receipt['files']:
        entry['sha256'] = sha(package / entry['path'])
    manifest = package / 'package-receipt.json'
    manifest.write_text(json.dumps(receipt, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    digest = sha(manifest)
    version = receipt['version'].removeprefix('codex-cli ')
    pointer = {'schema': 1, 'installationRoot': str(install),
               'bootstrapManifestSha256': digest, 'bootstrapPackageRoot': str(package),
               'packageRoot': str(package), 'manifestSha256': digest, 'revision': 33,
               'version': version, 'sourceCommit': receipt['sourceCommit']}
    (install / 'current-package.json').write_text(json.dumps(pointer), encoding='utf-8')
    launcher = install / 'opendiy-codex-launcher.exe'
    os.link(package / 'tools/opendiy-codex-launcher.exe', launcher)
    argv = [str(launcher), 'update', '--status']
    piped = run(argv, work, 'piped-status')
    utf8_error = None
    try:
        decoded = json.loads(piped.stdout.decode('utf-8', errors='strict'))
        utf8_ok = decoded['cli']['packageRoot'] == str(package)
    except (UnicodeError, ValueError, KeyError) as error:
        utf8_ok = False
        utf8_error = str(error)
    script = work / 'shell-run.vbs'
    command = subprocess.list2cmdline(argv)
    script.write_text('Option Explicit\r\nDim shell, result\r\n'
                      'Set shell = CreateObject("WScript.Shell")\r\n'
                      'result = shell.Run("' + command.replace('"', '""') + '", 0, True)\r\n'
                      'WScript.Echo "childExit:" & result\r\nWScript.Quit result\r\n', encoding='utf-16')
    system = Path(os.environ['SystemRoot']) / 'System32'
    hidden = run([str(system / 'cscript.exe'), '//Nologo', str(script)], work, 'hidden-status')
    hidden_ok = hidden.returncode == 0 and b'childExit:0' in hidden.stdout
    powershell = system / 'WindowsPowerShell/v1.0/powershell.exe'
    invalid = run([str(powershell), '-NoLogo', '-NoProfile', '-NonInteractive',
                   '-WindowStyle', 'Hidden', '-ExecutionPolicy', 'Bypass', '-File',
                   str(package / 'updater/update.ps1'), '-Action', 'Status',
                   '-InstallationRoot', str(install), '-PackageRoot', str(package),
                   '-ManifestSha256', '0' * 64], work, 'failure-status')
    failure_json = json.loads(invalid.stdout.decode('utf-8', errors='strict'))
    failure_ok = (invalid.returncode == 1 and failure_json['status'] == 'error'
                  and failure_json['daemon']['updateRequested'] is False)
    result = {'schema': 1, 'syntheticPackageOnly': True, 'binaryRebuilt': False,
              'updaterSha256': sha(args.updater), 'nativeCliSha256': receipt['binarySha256'],
              'strictUtf8Json': utf8_ok, 'utf8Error': utf8_error,
              'pipedExitCode': piped.returncode, 'hiddenExitCode': hidden.returncode,
              'hiddenCompleted': hidden_ok, 'helperFailureStillFails': failure_ok,
              'passed': piped.returncode == 0 and utf8_ok and hidden_ok and failure_ok}
    (work / 'result.json').write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(result, ensure_ascii=False))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
