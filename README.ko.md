# Arcvera-Core

[English](README.md) · [한국어](README.ko.md)

OpenAI Codex 소스를 기반으로 독립적으로 유지·관리하는 Windows CLI와 Rust App Server입니다. 공개 명령은 `arcvera`입니다. 현재 OpenAI 서버와 통신하며, 프로토콜 식별자와 내부 helper의 원본 명칭은 구현 출처를 나타냅니다. OpenAI 공식 제품이 아닙니다.

[릴리스 다운로드](https://github.com/jaeseongs95/Arcvera-Core/releases) · [소스 빌드](docs/source-import.md)

## Windows에서 시작하기

Windows x86-64 전체 ZIP과 `SHA256SUMS.txt`를 받고 ZIP 해시를 비교한 뒤 새 폴더에 압축을 풉니다. 1.0.1의 명령은 다음과 같습니다.

```powershell
Get-FileHash .\arcvera-core-1.0.1-windows-x86_64.zip -Algorithm SHA256
```

압축을 푼 폴더에서 실행합니다.

```powershell
.\bin\arcvera.exe
```

| 진입점 | 동작 |
| --- | --- |
| `bin\arcvera.exe` 또는 `arcvera.cmd` | Arcvera 로컬 공유 서버를 사용하는 기본 실행 |
| `arcvera-local.cmd` | `--no-daemon`으로 내장 백엔드 실행 |
| `bin\arcvera.exe --remote <주소>` | 필요한 인증을 사용하여 명시한 원격 서버 연결 |

기본 홈은 `~/.arcvera`입니다. 다른 기존 디렉터리를 선택하려면 `ARCVERA_HOME`을 설정합니다. `CODEX_HOME`은 사용하지 않으며 기존 Codex 데이터를 자동으로 가져오지 않습니다. 새 Arcvera 홈에서 별도로 로그인하십시오. 공식 Codex나 ChatGPT 설치는 필요하지 않습니다.

`codex-package.json`, `package-receipt.json`, `bin` 폴더와 helper 리소스를 함께 유지하십시오. 개별 실행 파일은 완전한 관리 서버 패키지가 아닙니다. 내부 helper 이름은 `codex-app-server.exe`, `codex-code-mode-host.exe`이며 같은 릴리스를 사용해야 합니다. 복구를 위해 이전 패키지를 보관하십시오.

음성 기능에는 Microsoft Visual C++ v14 **X64** 런타임을 별도로 설치해야 합니다. Microsoft DLL과 설치기는 동봉하지 않습니다. [런타임 요구사항](tools/windows-cli/RUNTIME.md)을 확인하십시오.

## 현재 릴리스

Arcvera-Core **1.0.1**은 네이티브 홈 분리·업데이트 정책 핫픽스입니다. `arcvera`, `ARCVERA_HOME`, `~/.arcvera`를 도입하고 상속된 자동 설치기와 업데이트 피드를 비활성화합니다. 새 버전은 이 저장소의 릴리스에서 직접 다운로드하십시오. [영한 릴리스 노트](docs/releases/v1.0.1.md)에 소스 기준 시점과 최종 실행본 검증 기록을 구분합니다.

이전 [1.0.0 릴리스](docs/releases/R41.md)는 R41 대화 기능과 버전 계약 변경을 포함합니다. 과거 시험 결과는 1.0.1의 검증 근거가 아닙니다. 구조 개선은 [R42](docs/roadmap/R42.md)에 유지합니다. 제품 버전은 공식 Codex 0.167.0과 독립적입니다.

## 1.1.0 후보: 문맥과 캐시 제어

1.1.0 후보는 네이티브 검증 중이며 아직 발행된 릴리스가 아닙니다. 인덱스 문맥은 작업 조각과 의존 항목을 모델의 활성 입력에 선택하고 원문은 권한에 따른 조회를 위해 보존합니다. 무관한 상세 내용을 활성 입력에서 제외해도 원문을 삭제하지 않습니다. 요청 전용 탐색 정보는 선택된 이력 뒤에 놓여 탐색 정보 변경이 안정적인 접두부를 바꾸지 않도록 합니다. 인덱스 포커스가 있으면 선택 문맥이 입력 예산을 사용하며 한도를 초과할 때 필수 근거를 요약으로 대체하지 않고 오류를 알립니다.

실험적 `thread/context/read`는 기존 연결 및 실행 인스턴스 접근 검사 아래 로드된 스레드의 본문 없는 구성 메타데이터를 반환합니다. UTF-8 직렬화된 상주 바이트, 로컬 토큰 추정치와 제공자 보고 사용량을 구분합니다. 페이지는 스냅샷에 묶이며 오래된 스냅샷이면 처음부터 다시 조회해야 합니다. 이용할 수 없는 보관 크기, 제공자 지원, 만료 및 개별 조각 TTL은 미확인 상태로 유지합니다. `hooks/list`는 현재 스레드에 설치된 런타임 정보와 설정 검색을 구분합니다. 재로드 요청 확인은 MCP 준비 완료를 뜻하지 않습니다. Hook stdout과 stderr는 각각 1 MiB로 제한하며 초과 시 오류를 반환합니다.

후보에는 마우스로 활성화하는 HTTP(S) 링크를 위한 `--terminal-link-owner=tui|host`가 추가됩니다. 기본값은 `tui`이며 터미널 호스트가 실제로 링크를 열 때에만 `host`를 지정하십시오. 파일 링크와 프로그램이 인증을 위해 여는 URL은 기존 경로를 유지합니다. 버전 문자열만으로 옵션 지원 여부를 판단하지 마십시오. 협력하는 설정 writer는 해석된 대상을 잠그고 현재 읽은 내용으로 버전을 검사합니다. 비협력 외부 프로세스의 경로 교체는 보장 범위에 포함되지 않습니다. 이 변경의 네이티브 및 Desktop 연동 검증은 진행 중입니다.

프롬프트 캐시 정책은 Responses 호환 제공자에 대해 명시적으로 선택합니다. `example`을 설정한 제공자 ID로 바꾸고 해당 제공자가 선택한 전송 정책을 지원할 때에만 다음 예를 사용하십시오.

```toml
[model_providers.example.capabilities.prompt_cache]
kind = "implicit"
ttl = "30m" # 또는 "12h"
```

명시적 레거시 대안은 `kind = "legacy"`와 `retention = "in_memory"` 또는 `"24h"`입니다. 정책을 지정하지 않으면 기존 전송 동작이 유지됩니다. 요청 TTL과 보고된 캐시 활동은 제공자의 지원, 적용 또는 보존을 확인하지 않습니다. 최소 유효 시각, 실제 적용 TTL, 개별 조각 TTL과 서버 확정 만료는 미확인 상태로 유지합니다. TTL은 권한, 필수 선택 또는 원문 보존을 변경하지 않습니다. [1.1.0 후보 안내](docs/releases/v1.1.0.md)를 참고하십시오.

## 대화 제어

다음 명령은 PowerShell이 아닌 CLI 대화 입력란에 넣습니다.

| 명령 | 용도 |
| --- | --- |
| `/context 120000` · `/context reset` | 대화 문맥 크기 지정·초기화 |
| `/context inspect` · `/context inspect next` · `/context cancel` | 1.1.0 후보: 크기가 제한된 메타데이터 페이지 조회·표시 취소 |
| `/memory view` · `/memory enable on` | 메모리 조회·명시적 활성화 |
| `/cron add --every 10m -- Check whether the build has finished` | 현재 CLI 세션의 반복 프롬프트 예약 |
| `/cron list` · `/cron remove 1` | 예약 조회·실제 예약 ID로 삭제 |
| `/subagents` | 하위 세션 조회 |
| `/background` 또는 `F6` | 지원되는 터미널 작업을 백그라운드로 전환 |
| `/reload_plugin` | 플러그인 다시 연결 요청 |
| `/language ko` · `/language en` | 다음 실행에 적용할 전역 UI 언어 저장 |

문맥 설정은 모델 한도를 늘리지 않습니다. 메모리 정정이 요약에 즉시 반영되는 것은 아닙니다. 예약은 대화가 바쁘면 해당 회차를 건너뛰며 CLI 종료 시 사라집니다. 자연어 제어에는 실행 중인 CLI의 도구가 필요합니다. 다시 연결 요청 확인이 모든 MCP 서버의 준비 완료를 뜻하지는 않습니다.

자동업데이트는 비활성화되어 있습니다. `arcvera update`와 상속된 daemon updater 명령은 직접 다운로드 정책을 안내합니다. 명시적 `app-server daemon update --from-cli`는 완전한 로컬 패키지를 Arcvera 전용 관리 서버 디렉터리에 설치할 때 사용합니다.

## Desktop과 공유 서버

[OpenDIY Desktop](https://github.com/jaeseongs95/opendiy-desktop)은 별도 클라이언트입니다. `arcvera`를 실행하고 기본값이 아닌 프로필은 `ARCVERA_HOME`을 명시해야 합니다. 새 CLI 파일을 선택해도 실행 중 서버는 교체되지 않습니다. 초기화 시 서버와 정확히 같은 전체 Core 버전을 선언해야 합니다.

명시적으로 관리 서버를 교체하기 전에 진행 중 작업을 마치십시오. [공유 서버 절차](docs/releases/v1.0.1.md#공유-서버-업데이트)에 따라 실제 서버 버전과 경로를 확인하십시오. ZIP 압축 해제는 프로필을 이전하거나 실행 중 서버를 교체하지 않습니다.

## 검증과 제한

[릴리스 노트](docs/releases/v1.0.1.md)는 소스 시험·최종 실행본·미실행 항목을 구분합니다. 최초의 중첩 SQLite/socket 오류는 새 1.0.0 시험 홈에서 재현되지 않았습니다. 홈 충돌과 상속된 updater 동작은 별도로 확인한 결함이며, 이를 고쳤다는 사실만으로 원 오류의 직접 원인이 입증되지는 않습니다.

실계정·실모델, 물리 IME, 음성, 장시간 부하, 새 PC의 런타임 설치는 별도 검증이 필요합니다. 원본 구현 명칭의 완전한 제거와 멀티벤더 로그인은 이번 핫픽스에 포함하지 않습니다.

[검증](docs/TESTING.md) · [소스 빌드](docs/source-import.md) · [Windows 패키징](tools/windows-cli/README.md) · [변경 고지](MODIFICATIONS.md)

[Core 1.0 workflow](.github/workflows/core-1.0.yml)는 Linux에서 소스 목록·패치 재현·패키징 fixture를 검사합니다. 제품 실행 파일을 빌드하거나 Windows의 실제 실행을 검증하지 않습니다. 제품 빌드와 실행 시험은 로컬에서 수행합니다.

## 라이선스

[OpenAI Codex](https://github.com/openai/codex) 기반의 독립 프로젝트입니다. Git에는 소스와 빌드 도구를, Releases에는 실행 자산을 게시합니다. 원 저작권·출처·제삼자 고지를 보존합니다.

[AGPL-3.0-or-later](LICENSE) · [라이선스](LICENSING.md) · [원 Apache-2.0](LICENSE-APACHE-2.0) · [NOTICE](NOTICE)
