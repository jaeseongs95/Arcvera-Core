# OpenDIY Codex

[English](README.md) · [한국어](README.ko.md)

OpenAI Codex를 기반으로 한 비공식 Windows용 Codex CLI·Rust App Server입니다. 대화 제어 기능을 제공하고 확인된 결함을 수정하며, 인증·권한 검사·저장된 대화·진행 중 작업을 보존합니다.

[최신 버전 다운로드](https://github.com/jaeseongs95/opendiy-codex/releases/latest) · [릴리스 이력](https://github.com/jaeseongs95/opendiy-codex/releases) · [소스 빌드](docs/source-import.md)

## Windows에서 시작하기

1. 릴리스 페이지에서 Windows x86-64 전체 ZIP과 `SHA256SUMS.txt`를 받습니다.
2. ZIP의 SHA-256을 체크섬 파일과 대조한 뒤, 새 폴더에 폴더 구조를 유지해 압축을 풉니다.
3. `bin\codex.exe`를 실행합니다. 소스 컴파일이나 CMD 실행 파일 사용은 필수가 아닙니다.

R40의 경우 첫 명령은 다운로드 폴더에서, 두 번째 명령은 압축을 푼 패키지 폴더에서 실행합니다.

```powershell
Get-FileHash .\opendiy-codex-r40-windows-x86_64.zip -Algorithm SHA256
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

관리형 공유 서버를 교체하려면 [R40 서버 업데이트 절차](https://github.com/jaeseongs95/opendiy-codex/releases/tag/v0.167.0-r40.20261108)를 따릅니다. 전체 ZIP이 필요하며 진행 중 작업을 마친 뒤 별도 PowerShell에서 실행합니다. EXE 경로만 바꿔도 실행 중인 서버가 업데이트되지는 않습니다.

### 개별 실행 파일

릴리스에는 `codex.exe`, `codex-app-server.exe`, `codex-code-mode-host.exe`도 개별 자산으로 제공됩니다. 세 파일은 같은 릴리스의 것을 사용합니다. 실제로 실행하는 EXE와 같은 폴더에 helper를 `codex-code-mode-host.exe`라는 원래 이름으로 둡니다. helper가 없으면 `os error 2`가 발생할 수 있습니다. CLI와 서버를 함께 두면 세 파일 모두 같은 폴더에 둡니다.

이 세 파일은 관리형 서버 설치용 전체 패키지가 아닙니다. 음성 리소스, launcher, updater 지원과 `app-server daemon update --from-cli`에는 전체 ZIP이 필요합니다. 다운로드·버전 확인은 새 PC에서의 최초 설치 시험을 뜻하지 않습니다.

## 현재 릴리스

| 릴리스 | 주요 변경 | 검증 범위 |
| --- | --- | --- |
| [R40](https://github.com/jaeseongs95/opendiy-codex/releases/tag/v0.167.0-r40.20261108) · [노트 (EN/KO)](docs/releases/R40.md) | 임시 MCP 재연결, 비차단 컨텍스트 조회, 후크 경계, 스크롤, 한국어 문구, 확인된 메모리·상속 출처 결함 수정 | 최종 CLI·RPC·ConPTY와 격리 TEST Desktop 검사. 실패·미실행 항목은 노트 참조. |
| [R39](https://github.com/jaeseongs95/opendiy-codex/releases/tag/v0.167.0-r39.20261107) · [노트 (EN/KO)](docs/releases/R39.md) | 미완 작업 중심 압축 | 최종 패키지 격리 검사 15개와 소스 압축 시험. 최종 Desktop 공동시험은 미실행. |

두 버전 모두 공식 Codex 0.167.0을 기반으로 합니다. 공개됐다는 사실이 해당 PC의 설치 버전을 뜻하지는 않습니다. 이전 버전은 [릴리스 이력](https://github.com/jaeseongs95/opendiy-codex/releases)에서 확인할 수 있습니다.

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

Desktop 1.1.0의 OpenDIY 확장 기능은 CLI와 App Server 모두 최소 R36(`0.166.0+native-r36.20261103`)이 필요합니다. 이 최소 버전이 이후 모든 조합의 호환성을 보장하지는 않습니다. R40 통합시험은 시험에 사용한 격리 TEST Desktop 후보에 한정되며 모든 공개 Desktop 설치물을 검증한 결과가 아닙니다. R39 최종 Desktop 공동시험은 실행하지 않았습니다.

연결 성공, initialize 응답, 버전 일치만으로 전체 호환성을 판단할 수 없습니다. 기존 데이터와 진행 중 작업을 보존하며 서버를 자동 교체하지 않습니다.

## 검증과 제한

시험한 실행본, 원 실패와 미실행 항목은 해당 태그의 릴리스 노트에서 확인합니다. 실제 모델의 압축 의미 품질, 물리 키보드·IME·음성 동작, 최초 설치, 장시간 부하에는 노트에 명시한 검증 공백이 있습니다. 이 시험으로 모든 GUI 지연이나 메모리 증가가 해결됐다고 판단할 수는 없습니다.

R40에는 메모리 전면 개편, 새로운 CodeMode 구조, 전체 리팩토링, 구조적 캐싱 개선이 포함되지 않습니다. 화면 이력 제한은 전체 프로세스 메모리 제한이 아니며 파일 생성은 클라우드 업로드 성공의 증거가 아닙니다.

[검증 안내](docs/TESTING.md) · [릴리스 노트 포맷](docs/releases/FORMAT.md) · [소스 빌드](docs/source-import.md) · [Windows 패키징](tools/windows-cli/README.md) · [변경 이력](MODIFICATIONS.md)

## 라이선스

[OpenAI Codex](https://github.com/openai/codex)를 기반으로 한 독립 프로젝트이며 OpenAI 공식 제품이 아닙니다. Git에는 소스와 빌드 도구를, Releases에는 실행 자산을 보관합니다. 인증정보와 운영 로그는 포함하지 않습니다.

[Apache-2.0](LICENSE) · [NOTICE](NOTICE)
