# OpenDIY Codex

[English](README.md) · [한국어](README.ko.md)

Windows용 Codex CLI·Rust App Server 비공식 수정판입니다. 요청 취소와 프로세스 정리, 대화별 제어, 예약 실행, 선택형 진단을 보강합니다.

[R33 다운로드](https://github.com/jaeseongs95/opendiy-codex/releases/tag/v0.166.0-r33.20261102) · [소스 빌드](docs/source-import.md) · [검증 기록](docs/TESTING.md) · [변경 내역](MODIFICATIONS.md)

## 배포와 개발 상태

| 구분 | 공식 기반 | 상태 |
| --- | --- | --- |
| **R33 공개판** | 0.166.0 | `0.166.0+native-r33.20261102`; Windows CLI·별도 앱 서버·휴대용 패키지 제공 |
| **R33 업데이트 수정 후보** | 0.166.0 | 자동업데이트 출력·예약 소유자 판정 수정. 소스·격리시험 검토 완료, 미배포 |
| **R32** | 0.166.0 | 이전 공개판. 기존 설치를 보존해 복구에 사용 가능 |
| **R31** | 0.166.0 | 별도 릴리스 취소, 수정 사항은 R32에 통합 |

**R33 자동업데이트에는 알려진 Windows 결함이 있습니다.** 한글 경로를 포함한 출력의 인코딩 때문에 숨김 실행이 실패하고, 같은 사용자를 예약 소유권 충돌로 오인할 수 있습니다. 현재 소스 `bd2ed8ec7854d64905caa67d0f77758e758a930c`는 두 결함의 수정 후보입니다. 원본에서 실패하고 후보에서 성공하는 격리 시험과 새 소스 독립검토를 마쳤지만, 공개 R33 실행 파일이나 실제 설치본에 수정이 적용됐다는 뜻은 아닙니다. 실제 예약 실행·후속 패키지·배포 검증은 남아 있습니다. [수정과 검증 범위](docs/r33-updater-postinstall-fix.ko.md)를 확인하세요.

공개 R33의 CLI·서버는 `36ae395a552fd55962e19409cbf29b1fdf03b0ec`에서 빌드됐습니다. 공개 자산 5개는 실제 익명 다운로드 후 해시를 대조했고, Windows 설치·바로가기 전환과 OpenDIY Desktop 1.0.2 공동시험을 확인했습니다. 자동업데이트 사후감사는 위 두 결함 때문에 통과하지 못했습니다. 이전 실패 기록과 미검증 범위는 [검증 기록](docs/r33-development-verification.ko.md)에 보존하며, [소스 출처](SOURCE_BASELINE.json)는 미배포 수정 후보를 가리킵니다.

## 주요 기능

- **취소와 정리:** 대기 요청의 취소를 전달하고, 용량 부족 재시도를 제한하며 직접 소유한 하위 프로세스를 정리합니다.
- **대화별 제어:** 컨텍스트 창 크기와 메모리 교정 기록을 관리하고 하위 에이전트 상태를 확인합니다.
- **예약 실행:** CLI가 켜진 동안 요청을 예약합니다. 작업 중인 대화는 해당 회차를 건너뜁니다.
- **화면 이력과 언어:** 표시 이력의 보유 구간을 제한하고 영어·한국어 화면을 선택합니다. 모델의 원본 대화는 유지합니다.
- **선택형 진단:** 이벤트를 버퍼에 모아 완결된 파일로 내보냅니다. R32는 응답·송신 캐시 진단의 배치 전달과 실행 식별을 보강했습니다.

R33에는 이력·버퍼가 겹칠 때의 중복 표시 수정, 비강제 종료 재요청의 drain 보존, 공통 cleanup 시작 경로와 로컬 CLI·서버의 전체 버전 확인이 추가됩니다. Windows 자동업데이트 소스는 검증한 패키지를 다음 CLI 실행에서 선택하도록 작성했으며 기존 패키지와 공유 daemon을 보존합니다. 공개 R33에는 이 변경이 포함되며, 자동업데이트 결함은 위에 따로 안내합니다.

## Windows에서 실행

1. [R33 릴리스](https://github.com/jaeseongs95/opendiy-codex/releases/tag/v0.166.0-r33.20261102)에서 `opendiy-codex-r33-windows-x86_64.zip`과 `SHA256SUMS.txt`를 받습니다.
2. SHA-256을 확인하고 기존 설치를 보존한 채 새 폴더에 압축을 풉니다.
3. 기존 공유 서버와 분리된 로컬 엔진을 사용하려면 `codex-local.cmd`를 실행합니다. 패키지의 `USAGE.ko.md`를 참고하세요.

개별 `codex.exe`·`codex-app-server.exe`도 제공하지만 보조 파일이 필요한 기능에는 전체 패키지가 필요합니다. 기본 CLI와 `codex-patched.cmd`는 기존 공유 서버에 연결할 수 있으므로 실제 서버의 버전은 별도로 확인해야 합니다.

## 명령을 입력하는 곳

다음 명령은 R30에서 제공했고 R33에도 이어지는 대화 입력창 명령입니다. PowerShell 명령이 아닙니다.

| 명령 | 용도 |
| --- | --- |
| `/cron add --every 10m -- 빌드 완료 여부를 확인해줘` | 반복 요청 등록 |
| `/cron list` · `/cron remove 1` | 예약 조회·제거. 제거할 때 실제 ID 사용 |
| `/context 120000` · `/context reset` | 대화별 컨텍스트 창 설정·해제 |
| `/memory view` · `/memory edit <교정 내용>` | 메모리 조회·후속 통합용 교정 기록 추가 |
| `/subagents` | 하위 에이전트 대화 확인 |
| `/background` 또는 `F6` | 지원되는 터미널 작업을 유지하면서 제어 반환 |
| `/reload_plugin` | 현재 대화의 플러그인 다시 불러오기 요청 |
| `/language ko` · `/language en` | 다음 실행부터 사용할 전역 화면 언어 저장 |

컨텍스트 설정은 모델 카탈로그의 한도를 늘리지 않습니다. 메모리 교정은 기존 요약을 즉시 바꾸지 않습니다. 예약은 CLI 종료 시 사라지며 자연어 예약에는 대화의 `cron` 도구 지원이 필요합니다. [예약 안내](docs/native-cli-cron.ko.md)와 [컨텍스트·언어·로그 안내](docs/native-cli-r27.ko.md)를 참고하세요.

아래는 **R33의 PowerShell 명령**입니다. 대화 입력창의 슬래시 명령이 아니며 R32 다운로드에서는 제공하지 않습니다.

```powershell
codex update --check
codex update --status
codex update --enable-updates --interval-minutes 60
codex update --disable-updates
codex update
```

자동업데이트용 설치 구성이 필요한 명령입니다. 전역 feature 옵션 `--enable`·`--disable`과 구분하며, helper·예약·패키지 요구사항은 [자동업데이트 안내](tools/windows-cli/UPDATE.ko.md)를 따릅니다.

## 검증과 연동

로그는 명시적으로 선택해 수집합니다. 파일 내보내기 성공이 클라우드 업로드를 뜻하지 않으며, 캐시 진단 시험은 실제 서버의 캐시 적중률 향상을 입증하지 않습니다. 화면 이력 제한도 프로세스 전체 메모리 상한은 아닙니다.

[OpenDIY Desktop](https://github.com/jaeseongs95/opendiy-desktop)은 별도 화면·다중 세션 프로젝트입니다. 공개 R33(`36ae395a552fd55962e19409cbf29b1fdf03b0ec`)과 Desktop 1.0.2(`7069c80463cf60b2cf5f55023a42ca93f0bf57c2`)의 공동시험에서 중지 상태의 서버 전환·기존 서버 연결·격리 GUI 복원과 하위 세션 관리를 확인했습니다. 외부 서버 교체 뒤 Desktop에 상태 미확인이 남으면 앱을 직접 다시 열어야 하는 한계가 있습니다. 자동업데이트 수정 후보의 설치 후 조합, 실제 모바일 연결·장시간 부하·원래 지연과 메모리 증가의 해결은 미검증입니다.

## 빌드와 라이선스

[소스 빌드 안내](docs/source-import.md)와 [Windows 패키징 도구](tools/windows-cli/README.md)를 참고하세요. 소스 출처 검사는 공개 사본의 전체 빌드나 runtime 검증을 대신하지 않습니다. Git에는 소스와 도구를, Releases에는 실행 패키지를 게시하며 인증 정보·운영 로그는 포함하지 않습니다.

[OpenAI Codex](https://github.com/openai/codex) 기반의 독립 프로젝트이며 OpenAI 공식 제품이 아닙니다. [Apache-2.0](LICENSE), [NOTICE](NOTICE)와 구성요소별 라이선스를 따릅니다.
