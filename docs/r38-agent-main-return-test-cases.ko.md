<!-- Modified for OpenDIY Codex R38 (2026-11-06); see MODIFICATIONS.md. -->
# R38 메인 에이전트 마우스 복귀와 hover 완화 시험 사례

작성 시점: 구현 전. runner는 기존 codex-tui Rust unit/integration fixture이며 새 runner를 추가하지 않는다. 실행은 조정 담당이 격리 시험 사본에서 수행한다. 아래 실제 결과는 모두 NOT_RUN이다.

| 사례 | 입력·전제 | 절차 | 기대 결과·합격 기준 | 실제 결과 |
| --- | --- | --- | --- | --- |
| MAIN-01 | 정상 root와 자식 1개, root/child 각 초안 | 실제 footer 좌표를 클릭해 child로 전환한 뒤 메인행 클릭; owned/inline 경로 반복 | 발행된 root/thread ID와 active_thread_id가 정확히 왕복; 각 초안 복원; submit/interrupt 미발행 | NOT_RUN |
| MAIN-02 | child 화면, 표시되는 자식 0개(숨김/완료 만료 포함) | 접힌 footer의 메인행 클릭 | 정확한 root로 복귀; 메인이 자식 개수에 포함되지 않음 | NOT_RUN |
| MAIN-03 | 자식 15/100개, 긴 목록 | 펼치기·스크롤·페이지 이동 후 메인행 클릭 | 메인행 고정; 상세행 기존 최대 3개 한도 내; 현재행 표시와 선택 커서 구별 | NOT_RUN |
| MAIN-04 | 폭 1/24/47/80/120, composer 높이 제한 | 접힘/펼침·resize·제한 렌더 후 메인행 클릭 | 사용 가능한 footer에서 메인행 유지; 실제 그린 행만 선택; composer 초안 유지 | NOT_RUN |
| MAIN-05 | root 없음/다른 root로 바뀜, 오래된 click | 이전 좌표·root/thread ID 이벤트 전달 | 임의 세션을 선택하지 않음; 현재 backend의 정확한 primary만 수용 | NOT_RUN |
| MAIN-06 | 진행 중 turn과 queued input/초안 | child 왕복 | 기존 navigation/cache 경로 사용; 작업 중단/제출/큐 편집 없음 | NOT_RUN |
| HOVER-01 | 영어/한국어, summary/메인/child | 마우스 진입·같은행 이동·이탈, 전후 Buffer 비교 | hover 전후 글자·행높이 동일; 전체 반전 없음; 작은 underlined 단서만 변경; 현재행 표시 유지 | NOT_RUN |
| HOVER-02 | popup·search·selection·resize·root 변경 | 기존 공개 mouse 경로로 hover/click | modal 우선순위·draft·진행 turn 유지; 오래된 hit map 무효화 | NOT_RUN |

선택한 수준: Buffer/MouseEvent → AppEvent → 기존 App::handle_event → 실제 embedded server navigation을 포함하는 기존 fixture. 실제 사용자 GUI 체감·운영 backend·전체 부하·실행 파일 배포는 이 소스 시험의 보장 범위 밖이다. hover 강도 완화는 렌더 스타일/문구 안정성으로 판정하며 사용자 체감 개선은 실측 전 미확인이다.

조정 담당 실행 필터: `cargo test -p codex-tui subagent_status -- --test-threads=1`, `cargo test -p codex-tui footer_ -- --test-threads=1`, `cargo test -p codex-tui expanded_agent_footer -- --test-threads=1`. 실행 명령·대상 commit·종료 코드는 실행 담당이 추가한다. red/green은 본 담당에게 시험 실행이 금지되어 NOT_RUN이며 원본 소스를 되돌려 시행하지 않는다.

통합 3차: subagent_status 21개 중 20 PASS/1 FAIL(exit101). 실패는 owned/inline 두 순회에서 restore_user_message_to_composer를 두 번 호출한 fixture 준비 때문이었다. 기존 input_restore.rs:543의 이 함수는 기존 초안과 입력을 병합한다. 첫 순회에서만 빈 초안에 한 번 넣고 두 번째 순회에서는 보존된 동일 초안을 먼저 assertion하는 방식으로 보정한다. 정확한 단일 초안·큐·turn 보존, 실제 마우스 왕복 oracle는 그대로 유지한다. 원 실패를 제품 red/green으로 포장하지 않는다.


## Windows 최종 개발 시험 결과

위 NOT_RUN은 실행 전 계획이다. 통합 4차의 `subagent_status` 21개, `footer_` 82개, `expanded_agent_footer` 1개, `agent_navigation` 8개는 각각 exit0이다. 필터 간 중복이 있으므로 수를 합산하지 않는다. 실제 MouseEvent → AppEvent → embedded server 탐색에서 owned/inline 메인 왕복, 단일 child 초안, root 초안·진행 turn·대기 입력, 다른 root의 stale 이벤트 거부와 마지막 child 숨김 뒤 복귀가 통과했다. MAIN-01~06 및 HOVER-01~02의 해당 fixture·Buffer 경계가 통과했다.

호버 시험은 EN/KO의 글자·높이 불변, 작은 marker의 밑줄, 전체 반전 없음과 modal/검색/선택 보존을 검사했다. 실제 운영 터미널의 사용자 체감·R38 설치본은 미검증이다. 원래 fixture 오류는 위 기록대로 보존하며 제품 결함 탐지 red/green으로 계산하지 않는다.
