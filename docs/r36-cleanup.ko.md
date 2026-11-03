<!-- Modified for OpenDIY Codex R36 (2026-11-03); see MODIFICATIONS.md. -->
# R36: 종료 서브에이전트 정리 사유와 보존 경계

기존 `preview_completed_children`는 저장된 그래프와 메모리의 자손을 함께 나열했지만 `get_thread(id)`가 실패하면 후보를 무조건 보존했다. 따라서 저장된 종료 대화 수와 `eligible` 수는 일치하지 않을 수 있었다. 각 항목의 보존 사유도 응답에 없었다. 이번 작업에서는 실제 사용자 세션·DB·로그를 열지 않았으므로 사용자에게 제시된 9개/302개의 개별 원인을 재판정한 결과는 아니다.

제품의 **unloaded cold 삭제는 운영 미완이며 기본 보존**한다. 구버전 writer의 참조 게시 참여를 입증할 장치가 없으므로 preview와 confirm 양쪽에서 `reference_participants_unverified`를 반환한다. 운영 설정·환경 변수·자동 허용 경로를 추가하지 않는다. 아래 metadata-only 삭제 절차는 저장소 경계와 private `cfg(test)` 임시 home fixture에서만 검증하며, 운영 지원 완료를 뜻하지 않는다. 기존 loaded 삭제의 지원 범위를 넓히지 않으며, 구버전과 경합하는 기존 위험도 별도 미해결 조건으로 남긴다.

R36은 `CompletedChildCleanupResult.reason`을 추가한다. 기존 `outcome`과 snapshot 선택 계약을 유지하며, CLI는 JSON과 별도로 outcome/reason별 수를 stderr에 표시한다. `reason`이 없는 구버전 응답은 기존 outcome으로 집계한다.

## 메커니즘과 보존 조건

로드된 런타임은 기존 admission, residency gate, 활동 revision, 마지막 턴, 정확한 런타임 객체, 종료 확인 절차를 사용한다. 메모리에 없는 항목은 모델·런타임을 복원하지 않고 다음 순서로 판단한다.

1. 현재 부모/현재 대화가 로컬에서 실행 중인지 확인한다. 저장된 부모 연결과 `ThreadSpawn` source가 현재 범위까지 이어져야 한다. 영속 그래프가 없으면 `persistent_graph_unavailable`로 보존한다.
2. 해당 UUID의 Core admission을 예약한다. LocalThreadStore의 lifecycle/write 잠금과 기존 OS writer lock을 확보하고, 로컬 live recorder가 있거나 다른 manager가 writer를 보유하면 `writer_owned`로 보존한다. 이 잠금은 확인부터 정확한 삭제까지 유지된다.
3. canonical header의 MAv2, ThreadSpawn source, 부모를 확인한다. 저장 기록은 한 줄씩 읽고 내용 필드는 역직렬화 대상에서 제외한다. 메시지·도구 결과를 history Vec로 복원하거나 모델에 전달하지 않는다. 상속된 기록은 `subagent_history_start_ordinal` 이전까지 제외한다. 페이지형 기록은 ordinal이 정확히 이어져야 한다.
4. 마지막으로 시작한 자체 턴과 종료 이벤트의 ID가 일치해야 한다. 미완성 마지막 줄, 잘못된 JSON, ordinal 누락/역전/간격, rollback, 확인 불가능한 상속 경계, 마지막 종료 뒤 작업 기록, 64MiB를 넘는 스캔은 사유를 붙여 보존한다. 완료·실패·중단은 각각 `completed`·`failed`·`cancelled`로 표현한다.
5. snapshot은 `stored-v1:<부모 런타임 ID>:<durable revision digest>`에 결속한다. 경로와 파일 시각은 public snapshot에 노출하지 않는다. 확인 시 같은 UUID·부모 실행 세대·파일 revision·마지막 턴·종료 상태를 다시 비교한다. 로드된 snapshot을 unload 뒤에 재사용할 수 없다.
6. Core의 예약 아래 pending agent mail을 재확인하고, app-server의 사용자 queue/goal/goal-resume guard를 기존 owner task가 유지한다. 선택하지 않은 자손이 있으면 삭제하지 않는다. 기존 attached-thread/history-reference 검사와 host-owned data 정리는 writer 소유를 유지한 채 그대로 실행한다. 정확히 선택한 UUID 하나만 삭제하고 성공 뒤 관련 로컬 registry 항목을 해제한다.

삭제 owner task는 호출자가 취소되어도 admission·writer·host guard를 유지한다. attached-child와 외부 history reference 검사는 host cleanup·파일·DB 변경 전에 끝낸다. 이 사전 검사에서 거부되면 `storage_rejected_before_deletion`을 반환하고 cold admission 예약을 해제한다. 로드된 항목도 이 검사를 close/shutdown 전에 수행하므로 명확한 사전 거부가 런타임을 종료하지 않는다. 저장소는 실제 첫 변경 직전에 callback으로 admission을 확정한다. 그 뒤 발생한 오류는 파일이 남아 있더라도 부분 변경 가능성을 배제하지 않고 `storage_unconfirmed_partial_deletion_possible`과 tombstone을 유지한다. 오류 문자열이나 오류 enum만으로 변경 여부를 추정하지 않는다. 부모·현재 대화, 원격/host 소유, 다른 세대, 활동 중/종료 불명 항목의 보호를 완화하지 않는다.

## 프로세스 간 참조 게시 경계

기존 store 단위 lifecycle 예약만으로는 같은 home의 다른 `LocalThreadStore`가 보유한 `PreparedFork`를 볼 수 없었다. 격리된 실제 시험(`236135ca6`, `r36-store-followup-tests-01`)에서 A의 원본 삭제와 B의 후속 fork 게시가 모두 `Ok(())`여서 dangling reference assertion이 실패했다. 이 RED는 예상 실패로 덮거나 통과로 집계하지 않는다.

수정은 기존 `WriterLockCoordinator`와 표준 OS 파일 잠금을 재사용한다. `thread-writer-locks/.references.lock`의 shared 잠금은 fork 준비부터 canonical header 게시까지, exclusive 잠금은 삭제 scan부터 파일·DB 삭제가 끝날 때까지 유지한다. 이 파일은 잠긴 inode를 교체하지 않도록 해제 뒤에도 unlink하지 않는다. shared/exclusive 모두 비차단이며 경합 시 새 참조 게시는 retryable error, cold 정리는 `reference_publication_busy`로 보존한다. home 전체가 하나의 참조 게시 영역이므로 다른 항목의 준비 중 fork도 정리를 보수적으로 막을 수 있다.

`PreparedFork`는 기존 source 예약과 shared File을 함께 보관한다. 참조가 있는 recorder는 지연 생성 중에도 background writer state가 별도 shared File을 가지고, header와 pending 기록의 flush 성공 뒤 해제한다. persist 답변을 기다리던 호출자가 취소되거나 persist가 실패해도 게시 보호가 먼저 풀리지 않는다. 직접 create도 source 파일 존재를 shared 영역에서 재확인하므로 예약이 해제된 stale `PreparedFork` 값으로 삭제된 원본을 가리킬 수 없다. 활성 원본 writer의 독점 UUID 잠금을 fork 차단 수단으로 사용하지 않는다.

revert replacement, archive/unarchive의 파일 이동, compression의 게시·원본 제거도 shared 영역에 들어간다. 일반 hard-delete와 이동·revert의 owner task는 호출자가 취소돼도 실제 파일 작업이 끝날 때까지 잠금을 유지한다. 삭제의 기존 local lifecycle 대기를 유지한 뒤 exclusive File을 비차단 획득하며, 다중삭제는 기존 UUID 정렬과 한 번의 exclusive 획득을 사용한다. writer coordination 파일과 참조 파일은 별개이므로 WriterLockGuard의 Drop이 같은 참조 잠금을 재진입하지 않는다.

**혼합 구버전은 미해결 조건이므로 제품에서 cold 삭제를 거부한다.** R28/R34 등 구버전 바이너리는 새 shared lease에 참여하지 않는다. 기존 UUID writer File에는 PID·실행파일 버전·capability 기록이 없으며, 일시적으로 writer 잠금이 비었다는 사실이나 저장된 MAv2 header만으로 구버전의 보류 fork·미래 게시가 없음을 증명할 수 없다. 현재 수정만으로 구버전과 같은 home에 쓰는 운영 환경까지 안전하다고 판정할 수 없으므로 unloaded preview/confirm을 기본 보존한다. `cfg(test)` private fixture 값은 실제 TempDir home에서 깊은 경합 회귀만 실행하며 제품 빌드에는 존재하지 않는다. 이 조건을 문서나 신규 store 시험의 PASS로 대체하지 않는다. 실제 사용자 데이터·프로세스에 접근하거나 구버전을 종료·재시작하지 않았다.

## 주요 reason

| 코드 | 의미 |
| --- | --- |
| `loaded_terminal`, `unloaded_terminal` | 해당 경로에서 종료와 소유 조건을 확인한 후보 |
| `ownership_unverified` | 저장된 부모/source 체인을 확인할 수 없음 |
| `reference_participants_unverified` | 모든 writer의 참조 잠금 참여를 입증할 수 없어 제품의 cold preview/confirm 보존 |
| `reference_publication_busy` | 다른 참조 준비·게시 또는 삭제와 경합하여 보존 |
| `writer_owned`, `busy` | 다른 소유자·writer·진입 작업과 경합 중 |
| `pending_agent_mail`, `pending_user_queue`, `unfinished_goal` | 보존해야 할 대기 작업 존재 |
| `unsupported_generation`, `unsupported_source`, `unsupported_store` | 지원하지 않는 실행 세대·출처·저장소 |
| `runtime_owner_mismatch`, `runtime_closing`, `runtime_busy`, `runtime_not_terminal` | 로드된 런타임 소유/상태 조건 미충족 |
| `terminal_turn_unverified`, `terminal_turn_mismatch`, `terminal_turn_in_progress` | 마지막 자체 턴의 종료를 확정할 수 없음 |
| `missing_metadata`, `invalid_metadata`, `incomplete_metadata`, `invalid_metadata_ordinal` | 저장된 메타데이터를 안전하게 검증할 수 없음 |
| `inherited_history_unverified`, `rollback_unverified`, `activity_after_terminal` | 상속/되돌림/추가 작업 때문에 종료 판정 불가 |
| `metadata_scan_limit`, `persistent_graph_unavailable` | 조사 상한 초과 또는 영속 그래프 미제공 |
| `storage_rejected_before_deletion` | 실제 삭제 전 reference 검사에서 거부됨; admission 재진입 가능 |
| `storage_unconfirmed_partial_deletion_possible` | 실제 변경 경계를 통과한 뒤 저장 오류; tombstone 유지 |
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
| `unloaded_cleanup_reference_rejection_releases_admission_and_allows_real_reload` | attached/external reference 사전 거부 뒤 원본·참조 보존, admission 해제와 실제 재로드 가능 |
| `loaded_cleanup_reference_rejection_preserves_live_owner_and_admission` | 같은 사전 거부가 live owner·admission·런타임을 종료하지 않음 |
| `unloaded_cleanup_retains_tombstone_after_host_cleanup_mutation_boundary_failure` | host cleanup 시작 경계를 통과한 오류는 실제 원본이 남아도 tombstone 유지 |
| `metadata_cleanup_cannot_publish_dangling_cross_store_prepared_fork` | 별도 A/B store가 같은 격리 home/DB를 사용; B.prepare_fork 보유 → A cold preview/delete → B.create/persist에서 삭제된 부모를 가리키는 참조를 게시하면 실패 |
| `unloaded_cleanup_defaults_to_preservation_when_participants_are_unverified` | 제품 기본 경로에서 cold preview와 loaded snapshot confirm 모두 사유와 함께 보존, 기록/admission/host callback 유지 |
| `metadata_cleanup_rejects_stale_fork_after_complete_set_multi_delete` | 같은 영역의 complete-set 다중삭제 뒤 예약 해제된 stale prefix를 직접 create해도 missing source로 거부 |
| `reference_leases_share_with_active_writers_and_never_split_the_namespace` | 활성 writer와 concurrent shared lease 공존, exclusive 경합·Drop·stale-cleanup·compression 게시 보호 |
| `reference_leases_are_enforced_across_processes` | 실제 같은 test executable의 자식 process가 parent shared File 보유/해제 전후 exclusive 결과 확인 |
| `reference_publication_lease_survives_cancelled_reply_and_failed_persist` | 실제 생성 실패·재시도와 취소된 persist reply에도 background lease를 유지하고 durable header 뒤 해제 |
| `reference_deletion_owner_retains_exclusion_after_caller_cancels` | 실제 삭제 검사 후 barrier에서 호출자를 취소해도 File과 원본을 유지하다 owner가 삭제 완료(test-support) |
| 기존 Core/app-server cleanup suite | 실제 shutdown 실패, stale Arc, host queue/goal, 선택하지 않은 자손, 저장 오류 보호 회귀(C1–C5) |

Cargo/시험 실행은 통합 담당에게 일원화한다. 통합 담당의 `d5ca137a5` 격리 실행 영수증에는 14개 filter 모두 exit 0, `sourceFilesUnchanged=true`가 기록되었고 기존 cold 4개 시험의 PASS 로그를 확인했다. 이번 사전 거부 수정의 Core 3개와 추가한 기본 보존·참조 lease·취소·다중삭제 시험은 아직 **NOT_RUN**이다. 기존 cross-store 저장소 fixture의 수정 전 실행은 RED이며 수정 후 재실행은 아직 하지 않았다. 특히 cross-store 시험은 안전성을 판별하는 회귀 시험이며, 해당 경계 수정 소스를 추가했으나 수정 후 통과는 아직 확인하지 않았다. 기존 lifecycle 예약의 store 인스턴스 경계를 넘는 실제 실패는 RED로 확인했고, 수정 후 결과는 후속 실행에서 확인해야 한다.

`reason` 생성물은 실제 Rust generator의 stable/experimental 각 1개 시험 exit 0으로 생성했다. `manual-generated-schema`와 통합 runner의 `generated-schema`는 1,118개 파일의 상대 경로와 SHA-256이 모두 일치했다. 의미가 바뀐 JSON 3개, TypeScript 1개, precomputed 압축 export 2개만 반영했으며 텍스트는 저장소 CRLF를 유지했다. 생성 실행의 source는 `d5ca137a5`이고 이번 사전 거부 패치는 protocol 선언을 바꾸지 않는다. 새 시험, 생성물 fixture 대조, 독립 감사, 실제 앱 부하 조건과 효과는 후속 통합 검증에서 확인해야 한다. 포맷 명령은 지정된 Rust 1.99.1의 `rustfmt.exe --edition 2024 --config skip_children=true`로 소유 파일에만 실행했다. 이 문서만으로 실제 사용자 항목의 삭제 가능 여부나 문제 해결을 선언하지 않는다.
