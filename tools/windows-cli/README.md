<!-- Modified for OpenDIY Codex R33 (2026-11-02); see MODIFICATIONS.md. -->
# Windows CLI packaging tool

Creates a **new local package** with the built CLI and matching official helper resources. Packaging does not change PATH, authentication, daemon state, or user configuration.

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
