# Codex Cloud CLI

R35 후보는 공식 Cloud 작업 API와 기존 R28 Cloud CLI 보완을 사용합니다. 계정의 실제 접근 권한이나 Cloud 환경을 새로 만들지 않습니다.

```powershell
codex cloud environments --json
codex cloud exec --env 실제_환경_ID --branch 원격_브랜치 "작업 요청"
codex cloud list --env 실제_환경_ID --json
```

`environments`는 접근 가능한 전체 환경의 ID·표시명·고정 여부만 반환합니다. 작업 목록의 `environment_id`가 비어 있더라도 환경 목록에서 ID를 확인할 수 있습니다. 환경 이름이 중복되면 `exec --env`와 `list --env`에는 ID를 지정합니다.

R28에는 이미 CLI 설정·인증 관리자 공유, API key 로그인과 ChatGPT 로그인 오류 구분, `cloud environments`가 포함돼 있습니다. R35에서 새로 확인한 결함은 `exec --env`와 `list --env`가 화면용 부분 조회 결과로 환경을 판정하던 경로입니다. 저장소별 조회가 성공하고 전체 조회가 실패하면 존재하는 환경을 찾지 못했다고 알리거나, 전체에서 중복인 이름을 유일한 환경으로 판단할 수 있었습니다.

R35 후보는 명시한 환경을 완전한 전체 목록으로 확인합니다. HTTP 401·403·429·5xx와 응답 해석 실패는 조회 오류로 반환하며, 성공한 빈 목록과 구분합니다. 이 단계가 실패하면 작업 제출 API를 호출하지 않습니다. 실제 계정의 권한·환경 부재·서비스 장애를 소스 확인만으로 판정하지 않습니다.

계정 확인에는 `codex login status`를 사용합니다. Cloud에는 ChatGPT 로그인이 필요하며 API key만으로 Cloud 권한이 생기지는 않습니다. 인증 파일이나 토큰을 복사하지 않습니다. 환경이 안 보인다면 실행 버전, 실제 명령과 오류 코드, 사용한 계정·워크스페이스를 확인합니다.

## 확인 범위

2026-11-03 기준 소스에서 기존 보완과 잔여 오류 경로를 확인했습니다. 별도의 mock 회귀시험을 추가했으며 실제 제품 시험 실행 결과는 R35 후보 검증 기록에 따릅니다. 사용자 계정의 Cloud 작업·대화 조회, 작업 제출·적용, 설치·배포는 이 조사에서 실행하지 않았습니다. 과거 사용자 호출 실패와 이번 오류 경로가 같은 원인이었다는 점은 미확인입니다.

참조한 공식 소스는 [Cloud CLI 명령](https://github.com/openai/codex/blob/main/codex-rs/cloud-tasks/src/cli.rs)과 [환경 확인 후 작업 제출 경로](https://github.com/openai/codex/blob/main/codex-rs/cloud-tasks/src/lib.rs)입니다. 확인한 공식 main에는 별도의 `cloud environments` 명령이 없으며 이 명령은 기존 로컬 R28 보완입니다.
