# R37 replay·compact 사전 사례

대상: `1b441423baf713d144647d5be37525f7d9cc03c0`에서 시작한 R37 작업 후보. R36 원본·실행본·발행 자산과 운영 데이터는 변경하지 않는다. 합성 입력만 사용한다. Cargo 실행은 root가 직렬로 수행한다.

## 요구와 판정 기준

- T05/R-ID: 같은 thread/turn/item의 동일 payload가 snapshot·buffer 또는 재연결 buffer에 겹치면 사용자/assistant 본문은 한 번 표시한다. 서로 다른 thread/turn/item과 변경된 payload는 단순 본문 비교로 지우지 않는다.
- T06/R-REPEAT: 같은 본문을 가진 서로 다른 ID의 실제 제출은 각각 표시한다.
- T07/R-LIVE: 진행 중 commentary의 완료 item과 재연결/채팅 전환의 replay가 겹쳐도 완료 상태·진행 중 델타·대기 질문/승인·terminal event를 보존한다.
- T08/R-EVIDENCE: 저장 1건/화면 2건, 실제 제출 2건을 별도로 판정한다. 실제 화면 재현이 없으면 사용자 현상 해결 완료를 선언하지 않는다.
- T23~25/R-COMPACT: 부모 foreground의 idle 여부로 admission을 결정하고 자식 실행을 임의 중단하지 않는다. 자연어 요청은 originating turn 완료 뒤 한 번의 idle admission만 한다. busy·실패·취소·runtime 교체를 성공으로 바꾸거나 자동 재요청하지 않는다. 합성 지연 시험은 실제 지연 원인 해결의 근거가 아니다.

## engineering-test-plan

기존 runner: `codex-rs/tui/Cargo.toml`의 Rust unit/integration test. 공개 동작 oracle: `App::replay_thread_snapshot`이 내보내는 `AppEvent::InsertHistoryCell`의 본문 개수와 widget의 running 상태. 직접 filter 검사는 payload·질문·미완 델타 보존이라는 내부 계약만 검사한다. 제품 구현과 같은 ID-set 알고리즘을 oracle로 복사하지 않는다.

| case | 사전조건·입력·절차 | 독립 기대값·거부할 오류 | 경계/제외 |
| --- | --- | --- | --- |
| P01/T05 | completed snapshot와 동일 ID user/assistant ItemCompleted 및 TurnCompleted를 replay | question=1, answer=1, terminal 완료 | 기존 `snapshot_and_buffered_replay_render_same_message_ids_once` 재사용 |
| P02/T06 | 동일 본문, 다른 turn/item ID를 replay | question=2, answer=2 | 기존 `snapshot_and_buffered_replay_preserve_same_text_with_distinct_message_ids` 재사용 |
| P03/T05 | history 없는 buffer에 동일 user/assistant completion을 두 번 넣고 replay | question=1, answer=1; user 중복 2건 또는 assistant 재렌더는 실패 | reconnect notification 중복 경계, 네트워크 자체 제외 |
| P04/T07 | in-progress snapshot의 이미 완료된 user/commentary와 동일 buffer completion | 완료 본문 1회; 미완 item의 새 델타·완료 상태 보존 | 실제 renderer 동작 확인 후 결함 여부 판단; partial payload를 임의 제거하지 않음 |
| P05/T07 | snapshot와 같은 ID이나 최신 payload가 변경됐고 질문/승인/미완 델타도 존재 | 변경 payload·질문·요청·미완 델타 보존 | 기존 보존 시험 재사용 |
| P06/T23~24 | originating turn 완료·다른 turn 완료·중복 완료 / busy·notStarted·started | origin 완료 때 `thread/compact/startIfIdle` 1회, 이후 자동 재시도 0회 | 기존 `natural_compact_waits_for_original_turn_and_uses_one_idle_admission` 재사용 |
| P07/T24 | failed/interrupted/runtime 교체 뒤 origin terminal notification | admission 요청 0회 | 기존 `natural_compact_does_not_run_after_interruption_or_runtime_replacement` 재사용 |
| P08/T23 | parent idle + child running, backend admission 소스와 기존 시험 대조 | parent의 foreground 상태만 판단하고 child 작업 보존 | backend 담당 변경 필요 시 root에 인계 |
| P09/T08,T25 | 실제 UI reconnect/채팅 전환과 허용된 단계 계측 | 화면 개수·server 저장 개수 대조, 단계별 실제 소요시간 | NOT_RUN: 운영 앱/대화/로그 접근 범위 없음 |

## CS 조건과 근거 계획

분산 이벤트의 전달 중복과 순서를 구분한다. 중복 identity는 thread+turn+nonempty item ID+동일 payload이며 본문 자체가 아니다. replay reader는 원본 store를 변경하지 않는다. 제거는 표시 중복에 한정하고 terminal/pending interactive 상태는 보존한다. 이 조건의 검증은 P01~P05와 기존 request/delta 보존 사례로 한다.

compact 요청 수명은 origin turn과 runtime registration에 귀속된다. origin 완료 시 요청을 소비한 뒤 admission 한 번만 하며 fail/busy는 자동 재시도하지 않는다. P06~P08로 경계를 확인한다.

초기 상태: 전 사례 NOT_RUN. 기존 테스트의 과거 PASS는 R37 실행 근거로 사용하지 않는다. source 조사·추가 사례·root 실행 결과를 후속 절에 구분하여 기록한다.

## 조사·구현 결과 (Cargo 실행 전)

확인된 소스 공백은 두 개다. `replay_filter::omit_messages_covered_by_history`는 completed turn만 참조하므로 in-progress turn 안의 완료 시각이 기록된 user/commentary가 buffer와 겹쳐도 제외하지 않았다. 또한 history가 비어 있는 buffer의 동일 ItemCompleted 반복에는 메시지 identity 필터가 없었다. `ChatWidget::replay_thread_turns`는 in-progress의 message payload도 렌더하며 `handle_item_completed_notification`은 replay completion을 다시 message handler로 넘긴다. 이는 공개 renderer 회귀 사례를 만들 수 있는 경로이며 사용자 실제 화면 발생 원인의 확정은 아니다.

최소 수정: 완료 timestamp가 있는 item을 history coverage에 포함하고, buffer에서 같은 thread/turn/nonempty item ID와 동일 payload의 후속 completion만 표시를 억제한다. user completion은 제외하고 assistant는 text만 비워 completion 상태·questions를 유지한다. 서로 다른 ID와 변경 payload를 보존한다. 필터는 snapshot copy에서만 실행하며 per-thread 영구 상태나 새로운 dependency를 만들지 않는다. history coverage 검사 전에 payload를 바꾸면 coverage가 무효화될 수 있으므로 순서는 history overlap → repeated completion → delta replay로 고정한다.

추가 시험 filter:

- `app::tests::buffered_replay::buffered_replay_renders_repeated_completed_message_ids_once`
- `app::tests::buffered_replay::active_snapshot_and_buffered_completed_commentary_render_once`
- `app::tests::buffered_replay::repeated_completion_filter_preserves_changed_payloads_and_identity_boundaries`

기존 회귀는 `app::tests::buffered_replay` 전체로 실행한다. 수정 전 red 실행은 하지 않았다. source에서 결함 경로를 확인했으나 이를 red 실행으로 기록하지 않는다. root가 안정 후보에서 Cargo를 직렬로 실행한다. rustfmt는 직접 `1.99.1` toolchain 실행 파일로 세 소유 Rust 파일에만 실행하여 종료 0이었다(기존 unstable imports_granularity 경고 있음). 기존 CRLF를 유지했고 세 파일의 `git diff --check`는 종료 0이었다.

compact 소스 대조:

- `core/src/session/turn_input.rs::reserve_compact_if_idle`는 정확한 부모 `active_turn`과 부모 pending mailbox만 같은 admission lock 아래 검사한다. 자식 실행을 순회하여 busy로 바꾸거나 종료하지 않는다.
- 기존 `core/src/thread_manager/idle_control_followup_tests.rs::idle_compact_real_parent_admission_preserves_running_child_and_relationship`는 실제 loaded root/child runtime과 held task를 사용하여 부모 admission, child cancellation=false, parent/child 관계 보존을 검사한다.
- `tui/src/app_server_session.rs::next_event`는 origin thread+turn terminal event에서 pending request를 `take()`하여 한 번만 처리하고 completed일 때만 `thread/compact/startIfIdle`를 한 번 호출한다. disconnected/stream 종료는 pending을 지우고 runtime lifetime 취소는 admission을 막는다. busy·notStarted·error·timeout을 각각 구분하며 자동 재요청 루프는 없다.
- `tui/src/app/session_control_tests.rs::natural_compact_waits_for_original_turn_and_uses_one_idle_admission`, `natural_compact_does_not_run_after_interruption_or_runtime_replacement`는 그대로 재사용한다. 이 파일들은 root 소유로 변경하지 않았다.
- `core/src/tasks/compact.rs`, `core/src/compact.rs`에는 준비·원격/로컬 요약 경계 timer가 존재한다. 실제 허용 로그를 읽지 않았으므로 느림의 지배 구간이나 개선율을 제시하지 않는다. 계측의 존재는 지연 해결의 근거가 아니다.

이 조사에서 compact backend의 새 결함은 확인하지 못했다. 기존 시험 재실행과 실제 단계별 관측이 남으며 source만으로 사용자 상태를 정상/해결로 판정하지 않는다. compact 제품 코드는 변경하지 않았다.
