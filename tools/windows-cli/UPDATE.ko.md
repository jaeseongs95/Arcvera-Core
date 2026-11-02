# Windows 자동업데이트

`update.ps1`은 Windows PowerShell 5.1만 사용한다. 기존 공개 저장소 `jaeseongs95/opendiy-codex`의 최신 정식 릴리스를 확인하고, `opendiy-codex-rNN-windows-x86_64.zip`과 `SHA256SUMS.txt`의 정확한 ZIP 항목을 검증한다. Windows x86_64만 지원한다. ZIP은 최상위 `package-receipt.json`, `codex-package.json`, `bin/codex.exe`, `bin/codex-app-server.exe`, `tools/opendiy-codex-launcher.exe`, `updater/update.ps1`, `updater/update.vbs`를 포함하며 모든 파일이 receipt에 있어야 한다.

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File PACKAGE\updater\update.ps1 -Action Update -InstallationRoot ROOT -PackageRoot PACKAGE -ManifestSha256 RECEIPT_SHA256
```

`PACKAGE`와 `RECEIPT_SHA256`은 현재 선택된 패키지 또는 보존한 초기 패키지 및 그 `package-receipt.json` SHA-256이다. `Update`는 검증한 패키지를 새 `ROOT/versions/` 폴더에 보관하고 `ROOT/current-package.json`을 원자 교체하므로 다음 CLI 실행이 새 패키지를 선택한다. 기존 실행 파일·패키지·공유 서버는 보존한다. 서버에 종료·업데이트를 요청하지 않으며 CLI 선택 성공과 서버 전환 성공을 합치지 않는다.

패키지 전체 버전은 `정식 또는 alpha 기반 버전+native-rNN.8자리날짜` 형식이며, 릴리스 태그는 이 문자열의 `+native-r`를 `-r`로 바꾸고 `v`를 앞에 붙인 값과 정확히 같아야 한다. 필수 receipt 경로의 소문자 표기도 native launcher와 동일하게 검사한다.

`package_cli.py`는 `codex-package.json`에 `opendiyUpdate:{schema:1,minLauncher:1}`을 생성한다. 고정 bootstrap launcher 1이 지원하는 두 정수 값 1만 자동 선택하며, 다른 요구나 누락은 선택 전에 거부한다. 지원 범위를 바꾸려면 별도 수동 bootstrap 업그레이드가 필요하다.

같은 인자에서 `-Action Check`는 확인만, `Status`는 로컬 선택 조회, `Enable -IntervalMinutes 60`은 사용자 작업 스케줄러 한 항목 등록, `Disable`은 그 항목 비활성화다. 간격은 5~1440분이다. 예약 실행은 OS의 `wscript.exe → ROOT/updater/update.vbs -Dispatch ROOT → ROOT/opendiy-codex-launcher.exe update`로 창 없이 실행한다. launcher는 현재 선택된 CLI와 그 패키지의 검증된 helper를 실행하므로 후속 릴리스의 updater 개선도 반영된다. `Enable`은 stable launcher를 초기 receipt의 `tools/opendiy-codex-launcher.exe` 해시와 대조하고 보존한다. VBS의 직접 helper 실행 모드는 시스템 PowerShell에 `-WindowStyle Hidden`을 전달한다.

초기 포인터는 `schema:1`, `installationRoot`, `bootstrapPackageRoot`, `bootstrapManifestSha256`, `packageRoot`, `manifestSha256`, 숫자 `revision`, 전체 `version`(`codex-cli ` 접두 제외), `sourceCommit`을 담는다. 두 package root는 `ROOT/versions`의 직계 자식이고 초기 receipt 결속은 갱신 뒤에도 보존한다. 릴리스 ZIP의 `archiveSha256`도 초기 설치 때 넣어야 같은 revision의 ZIP 교체를 첫 확인부터 탐지한다. 없으면 첫 동일 revision 확인에서 기존 receipt와 비교한 뒤 ZIP 해시를 결속한다. 모든 포인터 작성자는 `ROOT/update.lock`의 배타 잠금을 따라야 한다.

표준 출력은 JSON 한 줄이다. 정상 종료 0의 `status`는 `updated`, `current`, `available`, `enabled`, `disabled`이며 `cli`에 실제 선택된 패키지와 전체 버전·해시·revision이 있다. `daemon`은 항상 `{status:"preserved",updateRequested:false}`다. 오류는 종료 1과 `status:"error"`, `error.code`, `error.message`로 구분한다. 같은 revision의 다른 해시와 자동 downgrade는 거부한다. 원격 스크립트는 내려받아 실행하지 않는다.

잠금과 설치 결속 검증을 마친 작업의 결과는 비밀 없는 JSON과 UTC 시각으로 `ROOT/last-update.json`에 원자 저장한다. `Status`는 현재 선택 사실과 별개인 `lastResult`에 이를 보여주므로 숨김 예약 작업의 실패 원인을 확인할 수 있다. 잠금 경합·잘못된 설치 입력은 이전 결과를 덮어쓰지 않는다. 원격 응답·URL·원시 예외문은 저장하지 않는다.

`Status.schedule`은 실제 작업 스케줄러를 읽기 전용으로 조회해 `taskName`, `state`(`enabled`, `disabled`, `not_registered`, `conflict`, `unavailable`)와 조회 가능한 `intervalMinutes`를 제공한다. 마지막 확인 결과가 `current`여도 예약 활성 여부는 이 별도 값으로 확인한다. 정확한 실행 동작·사용자·실행 권한이 다른 작업은 `conflict`로 표시한다.

격리 시험은 다음 명령으로 실행한다. 새 시험 폴더와 receipt는 `D:/codex/build/` 아래에 남긴다. 실제 네트워크·사용자 프로필·스케줄러 등록을 사용하지 않으며, HTTP 전송만 파일 fixture로 대체한다. 실제 ZIP 검사·원자 포인터 교체·독립 프로세스 잠금·숨김 VBS 실행을 검증한다.

```powershell
powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File tools/windows-cli/test_update.ps1
```
