<!-- Modified for OpenDIY Codex R33 (2026-11-02); see MODIFICATIONS.md. -->
# Windows CLI packaging tool

Creates a **new local package** with the built CLI and matching official helper resources. Packaging does not change PATH, authentication, daemon state, or user configuration.

## AGPL and corresponding source / AGPL과 대응소스

Core 1.0.0 and every subsequent project version use `AGPL-3.0-or-later`. Packages carry the full license, original Apache license and preserved notices in `licenses/arcvera-core/`, the exact committed project source in `source/arcvera-core-source.zip`, and the bilingual source offer in `SOURCE.md`. The archive is checked against every Git blob and the complete frozen file set. Vendor files colliding with these delivery paths are rejected before output creation; other vendor notices remain unchanged. The CLI entrypoint is `bin/arcvera.exe`; inherited `opendiyUpdate` metadata is removed. See [BUILD.md](BUILD.md) and [LICENSING.md](../../LICENSING.md). Dependency/native corresponding source and the source offer visible to actual network users require separate release checks.

Core 1.0.0과 이후 모든 프로젝트 버전에 `AGPL-3.0-or-later`를 적용합니다. 패키지는 `licenses/arcvera-core/`에 전문·원 Apache 라이선스·고지를, `source/arcvera-core-source.zip`에 정확한 커밋의 프로젝트 소스를, `SOURCE.md`에 영한 소스 제공 안내를 포함합니다. 소스 압축파일을 전체 Git blob·동결 파일 집합과 대조합니다. 제공 경로와 충돌하는 벤더 파일은 출력 생성 전에 거부하며 다른 벤더 고지는 보존합니다. CLI 진입점은 `bin/arcvera.exe`이며 상속된 `opendiyUpdate` 메타데이터를 제거합니다. 의존성·네이티브 대응소스와 실제 네트워크 사용자에게 보이는 소스 제공 안내는 별도 릴리스 검증 대상입니다.

## Arcvera-Core release packaging / 릴리스 패키징

Arcvera-Core 1.0.1 uses official Codex 0.167.0 helper resources. Pass the upstream version separately and supply all three built candidates from one frozen commit:

```text
python package_cli.py --source CHECKOUT --freeze source-freeze.json --binary BUILT_ARCVERA_EXE --app-server BUILT_APP_SERVER_EXE --code-mode-host BUILT_CODE_MODE_HOST_EXE --upstream-version 0.167.0 --vendor OFFICIAL_VENDOR_DIRECTORY --output NEW_DIRECTORY --version 1.0.1
```

In addition to `candidateCommit` and `files`, the freeze manifest must contain:

```json
{
  "executables": {
    "cli": {"sourceCommit": "FULL_CANDIDATE_COMMIT", "sha256": "CLI_SHA256"},
    "app-server": {"sourceCommit": "FULL_CANDIDATE_COMMIT", "sha256": "SERVER_SHA256"},
    "code-mode-host": {"sourceCommit": "FULL_CANDIDATE_COMMIT", "sha256": "HOST_SHA256"}
  }
}
```

The build owner creates these entries from the actual build receipts. Every executable must match that commit and hash, and report its exact project version through `--version`. The tool checks the copied executables again before writing `package-receipt.json`. The receipt confirms package completeness and identity; runtime and release acceptance require separate evidence. Existing single-CLI callers remain supported. Independent project/upstream versions require the explicit options above; do not use the single-CLI fallback for a new Arcvera-Core release.

Arcvera-Core 1.0.1은 공식 Codex 0.167.0의 보조 리소스를 사용합니다. 위 명령처럼 공식 기반 버전을 별도로 지정하고, 같은 고정 커밋에서 빌드한 CLI·앱 서버·CodeMode 호스트를 모두 전달합니다. 동결 명세에는 `candidateCommit`·`files`와 위 `executables` 항목이 필요합니다. 빌드 담당자가 실제 빌드 영수증을 기준으로 각 실행 파일의 전체 소스 커밋과 SHA-256을 기록해야 합니다.

도구는 실행 파일의 커밋·해시·실제 `--version` 출력과 복사 후 신원을 확인한 뒤 완료 영수증을 씁니다. 이 영수증은 패키지 완결성과 신원을 나타내며 실제 동작·릴리스 수용 증거는 별도로 필요합니다. 기존 CLI 단독 API 호출은 유지됩니다. 프로젝트 버전과 공식 기반 버전이 다른 새 릴리스에는 위 명시 옵션을 사용합니다.

`arcvera.cmd`는 `remote-control`과 전역 옵션을 포함한 인자를 인접 실행 파일에 그대로 전달한다. R33의 암시적 로컬 연결은 CLI·서버의 전체 버전을 검사하고 호환되지 않으면 내장 백엔드로 전환한다. 관리 서버·명시적 원격 연결의 실제 버전과 선택 상태는 별도로 확인해야 한다. 일치하는 관리 서버 설치·전환은 명시적 수동 단계(`bin\arcvera.exe app-server daemon update --from-cli`)이며 공유 서버를 재시작하고 다른 클라이언트의 작업에 영향을 줄 수 있다. 실행 중 서버의 경로·해시를 확인하기 전에는 패치 활성화나 모바일과 동일한 백엔드 사용이 검증됐다고 보고하지 않는다.

`arcvera-local.cmd` explicitly adds `--no-daemon` for an isolated embedded backend. It cannot be used for managed `remote-control` commands or as evidence that a mobile client and local TUI share a live backend. Packaging does not perform enrollment, pairing, or installation. OpenDIY Desktop's panel-owned WebSocket servers also require a separate backend-identity and lifecycle check; sharing a profile directory is not sufficient.

Arcvera-Core 자동업데이트는 비활성화되어 있습니다. [수동 업데이트 안내](UPDATE.ko.md)를 따르십시오.

```text
python package_cli.py --source CHECKOUT --freeze source-freeze.json --binary BUILT_ARCVERA_EXE --vendor OFFICIAL_VENDOR_DIRECTORY --output NEW_DIRECTORY --version PATCHED_VERSION
```

The source-freeze manifest contains `candidateCommit` and `files` entries with relative `path` and `sha256`. The tool checks HEAD, tracked changes, manifest hashes, helper base version, executable version, and copied file hashes. A complete-copy receipt is written last. A receipt does not prove a successful build, independent audit, or correct backend operation; those checks remain required before installation.

Test the package and launcher with a disposable argv fixture, not a user profile:

```text
rustc --edition=2024 argv_fixture.rs -o argv-fixture.exe
```

Set `PACKAGE_FIXTURE_EXE` to that executable's absolute path, then run `python -m unittest -v test_package_cli`. Tests use temporary Git repositories and new output directories; the fixture handles no real Codex requests. Source files and build tools may be published; generated packages, executables, and local receipts are not source deliverables.
# External runtime packaging / 외부 런타임 패키징

Microsoft CRT DLLs and installers are not distributed. Users install the
runtime directly from Microsoft as described in [RUNTIME.md](RUNTIME.md).
`package_cli.py` rejects Microsoft runtime payload names. `external_runtime.py`
converts a previously frozen package without rebuilding any executable:

```powershell
python tools/windows-cli/external_runtime.py --base <frozen-package> --base-receipt-sha256 <approved-sha256> --source <clean-committed-source> --output <new-package>
```

The base inventory must match its approved receipt. The result retains the
native build commit and original corresponding source; `packaging.sourceCommit`
and `source/packaging-source.zip` identify the separate packaging revision.
The original receipt is included at `source/build-package-receipt.json`.
Run `python -m unittest discover -s tools/windows-cli -p test_external_runtime.py -v`.

Microsoft CRT DLL과 설치기는 배포하지 않습니다. 사용자는 [RUNTIME.md](RUNTIME.md)의
공식 경로로 직접 설치합니다. 위 명령은 새 출력 경로에만 재포장하며 실행
파일을 재빌드하지 않습니다. 기존 빌드 커밋과 대응소스는 보존하고 패키징
커밋·소스·원본 영수증을 별도로 기록합니다. 설치나 운영 버전 교체는 하지 않습니다.
