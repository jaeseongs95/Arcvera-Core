# OpenDIY Codex

[English](README.md) · [한국어](README.ko.md)

Windows용 Codex CLI·Rust App Server 비공식 수정판입니다. 요청 취소와 프로세스 정리, 대화별 제어, 예약 실행, 선택형 진단을 보강합니다.

[R32 다운로드](https://github.com/jaeseongs95/opendiy-codex/releases/tag/v0.166.0-r32.20261101) · [소스 빌드](docs/source-import.md) · [검증 기록](docs/TESTING.md) · [변경 내역](MODIFICATIONS.md)

## 배포와 개발 상태

| 구분 | 공식 기반 | 상태 |
| --- | --- | --- |
| **R32 정식 릴리스** | 0.166.0 | 최신 공개판. Windows x86-64 CLI·별도 앱 서버·휴대용 패키지 제공 |
| **R31** | 0.166.0 | 별도 릴리스 취소, 수정 사항은 R32에 통합 |
| **R33 소스 후보** | 0.166.0 | `0.166.0+native-r33.20261102`, 미배포 |

현재 소스는 `3d9eb86445925caa920dd2f958496cf1aa4ad7e2`에 고정되어 있으며 R32 다운로드와 다릅니다. 이전 `0c588` 후보의 개발용 시험 컴파일과 표적 Rust 73개 시험, PowerShell 13개·패키징 13개 시험은 통과했습니다. 현재 후보의 개발용 시험 컴파일과 daemon 시험 66개, nested stdio 경계의 개별 재실행 1개는 통과했으며 native fixture 1개는 무시됐습니다. 개별 재실행은 전체 시험과 겹칩니다. 별도 전달 준비 검사기는 초기 21개, 후속 보강 뒤 23개 fixture를 통과했습니다. 최적화 CLI·앱 서버·launcher 빌드와 package03 생성·해시 결속을 확인했습니다. 자연 EOF 8경계·격리 GUI 복원 범위의 Desktop 공동 시험은 통과했으나 graceful 완료 통지 실패로 전체 수용·릴리스·설치를 보류합니다. 최종 독립감사는 이 차단 결함으로 FAIL입니다. 실행 패키지 공개·실제 예약 등록·모바일·장기 부하는 미검증입니다. [R33 검증 범위](docs/r33-development-verification.ko.md)와 [소스 출처](SOURCE_BASELINE.json)에서 후보와 근거를 구분합니다.

## 주요 기능

- **취소와 정리:** 대기 요청의 취소를 전달하고, 용량 부족 재시도를 제한하며 직접 소유한 하위 프로세스를 정리합니다.
- **대화별 제어:** 컨텍스트 창 크기와 메모리 교정 기록을 관리하고 하위 에이전트 상태를 확인합니다.
- **예약 실행:** CLI가 켜진 동안 요청을 예약합니다. 작업 중인 대화는 해당 회차를 건너뜁니다.
- **화면 이력과 언어:** 표시 이력의 보유 구간을 제한하고 영어·한국어 화면을 선택합니다. 모델의 원본 대화는 유지합니다.
- **선택형 진단:** 이벤트를 버퍼에 모아 완결된 파일로 내보냅니다. R32는 응답·송신 캐시 진단의 배치 전달과 실행 식별을 보강했습니다.

R33 후보에는 이력·버퍼가 겹칠 때의 중복 표시 수정, 비강제 종료 재요청의 drain 보존, 공통 cleanup 시작 경로와 로컬 CLI·서버의 전체 버전 확인이 추가됩니다. Windows 자동업데이트 소스는 검증한 패키지를 다음 CLI 실행에서 선택하도록 작성했으며 기존 패키지와 공유 daemon을 보존합니다. 이 기능은 현재 R32 다운로드에 포함되지 않습니다.

## Windows에서 실행

1. [R32 릴리스](https://github.com/jaeseongs95/opendiy-codex/releases/tag/v0.166.0-r32.20261101)에서 `opendiy-codex-r32-windows-x86_64.zip`과 `SHA256SUMS.txt`를 받습니다.
2. SHA-256을 확인하고 기존 설치를 보존한 채 새 폴더에 압축을 풉니다.
3. 기존 공유 서버와 분리된 로컬 엔진을 사용하려면 `codex-local.cmd`를 실행합니다. 패키지의 `USAGE.ko.md`를 참고하세요.

개별 `codex.exe`·`codex-app-server.exe`도 제공하지만 보조 파일이 필요한 기능에는 전체 패키지가 필요합니다. 기본 CLI와 `codex-patched.cmd`는 기존 공유 서버에 연결할 수 있으므로 실제 서버의 버전은 별도로 확인해야 합니다.

## 명령을 입력하는 곳

다음 명령은 R30에서 제공했고 R32에도 이어지는 대화 입력창 명령입니다. PowerShell 명령이 아닙니다.

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

아래는 **R33 후보 전용 PowerShell 명령**입니다. 대화 입력창의 슬래시 명령이 아니며 R32 다운로드에서는 제공하지 않습니다.

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

[OpenDIY Desktop](https://github.com/jaeseongs95/opendiy-desktop)은 별도 화면·다중 세션 프로젝트입니다. Desktop 1.0.1 후보(`36f54c0d5b8c8ee9ca7fed435c26f8a246bc4985`)와 현재 R33의 공동 시험은 자연 EOF 8경계와 격리 GUI 복원 범위에서 통과했습니다. graceful 완료 통지 실패로 전체 조합 수용은 보류하며 CI 실행 상태를 호환성 보증으로 사용하지 않습니다. 실제 모바일 연결·장시간 부하·원래 지연과 메모리 증가의 해결도 미검증입니다.

## 빌드와 라이선스

[소스 빌드 안내](docs/source-import.md)와 [Windows 패키징 도구](tools/windows-cli/README.md)를 참고하세요. 소스 출처 검사는 공개 사본의 전체 빌드나 runtime 검증을 대신하지 않습니다. Git에는 소스와 도구를, Releases에는 실행 패키지를 게시하며 인증 정보·운영 로그는 포함하지 않습니다.

[OpenAI Codex](https://github.com/openai/codex) 기반의 독립 프로젝트이며 OpenAI 공식 제품이 아닙니다. [Apache-2.0](LICENSE), [NOTICE](NOTICE)와 구성요소별 라이선스를 따릅니다.
