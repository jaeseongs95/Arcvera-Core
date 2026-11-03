# R36: 메모리에 없는 종료 서브에이전트 정리

기존 `preview_completed_children`는 저장된 그래프와 메모리의 자손을 함께 나열했지만 `get_thread(id)`가 실패하면 후보를 무조건 보존했다. 따라서 저장된 종료 대화 수와 `eligible` 수는 일치하지 않을 수 있었다. 각 항목의 보존 사유도 응답에 없었다. 이번 작업에서는 실제 사용자 세션·DB·로그를 열지 않았으므로 사용자에게 제시된 9개/302개의 개별 원인을 재판정한 결과는 아니다.

R36은 `CompletedChildCleanupResult.reason`을 추가한다. 기존 `outcome`과 snapshot 선택 계약을 유지하며, CLI는 JSON과 별도로 outcome/reason별 수를 stderr에 표시한다. `reason`이 없는 구버전 응답은 기존 outcome으로 집계한다.

## 메커니즘과 보존 조건

로드된 런타임은 기존 admission, residency gate, 활동 revision, 마지막 턴, 정확한 런타임 객체, 종료 확인 절차를 사용한다. 메모리에 없는 항목은 모델·런타임을 복원하지 않고 다음 순서로 판단한다.

1. 현재 부모/현재 대화가 로컬에서 실행 중인지 확인한다. 저장된 부모 연결과 `ThreadSpawn` source가 현재 범위까지 이어져야 한다. 영속 그래프가 없으면 `persistent_graph_unavailable`로 보존한다.
2. 해당 UUID의 Core admission을 예약한다. LocalThreadStore의 lifecycle/write 잠금과 기존 OS writer lock을 확보하고, 로컬 live recorder가 있거나 다른 manager가 writer를 보유하면 `writer_owned`로 보존한다. 이 잠금은 확인부터 정확한 삭제까지 유지된다.
3. canonical header의 MAv2, ThreadSpawn source, 부모를 확인한다. 저장 기록은 한 줄씩 읽고 내용 필드는 역직렬화 대상에서 제외한다. 메시지·도구 결과를 history Vec로 복원하거나 모델에 전달하지 않는다. 상속된 기록은 `subagent_history_start_ordinal` 이전까지 제외한다. 페이지형 기록은 ordinal이 정확히 이어져야 한다.
4. 마지막으로 시작한 자체 턴과 종료 이벤트의 ID가 일치해야 한다. 미완성 마지막 줄, 잘못된 JSON, ordinal 누락/역전/간격, rollback, 확인 불가능한 상속 경계, 마지막 종료 뒤 작업 기록, 64MiB를 넘는 스캔은 사유를 붙여 보존한다. 완료·실패·중단은 각각 `completed`·`failed`·`cancelled`로 표현한다.
5. snapshot은 `stored-v1:<부모 런타임 ID>:<durable revision digest>`에 결속한다. 경로와 파일 시각은 public snapshot에 노출하지 않는다. 확인 시 같은 UUID·부모 실행 세대·파일 revision·마지막 턴·종료 상태를 다시 비교한다. 로드된 snapshot을 unload 뒤에 재사용할 수 없다.
6. Core의 예약 아래 pending agent mail을 재확인하고, app-server의 사용자 queue/goal/goal-resume guard를 기존 owner task가 유지한다. 선택하지 않은 자손이 있으면 삭제하지 않는다. 기존 attached-thread/history-reference 검사와 host-owned data 정리는 writer 소유를 유지한 채 그대로 실행한다. 정확히 선택한 UUID 하나만 삭제하고 성공 뒤 관련 로컬 registry 항목을 해제한다.

삭제 owner task는 호출자가 취소되어도 admission·writer·host guard를 유지한다. 저장소 삭제 결과가 불명확하면 성공으로 바꾸지 않고 기존 `storage_unconfirmed_partial_deletion_possible` 결과와 tombstone을 유지한다. 부모·현재 대화, 원격/host 소유, 다른 세대, 활동 중/종료 불명 항목의 보호를 완화하지 않는다.

## 주요 reason

| 코드 | 의미 |
| --- | --- |
| `loaded_terminal`, `unloaded_terminal` | 해당 경로에서 종료와 소유 조건을 확인한 후보 |
| `ownership_unverified` | 저장된 부모/source 체인을 확인할 수 없음 |
| `writer_owned`, `busy` | 다른 소유자·writer·진입 작업과 경합 중 |
| `pending_agent_mail`, `pending_user_queue`, `unfinished_goal` | 보존해야 할 대기 작업 존재 |
| `unsupported_generation`, `unsupported_source`, `unsupported_store` | 지원하지 않는 실행 세대·출처·저장소 |
| `runtime_owner_mismatch`, `runtime_closing`, `runtime_busy`, `runtime_not_terminal` | 로드된 런타임 소유/상태 조건 미충족 |
| `terminal_turn_unverified`, `terminal_turn_mismatch`, `terminal_turn_in_progress` | 마지막 자체 턴의 종료를 확정할 수 없음 |
| `missing_metadata`, `invalid_metadata`, `incomplete_metadata`, `invalid_metadata_ordinal` | 저장된 메타데이터를 안전하게 검증할 수 없음 |
| `inherited_history_unverified`, `rollback_unverified`, `activity_after_terminal` | 상속/되돌림/추가 작업 때문에 종료 판정 불가 |
| `metadata_scan_limit`, `persistent_graph_unavailable` | 조사 상한 초과 또는 영속 그래프 미제공 |
| `changed`, `has_descendants` | 미리보기 이후 변경 또는 보존할 자손 존재 |

## 검증 계획과 실행 상태

CS 불변조건은 (C1) UUID·소유 범위·실행 세대의 혼동 금지, (C2) metadata 검사와 삭제 사이 writer 변경 금지, (C3) queue/mail/goal 보존, (C4) 취소 뒤 owner 및 잠금 유지, (C5) 선택 집합 확장 금지다. 기존 Tokio/LocalThreadStore 격리 fixture를 사용하며 새 runner나 의존성을 추가하지 않는다.

| 사례/공개 경계 | 기대 결과와 불변조건 |
| --- | --- |
| `unloaded_terminal_cleanup_uses_metadata_without_loading_and_deletes_only_selection` | preview 시 복원 없이 eligible, 확인 후 선택 UUID만 삭제, 부모·미선택 형제 보존(C1/C5) |
| `unloaded_cleanup_rejects_new_turn_after_preview_and_keeps_history` | 미리보기 뒤 새 시작 턴이 생기면 삭제하지 않고 기록 보존(C1/C2) |
| `unloaded_cleanup_preserves_a_foreign_manager_writer_and_stale_loaded_snapshot` | 기존 loaded snapshot 거부, 별도 store writer 경합은 사유와 함께 보존(C1/C2) |
| `unloaded_cleanup_owner_keeps_writer_and_admission_after_request_cancellation` | 실제 host-cleanup callback에서 멈춘 뒤 호출자를 취소해도 writer/admission이 유지되고 owner만 삭제를 마침(C2/C4) |
| `metadata_cleanup_requires_matching_latest_terminal_and_rejects_uncertain_tail` | 시작/완료/중단·ID 불일치·새 시작·부분 기록·잘못된 JSON·rollback·종료 뒤 사용자/미래 이벤트 판정(C1/C3) |
| `metadata_cleanup_never_uses_inherited_parent_completion_as_child_completion` | 부모 완료를 자체 완료로 쓰지 않고 자체 경계 이후 턴만 인정, ordinal 간격 보존(C1) |
| `cleanup_summary_counts_preservation_reasons_and_accepts_older_responses` | 언어별 사유 집계와 reason이 없는 구버전 응답 역직렬화 |
| 기존 Core/app-server cleanup suite | 실제 shutdown 실패, stale Arc, host queue/goal, 선택하지 않은 자손, 저장 오류 보호 회귀(C1–C5) |

이 문서 작성 시 Cargo/시험 실행은 통합 담당에게 일원화되어 **NOT_RUN**이다. 위 7개 신규 시험의 실행 근거, 기존 suite 결과, 프로토콜 생성물, 독립 감사, 실제 앱 부하 조건과 효과는 통합 검증에서 확인해야 한다. 포맷 명령은 지정된 Rust 1.99.1의 `rustfmt.exe --edition 2024 --config skip_children=true`로 소유 파일에만 실행했다. 이 문서만으로 실제 사용자 항목의 삭제 가능 여부나 문제 해결을 선언하지 않는다.
