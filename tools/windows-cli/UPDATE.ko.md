# Arcvera-Core 업데이트 / Updates

## English

Arcvera-Core 1.0.1 disables inherited OpenAI and OpenDIY automatic installers, update checks and scheduling. Download the complete new package from [Arcvera-Core releases](https://github.com/jaeseongs95/Arcvera-Core/releases), verify `SHA256SUMS.txt`, and extract it into a new directory. Use `bin\arcvera.exe` or `arcvera.cmd`. Retain the previous package for recovery.

`arcvera update` and daemon updater commands without `--from-cli` intentionally fail with manual-download guidance. Old PowerShell updater scripts remain historical source; they are not the Arcvera release update mechanism and must not be used for 1.0.1.

To replace Arcvera's managed shared server using a verified complete local package, first finish active work and follow the [release procedure](../../docs/releases/v1.0.1.md#shared-server-update). `app-server daemon update --from-cli` can restart that server and interrupt connected clients. It does not import the official Codex home. Extracting a ZIP alone does not replace a running server.

## 한국어

Arcvera-Core 1.0.1은 상속된 OpenAI·OpenDIY 자동 설치기, 업데이트 확인과 예약을 비활성화합니다. [Arcvera-Core 릴리스](https://github.com/jaeseongs95/Arcvera-Core/releases)에서 새 전체 패키지를 받고 `SHA256SUMS.txt`를 확인한 뒤 새 디렉터리에 풉니다. `bin\arcvera.exe` 또는 `arcvera.cmd`로 실행하고 복구용 이전 패키지를 보관하십시오.

`arcvera update`와 `--from-cli` 없는 daemon updater 명령은 직접 다운로드 안내와 함께 실패하도록 되어 있습니다. 과거 PowerShell updater 스크립트는 이력 소스로 남아 있으며 Arcvera 릴리스 업데이트 수단이 아닙니다. 1.0.1에 사용하지 마십시오.

검증한 전체 로컬 패키지로 Arcvera 관리 공유 서버를 교체하려면 진행 중 작업을 마친 뒤 [릴리스 절차](../../docs/releases/v1.0.1.md#공유-서버-업데이트)를 따르십시오. `app-server daemon update --from-cli`는 서버를 재시작하여 연결 클라이언트 작업을 중단할 수 있습니다. 공식 Codex 홈을 가져오지 않습니다. ZIP 압축 해제만으로 실행 중 서버가 교체되지는 않습니다.
