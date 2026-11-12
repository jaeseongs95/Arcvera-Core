# Arcvera-Core

[English](README.md) · [한국어](README.ko.md)

OpenAI Codex에서 파생되어 독립적으로 개발하는 Windows용 CLI·Rust App Server입니다. 대화 제어 기능을 제공하고 확인된 결함을 수정하며, 인증·권한 검사·저장된 대화·진행 중 작업을 보존합니다.

Arcvera-Core는 OpenAI 서버와의 통신을 지원합니다. 공식 Codex CLI와의 호환성이나 상호 교체는 지원 범위에 포함하지 않습니다. upstream 버전은 소스 출처를 나타내며, 명령·프로토콜·저장 형식·릴리스 계약은 Arcvera-Core가 독립적으로 관리합니다.

[최신 버전 다운로드](https://github.com/jaeseongs95/Arcvera-Core/releases/latest) · [릴리스 이력](https://github.com/jaeseongs95/Arcvera-Core/releases) · [소스 빌드](docs/source-import.md)

## Windows에서 시작하기

네이티브 음성에 필요한 Microsoft Visual C++ v14 **X64** 런타임은 사용자가
별도로 설치해야 합니다. Microsoft DLL과 설치기는 동봉하지 않습니다.
공식 다운로드·지원 버전·DLL 누락 복구는
[런타임 설치 안내](tools/windows-cli/RUNTIME.md)를 확인하십시오.

1. 릴리스 페이지에서 Windows x86-64 전체 ZIP과 `SHA256SUMS.txt`를 받습니다.
2. ZIP의 SHA-256을 체크섬 파일과 대조한 뒤, 새 폴더에 폴더 구조를 유지해 압축을 풉니다.
3. `bin\codex.exe`를 실행합니다. 소스 컴파일이나 CMD 실행 파일 사용은 필수가 아닙니다.

발행된 1.0.0 패키지의 첫 명령은 다운로드 폴더에서, 두 번째 명령은 압축을 푼 패키지 폴더에서 실행합니다.

```powershell
Get-FileHash .\arcvera-core-1.0.0-windows-x86_64.zip -Algorithm SHA256
```

```powershell
.\bin\codex.exe
```

`codex-package.json`, `package-receipt.json`, `bin` 폴더를 함께 유지합니다. 복구할 수 있도록 이전 패키지도 보관합니다.

### 연결 방식 선택

| 실행 경로 | 동작 |
| --- | --- |
| `bin\codex.exe` 또는 `codex-patched.cmd` | 기본 실행입니다. 설치 상태와 설정에 따라 호환되는 로컬 공유 서버를 재사용할 수 있습니다. |
| `codex-local.cmd` | `--no-daemon`으로 실행해 별도의 내장 백엔드를 사용합니다. |
| `bin\codex.exe --remote <주소>` | 지정한 원격 서버에 연결합니다. 필요한 인증을 설정해야 합니다. |

ZIP에는 `codex-patched.cmd`가 이미 포함돼 있습니다. 인자를 `bin\codex.exe`에 전달하므로 공유 서버를 재사용할 수 있는 기본 실행을 CMD로도 시작할 수 있습니다. 서버 버전 교체, 원격 주소 설정, 모바일 페어링은 수행하지 않습니다. 자동 선택한 서버가 호환되지 않거나 버전을 확인할 수 없으면 내장 백엔드로 실행될 수 있습니다. 선택한 CLI 파일과 실제 서버의 식별 정보를 각각 확인해야 합니다.

관리형 공유 서버를 교체하려면 [1.0.0 서버 업데이트 절차와 호환성 경계](docs/releases/R41.md#공유-서버-업데이트)를 따릅니다. 전체 ZIP이 필요하며 진행 중 작업을 마친 뒤 별도 PowerShell에서 실행합니다. EXE 경로만 바꿔도 실행 중인 서버가 업데이트되지는 않습니다.

### 개별 실행 파일

릴리스에는 `codex.exe`, `codex-app-server.exe`, `codex-code-mode-host.exe`도 개별 자산으로 제공됩니다. 세 파일은 같은 릴리스의 것을 사용합니다. 실제로 실행하는 EXE와 같은 폴더에 helper를 `codex-code-mode-host.exe`라는 원래 이름으로 둡니다. helper가 없으면 `os error 2`가 발생할 수 있습니다. CLI와 서버를 함께 두면 세 파일 모두 같은 폴더에 둡니다.

이 세 파일은 관리형 서버 설치용 전체 패키지가 아닙니다. 음성 리소스, launcher, updater 지원과 `app-server daemon update --from-cli`에는 전체 ZIP이 필요합니다. 다운로드·버전 확인은 새 PC에서의 최초 설치 시험을 뜻하지 않습니다.

## 현재 릴리스

Arcvera-Core 1.0.0(R41 범위)의 [영한 릴리스 노트·SemVer 정책](docs/releases/R41.md), [인덱스 컨텍스트 설계·복구 안내](docs/context-index.ko.md)를 확인하십시오. 선택한 작업과 의존관계를 따라 도달하는 작업을 모델 API 문맥의 기준으로 삼고 사용자·제어·미분류 입력을 보존합니다. 캐시는 응답 단위 수치로 관측하며, 제공되지 않은 TTL·만료 시각은 알 수 없는 상태로 유지합니다. 클라이언트는 초기화할 때 서버의 정확한 Core 전체 버전을 선언해야 하며 이전 클라이언트는 지원하지 않습니다. 사용하지 않는 워크스페이스 패키지 7개를 제거했습니다. OpenDIY가 추가한 배치 로그·캐시 응답/wire 진단 기록·Drive 내보내기와 해당 세션 제어를 제거했습니다. 공식 Codex 로그·텔레메트리·대화 저장은 유지합니다. 이전 로그 설정은 읽지 않으며, 기존 설정·로그 파일은 자동 삭제하거나 이전하지 않습니다. 기존 R41의 광범위한 아키텍처 작업은 [R42](docs/roadmap/R42.md)로 이관합니다. 릴리스 버전은 공식 기반 Codex 0.167.0과 독립적이며 R41·R42는 범위 추적명입니다. 시험 모듈 이동 전 소스의 컨텍스트 회귀시험은 로컬 20개 묶음, 중복 제외 89개 시험이 통과했으며 문맥 구성 표본 180개를 확보했습니다. 시험 모듈 이동 전 소스의 버전 계약 시험 3건, 공식 diagnostics 시험 2건, API 시험 194건, SQLite 필터 시험 1건, TUI 세션 제어 시험 15건이 통과했습니다. 최종 로컬 빌드와 553개 파일의 전체 패키지는 [날짜별 검증 기록](docs/releases/R41.md#final-verification-record--2026-11-13-kst)에서 식별합니다. 실행 파일 세 개의 버전·도움말, 기존 설정 시작 6사례, 실제 stdio·버전 재시도와 명시한 격리 Desktop 수명 제어 결과를 기록했습니다. 감사·발행 결과는 별도로 기록하며 소스 시험 결과만으로 대신하지 않습니다. 실제 앱·모델·네트워크 성능은 미측정입니다. 빌드와 시험은 로컬에서 수행하며 GitHub/Actions 빌드는 제외합니다.

위 회귀시험과 측정은 `#[cfg(test)]` 모듈 두 개를 별도 파일로 옮기기 전에 실행했습니다. 전체 빌드 입력을 대조해 두 모듈의 배치와 필수 파일 고지만 변경됐고, 원래 시험 본문과 제품 코드의 바이트가 정확히 복원됨을 확인했습니다. 새 배치의 버전 계약 세 사례는 각각 RED/GREEN으로 결함 탐지 능력을 확인했고 daemon 모듈 네 시험도 통과했습니다. AppServer 시험 쌍은 기존 initialize 모듈을 변경 없이 포함한 r41_initialize 대상을 사용했으며, 원래 all 대상의 GREEN은 실행하지 않았습니다. Protocol과 daemon 결과는 실제 실행 당시의 소스 식별 정보를 유지하며, 추가된 대상과의 관계는 전체 입력의 정확한 비교로 확인했습니다. 최종 실행본·Desktop·발행의 별도 검증 근거는 릴리스 검증 부록에 기록합니다. 과거 시험을 새 파일 배치에서 실행한 결과로 표시하지 않습니다.

| 릴리스 | 주요 변경 | 검증 범위 |
| --- | --- | --- |
| [R40](https://github.com/jaeseongs95/Arcvera-Core/releases/tag/v0.167.0-r40.20261108) · [노트 (EN/KO)](docs/releases/R40.md) | 임시 MCP 재연결, 비차단 컨텍스트 조회, 후크 경계, 스크롤, 한국어 문구, 확인된 메모리·상속 출처 결함 수정 | 최종 CLI·RPC·ConPTY와 격리 TEST Desktop 검사. 실패·미실행 항목은 노트 참조. |
| [R39](https://github.com/jaeseongs95/Arcvera-Core/releases/tag/v0.167.0-r39.20261107) · [노트 (EN/KO)](docs/releases/R39.md) | 미완 작업 중심 압축 | 최종 패키지 격리 검사 15개와 소스 압축 시험. 최종 Desktop 공동시험은 미실행. |

두 버전 모두 공식 Codex 0.167.0을 기반으로 합니다. 공개됐다는 사실이 해당 PC의 설치 버전을 뜻하지는 않습니다. 이전 버전은 [릴리스 이력](https://github.com/jaeseongs95/Arcvera-Core/releases)에서 확인할 수 있습니다.

## 대화 제어

다음 명령은 PowerShell이 아닌 CLI 대화 입력란에 넣습니다.

| 명령 | 용도 |
| --- | --- |
| `/context 120000` · `/context reset` | 대화 문맥 크기 지정·초기화 |
| `/memory view` · `/memory enable on` | 메모리 조회·명시적 활성화 |
| `/cron add --every 10m -- Check whether the build has finished` | 현재 CLI 세션의 반복 프롬프트 예약 |
| `/cron list` · `/cron remove 1` | 예약 조회·실제 예약 ID로 삭제 |
| `/subagents` | 하위 세션 조회 |
| `/background` 또는 `F6` | 지원되는 터미널 작업을 백그라운드로 전환 |
| `/reload_plugin` | 플러그인 다시 연결 요청 |
| `/language ko` · `/language en` | 다음 실행에 적용할 전역 UI 언어 저장 |

문맥 설정은 모델 한도를 늘리지 않습니다. 메모리 정정이 요약에 즉시 반영되는 것은 아닙니다. 예약은 대화가 바쁘면 해당 회차를 건너뛰며 CLI 종료 시 사라집니다. 자연어 제어에는 실행 중인 CLI의 도구가 필요합니다. 다시 연결 요청 확인이 모든 MCP 서버의 준비 완료를 뜻하지는 않습니다.

업데이트 명령은 PowerShell에서 실행합니다. `codex update --check`, `codex update --status`, `codex update`는 지원되는 설치 패키지가 필요합니다. [업데이트 안내](tools/windows-cli/UPDATE.ko.md)를 확인하십시오.

## Desktop과 공유 서버

[OpenDIY Desktop](https://github.com/jaeseongs95/opendiy-desktop)은 데스크톱 UI와 다중 세션을 담당하는 별도 프로젝트입니다. 이 저장소는 CLI·App Server·Windows 패키지를 제공합니다. Desktop에서 새 CLI를 선택해도 이미 실행 중인 공유 서버는 교체되지 않습니다.

Core 1.0.0에는 초기화 시 Core 전체 버전을 정확히 선언하는 Desktop 클라이언트가 필요합니다. Desktop UI 버전은 별도로 유지합니다. [클라이언트 프로토콜과 이전 안내](codex-rs/app-server/README.md#arcvera-core-100-initialization--초기화)를 확인하십시오. 실제 시험한 Core·Desktop 식별 정보와 연동 결과는 릴리스 검증 부록에서 확인하십시오.

이전 Desktop 1.1.0의 확장 기능에는 CLI와 App Server 모두 최소 R36(`0.166.0+native-r36.20261103`)이 필요했습니다. 이 과거 기준이 Core 1.0.0과의 호환성을 뜻하지는 않습니다. R40 통합시험은 시험에 사용한 격리 TEST Desktop 후보에 한정되며 모든 공개 Desktop 설치물을 검증한 결과가 아닙니다. R39 최종 Desktop 공동시험은 실행하지 않았습니다.

연결 성공, initialize 응답, 버전 일치만으로 전체 호환성을 판단할 수 없습니다. 기존 데이터와 진행 중 작업을 보존하며 서버를 자동 교체하지 않습니다.

## 검증과 제한

격리한 1.0.0 소스의 버전 계약 시험 3건이 각각 제품 코드 변이 시 실패하고 복원 후 통과했습니다. 정확한 Core 버전 선언, 세션 진입 전 거절과 올바른 선언으로 재시도, 서버 거절의 신속한 보고를 검사했습니다. 같은 시험 3건이 시험 모듈 이동 전 소스에서도 통과했으며, 이는 성공 재실행 결과로 새 변이 시험 근거가 아닙니다. 최종 릴리스 실행본·Desktop 결과는 릴리스 검증 부록에 별도로 기록합니다. [검증 범위](docs/releases/R41.md#검증)를 참고하십시오.

시험한 실행본, 원 실패와 미실행 항목은 해당 태그의 릴리스 노트에서 확인합니다. 실제 모델의 압축 의미 품질, 물리 키보드·IME·음성 동작, 최초 설치, 장시간 부하에는 노트에 명시한 검증 공백이 있습니다. 이 시험으로 모든 GUI 지연이나 메모리 증가가 해결됐다고 판단할 수는 없습니다.

R40에는 메모리 전면 개편, 새로운 CodeMode 구조, 전체 리팩토링, 구조적 캐싱 개선이 포함되지 않습니다. 화면 이력 제한은 전체 프로세스 메모리 제한이 아니며 파일 생성은 클라우드 업로드 성공의 증거가 아닙니다.

[검증 안내](docs/TESTING.md) · [릴리스 노트 포맷](docs/releases/FORMAT.md) · [소스 빌드](docs/source-import.md) · [Windows 패키징](tools/windows-cli/README.md) · [변경 이력](MODIFICATIONS.md)

## 라이선스

[OpenAI Codex](https://github.com/openai/codex)를 기반으로 한 독립 프로젝트이며 OpenAI 공식 제품이 아닙니다. Git에는 소스와 빌드 도구를, Releases에는 실행 자산을 보관합니다. 인증정보와 운영 로그는 포함하지 않습니다.

[AGPL-3.0-or-later](LICENSE) · [Licensing / 라이선스](LICENSING.md) · [Original Apache-2.0](LICENSE-APACHE-2.0) · [NOTICE](NOTICE)
