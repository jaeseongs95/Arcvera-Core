<!-- Modified for OpenDIY Codex R33 (2026-11-02); see MODIFICATIONS.md. -->
# Windows CLI packaging tool

Creates a **new local package** with the built CLI and matching official helper resources. Packaging does not change PATH, authentication, daemon state, or user configuration.

## Arcvera-Core release packaging / 릴리스 패키징

Arcvera-Core 1.0.0 uses official Codex 0.167.0 helper resources. Pass the upstream version separately and supply all three built candidates from one frozen commit:

```text
python package_cli.py --source CHECKOUT --freeze source-freeze.json --binary BUILT_CODEX_EXE --app-server BUILT_APP_SERVER_EXE --code-mode-host BUILT_CODE_MODE_HOST_EXE --upstream-version 0.167.0 --vendor OFFICIAL_VENDOR_DIRECTORY --output NEW_DIRECTORY --version 1.0.0
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

Arcvera-Core 1.0.0은 공식 Codex 0.167.0의 보조 리소스를 사용합니다. 위 명령처럼 공식 기반 버전을 별도로 지정하고, 같은 고정 커밋에서 빌드한 CLI·앱 서버·CodeMode 호스트를 모두 전달합니다. 동결 명세에는 `candidateCommit`·`files`와 위 `executables` 항목이 필요합니다. 빌드 담당자가 실제 빌드 영수증을 기준으로 각 실행 파일의 전체 소스 커밋과 SHA-256을 기록해야 합니다.

도구는 실행 파일의 커밋·해시·실제 `--version` 출력과 복사 후 신원을 확인한 뒤 완료 영수증을 씁니다. 이 영수증은 패키지 완결성과 신원을 나타내며 실제 동작·릴리스 수용 증거는 별도로 필요합니다. 기존 CLI 단독 API 호출은 유지됩니다. 프로젝트 버전과 공식 기반 버전이 다른 새 릴리스에는 위 명시 옵션을 사용합니다.

`codex-patched.cmd`는 `remote-control`과 전역 옵션을 포함한 인자를 인접 실행 파일에 그대로 전달한다. R33의 암시적 로컬 연결은 CLI·서버의 전체 버전을 검사하고 호환되지 않으면 내장 백엔드로 전환한다. 관리 서버·명시적 원격 연결의 실제 버전과 선택 상태는 별도로 확인해야 한다. 일치하는 관리 서버 설치·전환은 명시적 수동 단계(`bin\codex.exe app-server daemon update --from-cli`)이며 공유 서버를 재시작하고 다른 클라이언트의 작업에 영향을 줄 수 있다. 실행 중 서버의 경로·해시를 확인하기 전에는 패치 활성화나 모바일과 동일한 백엔드 사용이 검증됐다고 보고하지 않는다.

`codex-local.cmd` explicitly adds `--no-daemon` for an isolated embedded backend. It cannot be used for managed `remote-control` commands or as evidence that a mobile client and local TUI share a live backend. Packaging does not perform enrollment, pairing, or installation. OpenDIY Desktop's panel-owned WebSocket servers also require a separate backend-identity and lifecycle check; sharing a profile directory is not sufficient.

Windows 자동업데이트와 숨김 예약 실행 계약은 [UPDATE.ko.md](UPDATE.ko.md)를 따른다. 자동 CLI 선택은 기존 공유 서버를 종료·교체하지 않는다.

```text
python package_cli.py --source CHECKOUT --freeze source-freeze.json --binary BUILT_CODEX_EXE --vendor OFFICIAL_VENDOR_DIRECTORY --output NEW_DIRECTORY --version PATCHED_VERSION
```

The source-freeze manifest contains `candidateCommit` and `files` entries with relative `path` and `sha256`. The tool checks HEAD, tracked changes, manifest hashes, helper base version, executable version, and copied file hashes. A complete-copy receipt is written last. A receipt does not prove a successful build, independent audit, or correct backend operation; those checks remain required before installation.

Test the package and launcher with a disposable argv fixture, not a user profile:

```text
rustc --edition=2024 argv_fixture.rs -o argv-fixture.exe
```

Set `PACKAGE_FIXTURE_EXE` to that executable's absolute path, then run `python -m unittest -v test_package_cli`. Tests use temporary Git repositories and new output directories; the fixture handles no real Codex requests. Source files and build tools may be published; generated packages, executables, and local receipts are not source deliverables.
