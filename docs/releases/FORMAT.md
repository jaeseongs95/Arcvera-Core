# Release note format / 릴리스 노트 작성 포맷

## English

Write English and Korean together in **one release note**. Keep both sections current; language links must not replace either section. Store the source as `docs/releases/R<number>.md` and use the same complete text for the GitHub release body.

For current releases, use the title `Arcvera-Core <SemVer> — <English summary> / <한국어 요약>`. R41/R42 identify work scope rather than release versions; preserve historical release titles and filenames. Place `## English` first and `## 한국어` second. Each language section follows the existing seven-part structure below, using level-three headings.

### Required structure

| Order | English heading | Korean heading | Required content |
| --- | --- | --- | --- |
| 1 | Overview | 개요 | Summary; version, platform, upstream base, release status |
| 2 | Changes | 변경 사항 | Behavior changed in this release and its user impact |
| 3 | Getting started | 시작하기 | Complete ZIP, SHA verification, direct EXE startup, default/shared and isolated CMD launchers, helper placement |
| 4 | Shared server update | 공유 서버 업데이트 | Complete-package requirement, interruption impact, explicit update/start/status commands, expected version and error recovery |
| 5 | Validation | 검증 | Targets, environments, results; distinguish final executables from historical source tests |
| 6 | Known limitations | 알려진 제한 | Original failures, exclusions, NOT_RUN cases, user impact and excluded changes |
| 7 | Source and integrity | 소스와 무결성 | Product/build sources, significant build differences, asset names and SHA-256 |

### Writing and maintenance

- Use formal, public-facing language. Omit private remarks, internal ledger identifiers, local work paths, test-round labels, and approval history. Retain technical information needed to assess or reproduce results.
- Keep versions, commands, counts, source references, checksums, failures, and unrun cases equivalent across languages. Put matching checksum tables in both sections.
- Distinguish startup, shared-server connection, and server-version replacement. Present direct EXE startup as the default. Three individual executables are not a complete installation package.
- Preserve authentication, permission checks, and existing data. Describe interruption risks before server-update commands.
- Do not convert failures or unrun cases into passes. Identify mocks, synthetic providers, isolated Desktop candidates, historical source tests, and final binaries by their actual scope.
- A successful connection or matching version alone does not prove compatibility or performance improvement.
- Documentation-only revisions are not new builds. Keep existing assets, tags, and checksums unchanged.
- Maintain the English main README and linked Korean README together, updating release information with each release. Each README links to the same bilingual release note.
- Apply the structure to current releases. Preserve R39/R40 and earlier published notes, assets and checksums; their historical naming is unchanged.

### Main README structure

`README.md` is the English entry point and links to `README.ko.md`. Maintain equivalent facts and links in both files, in this order:

| Order | English heading | Korean heading | Content |
| --- | --- | --- | --- |
| Introduction | Project title and introduction | 프로젝트 제목·소개 | Purpose, upstream provenance, support boundary, language and download links |
| 1 | Start on Windows | Windows에서 시작하기 | Complete package, checksum command, direct startup, connection modes, helper placement |
| 2 | Current releases | 현재 릴리스 | Current SemVer, bilingual notes, meaningful changes and actual validation boundary; distinguish historical releases |
| 3 | Conversation controls | 대화 제어 | Supported commands and their effects and limits |
| 4 | Desktop and shared servers | Desktop과 공유 서버 | Related client, server identity, update procedure and tested integration scope |
| 5 | Validation and limits | 검증과 제한 | Evidence scope, untested behavior, source/build/package documentation |
| 6 | License | 라이선스 | Derivative attribution, distribution boundary, license and notices |

Update startup examples and current-release links when publishing. Keep the full bilingual release note as the source for detailed test results and asset checksums.

## 한국어

**한 릴리스 노트 안에 영문과 한글을 함께 작성합니다.** 두 언어를 함께 최신화하며, 언어 링크로 본문을 대신하지 않습니다. 원본은 `docs/releases/R번호.md`에 보관하고 GitHub 릴리스 본문에도 같은 전체 내용을 사용합니다.

현재 릴리스 제목은 `Arcvera-Core <SemVer> — <English summary> / <한국어 요약>`으로 작성합니다. R41/R42는 릴리스 버전이 아닌 작업 범위를 나타내며, 과거 릴리스의 제목과 파일명은 보존합니다. `## English`를 먼저, `## 한국어`를 뒤에 배치합니다. 각 언어는 기존의 아래 7절 구성을 유지하고 3단계 제목을 사용합니다.

### 고정 구성

| 순서 | 영문 제목 | 한글 제목 | 필수 내용 |
| --- | --- | --- | --- |
| 1 | Overview | 개요 | 요약; 실행본 버전·운영체제·공식 기반·공개 상태 |
| 2 | Changes | 변경 사항 | 해당 릴리스에서 바뀐 동작과 사용자 영향 |
| 3 | Getting started | 시작하기 | 전체 ZIP·SHA 확인, EXE 직접 실행, 공유 기본·분리 CMD, helper 배치 |
| 4 | Shared server update | 공유 서버 업데이트 | 전체 패키지 조건, 작업 중단 영향, 명시적 update/start/status, 기대 버전·오류 대응 |
| 5 | Validation | 검증 | 대상·환경·결과. 최종 실행본과 과거 소스 시험 구분 |
| 6 | Known limitations | 알려진 제한 | 원 실패·제외·NOT_RUN, 사용자 영향과 미포함 변경 |
| 7 | Source and integrity | 소스와 무결성 | 제품·빌드 소스, 중요한 빌드 차이, 자산 이름과 SHA-256 |

### 작성·유지 규칙

- 공개 문서에 맞는 공식적인 표현을 사용합니다. 사적인 표현, 내부 장부 번호, 로컬 작업 경로, 시험 회차 이름, 승인 경위는 제외합니다. 결과 판단·재현에 필요한 기술 정보는 유지합니다.
- 버전·명령·수치·소스·체크섬·실패·미실행 범위를 두 언어에서 동일하게 유지합니다. 양쪽 절에 같은 체크섬 표를 둡니다.
- 실행·공유 서버 연결·서버 버전 교체를 구분합니다. EXE 직접 실행을 기본으로 안내합니다. 개별 실행 파일 세 개는 전체 설치 패키지가 아닙니다.
- 인증·권한 검사·기존 데이터를 보존합니다. 서버 업데이트 명령 전에 작업 중단 영향을 설명합니다.
- 실패·미실행을 통과로 바꾸지 않습니다. mock·합성 provider·격리 Desktop 후보·과거 소스 시험·최종 실행본은 실제 검증 범위로 구분합니다.
- 연결 성공이나 버전 일치만으로 호환성·성능 개선을 주장하지 않습니다.
- 문서 개정만으로 새 빌드가 되지는 않습니다. 기존 자산·태그·체크섬을 유지합니다.
- 영문 메인 README와 연결된 한글 README를 함께 관리하고 릴리스마다 정보를 최신화합니다. 두 README는 같은 영한 병기 릴리스 노트에 연결합니다.
- 현재 릴리스에 이 구성을 적용합니다. R39·R40과 그 이전 공개 노트·자산·체크섬 및 과거 명칭은 보존합니다.

### 메인 README 구성

`README.md`를 영문 진입점으로 두고 `README.ko.md`를 연결합니다. 두 파일의 사실과 링크를 동등하게 유지하며 다음 순서를 따릅니다.

| 순서 | 영문 제목 | 한글 제목 | 내용 |
| --- | --- | --- | --- |
| 소개 | Project title and introduction | 프로젝트 제목·소개 | 목적·공식 소스 출처·지원 범위·언어 및 다운로드 링크 |
| 1 | Start on Windows | Windows에서 시작하기 | 전체 패키지·체크섬 명령·직접 실행·연결 방식·helper 배치 |
| 2 | Current releases | 현재 릴리스 | 현재 SemVer·영한 노트·주요 변경·실제 검증 범위. 과거 릴리스와 구분 |
| 3 | Conversation controls | 대화 제어 | 지원 명령·효과·한계 |
| 4 | Desktop and shared servers | Desktop과 공유 서버 | 관련 클라이언트·서버 식별·업데이트 절차·통합시험 범위 |
| 5 | Validation and limits | 검증과 제한 | 근거 범위·미검증 동작·소스 및 빌드·패키지 문서 |
| 6 | License | 라이선스 | 파생 출처·배포 범위·라이선스·고지 |

발행 시 시작 예제와 현재 릴리스 링크를 최신화합니다. 상세 시험 결과와 자산 체크섬은 영한 병기 릴리스 노트를 정본으로 삼습니다.

[R39 — English / 한국어](R39.md) · [R40 — English / 한국어](R40.md)
