<!-- Modified for OpenDIY Codex R37 (2026-11-04); see MODIFICATIONS.md. -->
# R37 메모리 처리 검증 계획

대상은 `codex-memories-write`의 명시적 노트 접수와 기존 phase-2 통합 경로다. 운영 메모리, 인증, 사용자 DB 및 다른 세션의 대화 원문은 사용하지 않는다. 기존 Tokio/Cargo runner와 임시 디렉터리·실제 SQLite fixture·로컬 합성 Responses 응답을 사용한다. Cargo 실행과 실제 app-server 연결 시험은 통합 담당이 직렬로 수행한다.

## 공개 경계와 독립 판정 기준

| 사례 | 요구와 공개 진입점 | 기대 결과·oracle | 수준 |
| --- | --- | --- | --- |
| M01 | `add_user_memory_note` 접수 | 반환된 파일에 정정이 저장되지만 `memory_summary.md`는 생성·변경되지 않는다. 접수는 반영 완료가 아니다. | 실제 파일 통합 |
| M02 | 노트 중복·재시도 | 동일한 명시 요청을 재전송해도 별개의 반영 완료로 오인하지 않는다. 저장 횟수와 대기 상태를 구분한다. | 실제 파일/프로토콜 |
| M03 | 명시적 통합 시작 | 선택된 root/version의 기존 phase-2만 호출한다. 다른 프로젝트·전역 root와 phase-1의 다른 대화 추출은 호출하지 않는다. | phase-2 통합 |
| M04 | 동시 통합·노트 작성 | 기존 root 파일 잠금과 DB lease를 유지하고 busy를 식별한다. 실행 중 노트가 정상 접수됐다고 응답하지 않는다. | 실제 파일 잠금/DB |
| M05 | 통합 성공 조회 | 모델 종료만으로 성공 처리하지 않는다. 실제 산출물 검증·baseline 갱신·DB 완료 뒤에만 완료가 관측된다. | 합성 모델/실제 파일/DB |
| M06 | 통합 실패·재시도 | 실패 이유와 retry 제한을 보존한다. 명시적 재시도도 실행 중 lease를 빼앗지 않는다. 새 작업 성공으로 원래 실패 기록을 대체하지 않는다. | 실제 DB |
| M07 | root 격리 | project A 노트·통합·상태 조회가 project B 및 global fixture를 변경하지 않는다. | 실제 분리 fixture |
| M08 | 비활성·환경 제약 | 기능 off, generation off, 비지원 환경과 scope 미확인은 각각 실행 전 거부/미지원으로 표시하고 자동 활성화하지 않는다. | 공개 API 통합 |
| M09 | 대기와 완료 구분 | 접수된 노트가 baseline에 포함되기 전에는 pending이다. 기존 성공 DB 행이 있어도 새 노트 접수를 완료로 표시하지 않는다. | 실제 workspace git |

## CS 불변조건과 검증 의무

- CS-M1: scope/root/version은 요청을 처리하는 thread의 고정 선택을 사용한다. M03/M07로 확인한다.
- CS-M2: 파일 잠금과 DB ownership token이 동시에 작업 소유권을 지킨다. worker 종료·산출물 검증·baseline 갱신 전 잠금을 해제하지 않는다. M04/M05로 확인한다.
- CS-M3: 접수 ACK, 통합 admission, 실제 완료는 별개 상태다. 오류·busy·retry 제한을 성공으로 변환하지 않는다. M01/M04/M06/M09로 확인한다.
- CS-M4: 명시 통합은 phase-1의 대화 추출과 상시 서버를 새로 만들지 않고 기존 phase-2를 재사용한다. 실제 호출 경로 정적 검사와 M03으로 확인한다.

## 근거와 공백

`start.rs`의 `consolidate_memories`는 phase-1을 호출하지 않고 기존 phase-2에 진입한다. `memory_consolidation_status`는 thread config의 project/global 및 version으로 기존 store를 고르고, 행 없음과 DB/환경 오류를 구분한다. 상태 조회는 개별 노트의 의미 반영을 증명하지 않는다. 명시적인 `retry: true`만 cooldown/backoff를 우회하며 유효 lease와 파일 잠금은 계속 적용된다.

| 시험 이름 | 사례 | 실행 계획 |
| --- | --- | --- |
| 기존 `user_notes_preserve_utf8_use_unique_names_and_redact_before_storage` | M01/M02 | `cargo test -p codex-memories-write user_notes_` |
| 기존 `user_notes_return_would_block_while_consolidation_owns_root` | M04 | 같은 명령 |
| `explicit_consolidation_reports_workspace_busy_without_claim_or_model_request` | M03/M04 | `cargo test -p codex-memories-write explicit_consolidation` |
| `explicit_consolidation_respects_live_generation_disabled_without_creating_workspace` | M08 | 같은 명령 |
| `explicit_consolidation_uses_only_phase2_and_reports_invalid_artifacts_as_failure` | M03/M05/M06 | 같은 명령 |
| `explicit_consolidation_completion_is_job_state_not_a_note_application_receipt` | M05/M09 | 같은 명령 |
| `consolidation_status_observes_pending_running_failure_retry_and_completion` | M04/M06 | `cargo test -p codex-state consolidation_` |
| `consolidation_status_and_retry_are_isolated_by_project_and_version` | M07 | 같은 명령 |
| `explicit_consolidation_retry_has_one_winner_across_independent_pools` | M04 | 같은 명령 |

구현 담당은 Cargo를 실행하지 않는다. 위 동적 시험 및 red/green은 통합 담당의 직렬 실행 전까지 NOT_RUN이다. 정적 근거: 지정된 Rust 1.99.1 `rustfmt --edition 2024 --config skip_children=true,newline_style=Windows`가 변경한 Rust 9파일에서 종료 0(안정 채널의 imports_granularity 경고 포함), 소유 경로 `git diff --check` 종료 0. 최초 PATH 기반 rustfmt 시도는 실행 파일을 찾지 못한 실행기 오류였으며 동적 시험 실패로 계산하지 않는다.

protocol/frontend 연결과 실제 실행 파일의 자연어 제어→wire→접수/통합/상태 조회는 통합 담당이 수행하며, 이 crate 시험으로 전체 E2E 성공을 대신하지 않는다. 모델 응답을 로컬 합성 SSE로 대체하므로 실제 제공자의 메모리 선택 품질과 노트의 의미 반영은 이 시험으로 검증하지 않는다. `done`인 기존 DB 행이 있어도 이후 접수된 노트가 반영됐다고 판정하지 않는 반례를 명시적으로 검사한다.

## Transport 연결 보강

`ThreadMemoryConsolidate`는 `thread/memory/consolidate`에 연결하고, 기본 `retry`는 false다. request macro가 기존 experimental gating과 thread별 직렬화, 응답 payload 변환을 생성한다. `v2::memory`와 crate 루트는 glob 재수출하므로 별도 Rust export 목록은 필요하지 않다. app-server는 loaded thread에서 config와 permission profile/source를 읽으며, 일반 turn 경로를 우회하는 명시 통합에도 기존 `thread/sample`과 같은 최신 managed provider/network-policy 검사를 적용한다. scope/version/root를 새 request 인자로 받지 않는다.

신규 protocol 시험 3개: `memory_consolidate_defaults_to_normal_admission_and_preserves_explicit_retry`, `memory_consolidation_response_distinguishes_admission_and_persisted_failure`, `memory_consolidation_dispatch_is_experimental_and_serialized_by_selected_thread`.

`app-server/tests/suite/v2/memory_consolidate.rs`는 실제 stdio app-server와 자기 임시 home/project/SQLite만 사용한다. `memory_consolidation_wire_preserves_disabled_feature_and_generation_gates`는 기능 off와 thread 생성 off를, `memory_consolidation_wire_reports_cross_process_workspace_busy_without_other_scope_changes`는 별도 프로세스 파일 잠금과 A/B 격리를, `memory_consolidation_wire_status_uses_selected_project_and_global_watermarks`는 A/B/global의 서로 다른 error/running/done 상태·watermark·includeDisabled·기본 retry backoff를 검사한다. `cargo test -p codex-app-server --test all memory_consolidation_wire`로 실행한다. protocol 시험은 `cargo test -p codex-app-server-protocol --lib memory_consol`로 실행한다. 이 추가 6개도 Cargo 실행 전까지 NOT_RUN이다.

TUI 제어의 20초 시간 초과는 실제 admission 부재를 증명하지 않는다. in-process typed request 자체에는 별도 deadline이 없고 backend admission은 rate 확인·workspace 준비·worker 제출까지 기다린다. 시간 초과/응답 유실 때 자동 재전송하지 않고 상태를 조회해야 한다. `accepted`는 worker 제출 확인이며 최종 성공이 아니다. 실제 provider 거부 시험과 자연어 tool부터 시작하는 TUI 전체 E2E는 별도 공백이다.

precomputed TS/JSON `.zst`는 소스 macro 변경만으로 갱신되지 않는다. 통합 담당이 `codex-rs`에서 `python app-server-protocol/scripts/write_schema_fixtures.py`와 같은 명령의 `--experimental` 변형을 직렬 실행하고 schema 일치 시험을 수행한다. 기본 명령은 SDK 산출물 갱신도 포함하므로 대상 소유 범위를 먼저 확인한다. 이 구현자는 생성 명령을 실행하지 않았다.

## 명시적인 노트 저장 범위

추가 요구는 현재 대화의 읽기 namespace를 보존하면서 사용자가 명시한 프로젝트 노트를 저장하는 것이다. `memory_scope(project)`가 이후 대화 설정을 바꾼 사실만으로 현재 global 대화의 노트를 project에 저장했다고 보고해서는 안 된다. 현재 배포된 도구로 요청한 노트가 실제 저장됐다는 주장은 하지 않는다.

| 사례 | 공개 진입점과 경계 | 독립 oracle |
| --- | --- | --- |
| S01 | `memory_note` / `thread/memory/note/add`, scope 생략 | 기존 pinned root에만 저장하고 설정·읽기·통합 namespace를 바꾸지 않는다. |
| S02 | global 대화에서 명시 `scope: project` | 현재 loaded thread의 검증된 local project identity로 별도 저장 대상만 정한다. 응답의 scope/root가 실제 노트 파일의 위치와 일치한다. global 폴더와 config 파일은 바뀌지 않고 `memory_view`는 계속 global이다. |
| S03 | project 대화에서 명시 `scope: global` | 명시 요청한 global 저장 대상만 사용한다. 현재 project 조회 root는 그대로다. |
| S04 | project identity 미확인·untrusted·원격/복수 환경·리다이렉트 | 저장 전에 원인을 구분해 거부한다. global fallback은 없으며 어느 노트 디렉터리에도 파일을 만들지 않는다. |
| S05 | 임의 scope/경로/다른 thread 입력, scope 없는 기존 클라이언트 | enum 밖의 scope와 도구의 임의 path/threadId는 거부한다. 기존 scope 생략 요청·구형 응답 decode는 유지한다. |
| S06 | note 저장과 의미 반영 | 응답은 실제 저장 scope/root 및 pending만 확인한다. 현재 읽는 요약과 기존 대화를 변경하거나 통합 성공을 주장하지 않는다. |

`scope`는 optional additive enum이며 임의 경로나 project key를 받지 않는다. 동일한 현재 scope 요청은 고정 identity를 재사용한다. 다른 project 저장 대상을 명시했을 때만 thread의 설정된 cwd와 configured project markers를 해석하고, 신뢰·로컬 환경·경로 검증 후 기존 파일 저장 함수를 호출한다. global/project 설정값이나 메모리 주입 namespace를 수정하지 않는다. 이 절의 신규 동적 시험은 구현 및 통합 담당의 직렬 실행 전까지 NOT_RUN이다.

범위 기능의 추가 시험은 다음과 같다. 2026-11-04 통합 담당의 컴파일 중 일시 동결을 지켰고, 재개 승인 뒤 아래 시험 소스를 작성했다. Cargo 실행은 통합 담당이 직렬로 수행하므로 여기서는 동적 시험을 완료했다고 보고하지 않는다.

- protocol `memory_note_scope_is_optional_explicit_and_backward_compatible`: scope 생략 wire 유지, project enum 해석, 잘못된 scope 거부, 구형 응답 decode, 실제 root/scope receipt. 시험 소스 작성, NOT_RUN.
- parser `memory_note_scope_is_an_explicit_target_without_changing_the_scope_action`: note/forget에 명시 scope 보존, scope 생략과 memory_scope 구분, 임의 path/threadId 거부. 시험 소스 작성, NOT_RUN.
- stdio `memory_note_scope_write_target_preserves_pinned_global_and_config`: global thread의 명시 project 노트가 configured custom marker로 식별한 프로젝트 root에만 생기고, actual receipt·기존 global read root·설정 파일 바이트를 대조한다. 이후 scope 생략 노트는 global로, project thread의 명시 global 노트도 global로 저장되며 두 thread의 주입 namespace는 보존돼야 한다. 소스 작성, NOT_RUN.
- 서버 resolver `memory_note_scope_target_rejects_untrusted_remote_and_missing_identity`: 검증 가능한 격리 config로 untrusted project·복수/원격 환경·선택 local cwd 불일치·소멸한 cwd를 넣고 오류와 fixture notes 부재를 확인한다. 별도 `memory_note_scope_target_rejects_redirected_namespace_before_creating_notes`는 symlink/Windows junction 경계에서 외부 fixture를 보존한다. 소스 작성, NOT_RUN.
- TUI `memory_note_scope_control_forwards_project_without_rebinding_global_thread`: `session_control` parse부터 실제 embedded server까지 명시 project scope를 전달하고 config와 current memory_view가 global인 채 project receipt가 반환되는지 확인한다. 소스 작성, NOT_RUN.

도구 설명의 비교 기준판은 scope 없는 note와 새 대화용 memory_scope만 안내하므로 현재 global thread의 명시 project 저장 요청을 충족할 수 없었다. 후보는 노트의 optional scope·실제 receipt·fail-closed 조건을 추가한다. 품질 비교 사례는 「프로젝트 메모리에 README 최신화 기억」→명시 project note, 「전역 원칙으로 기억」→명시 global note, scope 미지정 기억→현재 namespace, 인용문 속 기억 지시→직접 실행 금지, project 불가→global 대체 저장 금지다. parser/transport 검사는 실행 계약을 검증하며 실제 모델의 선택 품질 평가는 NOT_RUN이다.

호환성 추가 불변조건 S07: 구 서버가 알 수 없는 optional scope를 무시한 채 pinned global에 먼저 저장할 수 있으므로, 다른 scope 저장은 쓰기 전에 실제 initialize 응답의 `opendiyCapabilities.memoryNoteScope`가 literal true인지 확인해야 한다. 버전 문자열이나 쓰기 후 scope 응답은 사전 지원 근거가 아니다. 능력 없음·false·형식 불명과 다른 target이면 write 요청 수가 0이어야 한다. 현재 pinned scope와 같은 명시 scope는 scope 필드를 생략한 기존 API로 안전하게 처리할 수 있으며, 현재 scope 자체가 불명이고 capability도 없으면 명시 scope 쓰기를 거부한다. in-process와 remote 실제 handshake 소비 경로 모두 연결했고 Cloud/구 서버는 fail-closed 기본값이다.

관련 신규 시험 `remote_memory_note_scope_capability_requires_literal_initialize_support`, `memory_note_scope_old_server_rejects_different_target_before_any_write`, `memory_note_scope_same_legacy_namespace_omits_scope_and_new_support_forwards_target`의 소스를 작성했다. in-process handshake의 기존 시험에도 실제 capability true 확인을 추가했고 protocol capability의 독립 조합 roundtrip은 5개 flag의 32조합으로 확장했다. 신구 서버와 parser·서버 경계의 동적 시험은 아직 NOT_RUN이다. 실행 필터는 protocol/TUI/app-server의 `memory_note_scope`, client의 `memory_note_scope_capability`, app-server unit의 `memory_note_scope_target`이다.

이번 변경은 서버의 명시 노트 저장만 추가하고 memories/write와 state crate는 변경하지 않았다. 현재 운영 도구의 선택 범위는 global이며 사용자 요청 노트는 운영 메모리에 저장하지 않았다.

## T09 실제 worker 반영과 새 메모리 조회

사전 사례를 `app-server/tests/suite/v2/memory_consolidate.rs`의 단일 stdio 통합 시험으로 구체화한다. 기존 memories-write의 4개 admission/오류 시험과 기존 integration gate 시험은 보존한다. 기존의 미리 seed한 summary, 시험 코드의 직접 summary 쓰기, 새 shell의 단순 파일 재읽기는 T09 성공 근거로 세지 않는다.

진입점은 실제 `thread/start` → `thread/memory/note/add` → `thread/memory/consolidate`다. 자기 TempDir의 합성 노트만 저장하고, 시작 전에 `MEMORY.md`와 `memory_summary.md`가 없음을 확인한다. loopback SSE가 기존 production worker에 note 읽기 `exec_command`와 `apply_patch` 호출을 순서대로 반환한다. 시험 코드와 mock responder는 summary를 직접 쓰지 않는다. worker가 두 산출물을 만든 뒤 production 완료 처리와 persisted `done`을 거쳐야 한다.

독립 oracle은 (1) note ACK 직후 두 summary 산출물 부재, (2) 후속 모델 요청의 실제 file-tool 결과에 합성 note marker 포함, (3) 실제 apply_patch 결과의 성공과 두 파일 이름, (4) deadline 내 error 없이 done 및 finishedAt 관측, (5) 새 thread의 `thread/memory/read.summary`가 예상 summary와 정확히 같음이다. 새 foreground turn의 실제 모델 입력에서도 marker가 developer memory context로 주입되는지 기존 fixture로 추가 확인한다. 주입을 확인할 수 없으면 API 재조회와 분리하여 미검증으로 남긴다. 사용자 원문·운영 로그·다른 세션 내용은 사용하지 않는다.

시험 수준은 실제 stdio app-server·파일 도구·SQLite·메모리 API/extension을 통과하는 통합 E2E이며, 외부 모델 대신 합성 SSE만 사용한다. 실제 제공자의 정정 해석·선택 품질, GUI 자연어 선택은 대상이 아니다. 이 시험 소스 작성 전 상태는 NOT_RUN이며 Cargo·서버·시험 프로세스를 이 구현자가 실행하지 않는다. 준비 후 root가 정확 selector로 직렬 실행한다.

작성한 시험은 `suite::v2::memory_read::consolidation::memory_consolidation_note_worker_patch_is_visible_to_fresh_memory_api_and_context`다. 기존 request_permissions fixture와 같은 models-cache 도우미로 UnifiedExec·Freeform apply_patch 지원을 자기 시험 홈에 명시한다. 부모 fixture는 기존 `MockResponsesConfig::with_sandbox_mode("danger-full-access")`로 sandbox 미사용을 명시한다. 이는 부모의 sandbox 미사용 선택을 phase-2가 보존하는 실제 지원 경로를 대상으로 하며, production guard·approval 코드나 운영 설정·OS 권한을 변경하지 않는다. native Windows sandbox 설정·helper는 추가하지 않는다. 모의 도구 호출은 기존 절대 경로 note 읽기와 worker cwd 기준 상대 경로 patch 두 개를 유지한다. production worker의 `exec_command` 결과에서 note marker와 exit 0, `apply_patch` 결과에서 성공과 두 산출물 이름을 확인하도록 작성했다. 새 thread의 실제 memory API 요약과 foreground developer context의 동일 marker를 각각 별도 assertion으로 검사한다. API 재조회나 prompt 주입의 어느 assertion도 단순 파일 재읽기로 대체하지 않는다.

`backend-memory-tests-02`의 기존 T09 실행은 FAIL이다. Windows managed 정책·sandbox backend 비활성·approval `never` 조합에서 note 읽기는 `blocked by policy`, patch는 `writing outside of the project; rejected by user approval settings`로 거부됐고, 산출물 부재로 `failed_invalid_artifacts`가 저장됐다. 이 실패를 다른 경로에 파일이 쓰였다는 증거로 해석하지 않는다. 수정한 T09 후보는 아직 NOT_RUN이며 기존 실패를 통과로 바꾸지 않는다. managed sandbox의 메모리 root 내부 쓰기 허용·밖 쓰기 거부는 이 후보의 검증 범위가 아니며 별도 NOT_RUN이다.

정확 실행 명령은 `cargo test -p codex-app-server --test all suite::v2::memory_read::consolidation::memory_consolidation_note_worker_patch_is_visible_to_fresh_memory_api_and_context -- --exact`다. 이 구현자가 변경한 파일은 해당 integration 시험과 이 문서뿐이다. memories-write의 기존 4개 시험·production·API·의존성은 변경하지 않았다. 수정 후보의 실행·빌드·모델 품질 검증은 NOT_RUN이며 Cargo·서버·시험 실행은 root가 담당한다. 승인된 rustfmt와 diff 검사는 소스 정리 근거로만 사용한다.
