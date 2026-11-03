<!-- Modified for OpenDIY Codex R36 (2026-11-03); see MODIFICATIONS.md. -->
# R36 Cloud·GitHub CI 관측

## Cloud 조사 결과와 경계

R36 기준 소스의 `cloud-tasks/src/cli.rs`, `lib.rs`, `env_detect.rs`,
`env_detect_tests.rs`와 `cloud-tasks-client/src/http.rs`를 확인했다.
`cloud exec/status/list/environments/diff/apply` 진입점과 공식 backend 호출이 존재한다.
`--env`는 전체 접근 가능 목록에서 ID를 우선 확인하고, 이름은 대소문자를 무시한
유일한 일치만 허용한다. 전체 목록의 인증·권한·한도·서버 실패를 빈 목록이나
부분적인 이름 일치로 바꾸지 않는 이전 수정과 회귀시험도 포함돼 있다.

이 범위에서 새로 입증한 Cloud 호출 결함은 없다. 따라서 backend·인증·환경 선택
코드는 추가로 변경하지 않았다. `codex cloud environments --json`으로 환경 ID를
확인한 뒤 `codex cloud exec --env <ID> --branch <BRANCH> <QUERY>`를 호출하는 기존
경로를 사용한다. CLI의 Cloud 호출에는 Cloud Environment 플러그인이 필수는 아니다.

실제 ChatGPT 계정·Cloud 환경 조회나 작업 제출은 이번 담당 범위에서 실행하지 않았다.
기존 이름 해석/오류 보존 시험과 소스 확인은 실제 계정의 호출 성공 증거가 아니다.
특히 접근 권한, 환경 공유, checkout 브랜치, 네트워크와 backend 운영 상태는 남은
실환경 확인 사항이다. 근거 가이드는 `G:/내 드라이브/문서/개발가이드/도구 사용법/Codex Cloud CLI 사용 가이드.md`다.

## 추가한 CI 명령

기존 GitHub CLI의 읽기 전용 `gh run list/view --json`을 사용한다. 새 SDK·서비스·인증
저장소나 daemon은 추가하지 않는다. 로컬 `gh` 설치 및 기존 읽기 권한이 필요하다.

```text
codex ci status
codex ci status --repo openai/codex --commit <FULL_SHA> --json
codex ci list --repo openai/codex --branch main --limit 20
codex ci watch <RUN_ID> --repo openai/codex --timeout 300 --interval 10 --json
```

`status`와 `list`는 한 번만 조회한다. `status`는 기본적으로 로컬 HEAD의 전체 SHA를
필터로 전달하고, 반환된 각 실행의 SHA를 다시 대조한다. 명시적인 `--branch`는
commit 필터에 추가된다. 원격 저장소를 지정해도 기본 HEAD는 현재 로컬 저장소의 HEAD다.
`list`는 명시한 필터가 없으면 최근 실행을 조회한다. 결과에는 run ID·시도 번호·워크플로·
HEAD SHA·브랜치·상태·결론·URL이 포함된다. 사용자/서버 식별자는 번역하지 않으며
앱 문구는 전역 en/ko UI 설정을 따른다.

`watch`만 명시적으로 반복 조회하며 특정 run ID만 관측한다. 전체 제한 시간은
1~86400초, 간격은 1~300초다. 각 `gh`/`git` 호출은 최대 30초로 제한된다.
전체 watch deadline은 조회와 대기를 함께 제한하며 벽시계 보정의 영향을 받지 않는다.
하위 프로세스는 Windows 콘솔을 만들지 않고 입력 요청을 막는다. 취소·timeout 시
자신이 시작한 프로세스를 종료한다. 원격 run은 취소하거나 재실행하지 않는다.

조회 성공은 CI 통과 판정이 아니다. 빈 결과도 통과를 뜻하지 않는다. 최대 100개의
최근 workflow run만 조회하며, 저장소 보호 규칙의 모든 필수 검사나 merge 승인을
대신하지 않는다. `status/list` 종료 0은 조회 성공을 뜻하고 실제 결론은 결과 필드로
확인한다. `watch`는 관측 대상이 `completed/success`로 끝나야 종료 0이며, 실패·취소·
중립·skip·결론 누락·시간초과는 비정상 종료한다. `--json` watch는 관측별 JSON 한 줄이다.

인증, 권한/API 제한, 404, 요청 한도, 실행 파일 부재, JSON 오류, SHA/run ID 불일치를
성공한 빈 결과로 바꾸지 않는다. 비공개 대상의 404는 부재로 단정하지 않는다.
오류의 원시 backend 본문·토큰·로그는 출력하지 않는다. 재실행·코멘트·workflow 등록·
인증 변경 명령은 제공하지 않는다.

## 설계 조건과 시험 계획

CS `OS-LIFETIME-001`과 `OS-CLOCK-003`을 적용했다. draft인 운영체제 카드를 선택한
이유는 새 기능이 하위 프로세스를 소유하고 조회·관측 timeout을 구현하기 때문이다.
조건은 명시 요청 때만 polling, monotonic deadline, 종료 시 자신의 자식 회수,
조회 실패 보존, 요청 commit/run ID와 결과의 일치다.

기존 Rust unit test 체계를 사용하며 별도 runner나 의존성을 추가하지 않는다.
`cli/src/ci.rs`의 시험은 다음 공개 CLI 경계와 오류 경계를 다룬다.

| 요구 | 사례·oracle | 환경·제외 경계 |
| --- | --- | --- |
| 명시적인 제한 관측 | status/list에 watch 없음, 0 timeout/interval·0 run·limit 초과·rerun 거절 | Clap 파싱, 외부 API 제외 |
| 정확한 후보 | 읽기 전용 argv와 전체 SHA 전달, 응답 SHA 불일치 거절 | JSON/argv 경계, 실제 GitHub 권한 제외 |
| 오류 보존 | invalid JSON·원시 비밀 비출력·인증/권한/404/한도 분리·missing executable | parser/process 경계 |
| 자식 수명 | 현재 시험 실행 파일을 자식으로 시작, 시작 marker 확인 후 timeout, 지연 marker 미생성 | 실제 OS 프로세스, 원격 run 제외 |
| run 상태 구분 | 진행/실패 완료/다른 run ID 구분 | 상태 해석 경계 |
| UI 언어 | 영어/한국어 메시지와 동일 동적 식별자 보존 | 실제 Fluent catalog |

담당자는 Cargo와 빌드를 실행하지 않았으며 위 Rust 시험은 통합 담당자의 일괄 실행
대상이다. 미실행 시험을 통과로 계산하지 않는다. timeout 회귀시험은 process fixture의
시작 marker를 요구하므로 자식 미기동이 종료 성공으로 오인되지 않는다.

## 담당자가 실제 실행한 확인

2026-11-03 이 Windows 환경의 `gh --version`은 `2.101.0`이었다.
`gh run list --help`, `gh run view --help`에서 사용 JSON 필드와 selector를 확인했다.
다음 읽기 전용 공개 저장소 조회는 각각 종료 코드 0이었다.

```text
gh run list --repo openai/codex --limit 1 --json databaseId,headSha,headBranch,status,conclusion,url,workflowName,attempt
gh run view 41625835032 --repo openai/codex --json databaseId,headSha,headBranch,status,conclusion,url,workflowName,attempt
```

조회한 run은 `41625835032`, HEAD는 `cb2b5857c249c88075a973faad53b6693248a6cc`,
브랜치는 `main`, 시도는 `1`, 워크플로는 `CLA Assistant`, 상태/결론은
`completed/success`, URL은 `https://github.com/openai/codex/actions/runs/41625835032`였다.
이는 기존 gh와 공개 API 계약 확인이며 새 R36 실행 파일의 실행 증거는 아니다.
소유한 Rust 파일은 지정 1.99.1 `rustfmt --edition 2024`로 포맷했다(종료 0).
stable formatter의 `imports_granularity` 경고는 발생했다.
