<!-- Modified for OpenDIY Codex R38 (2026-11-06); see MODIFICATIONS.md. -->
# R38 공유 서버 선택·버전 정합성 시험 계획

작성 기준: R38 소스 `feat/r38-startup-cache-daemon-20261106`. 구현 전 사례다. 이 문서 작성 시점에는 시험·Cargo·빌드를 실행하지 않았다. 각 결과는 `NOT_RUN`이며 실제 실행 담당자가 대상 commit·명령·종료 코드·원시 근거를 연결해야 한다.

## 요구와 불변조건

- R38-D1: 암시적 로컬 공유 서버 연결은 handshake로 확인한 전체 버전 문자열이 현재 CLI와 같을 때만 사용한다. 공식 SemVer만 같고 패치 build가 다르거나 버전이 없으면 현재 CLI의 embedded 서버로 fallback한다.
- R38-D2: 소켓 사전 탐색 성공과 자동 start의 버전 확인은 실제 최종 연결의 동일성을 보장하지 않는다. 실제 연결 및 재연결 경계에서도 D1을 확인한다.
- R38-D3: 버전 불일치를 해결하기 위해 공유 서버 업데이트·재시작·강제 종료를 자동 수행하지 않는다. 연결 종료는 이 클라이언트가 소유한 연결에 한정한다.
- R38-D4: 명시 remote 연결과 특정 backend의 기존 작업 복원은 암시적 fallback과 구별한다. 복원 대상이 다르다는 이유로 다른 backend에 작업을 생성하지 않는다.
- R38-D5: 안내는 활성화한 backend source와 확인한 버전·경로를 구별해 한국어/영어 catalog로 제공한다. 선택된 package 경로를 실제 실행 중인 서버 경로라고 단정하지 않는다. 명시적 전환 안내는 `codex app-server daemon update --from-cli --yes --no-force`를 유지한다.

CS 적용: 실제 handshake 응답이 사전 probe보다 권위가 높은 관측이다. 초기 탐색과 연결 사이의 교체를 고려하고, 연결 소유권을 보존한다. `DIST-ORDER-004`의 적용 조건인 역순 상태 알림은 이 수정에 해당하지 않으므로 의무로 추가하지 않는다. 버전 문자열은 호환성 협상의 대체물이 아니라 로컬 자동 선택의 정책 조건이다.

## 사례

| 사례 | 요구 | 전제·입력 | 절차 | 기대 결과·독립 oracle | 수준·현재 결과 |
| --- | --- | --- | --- | --- | --- |
| R38-DAEMON-01 | D1 | 자동 start 결과 server=`0.166.0+native-r36.20261104`, CLI=`0.166.0+native-r38.20261106` | 기존 `automatic_daemon_requires_the_full_known_cli_version` 경계에서 full version을 비교 | mismatch 안내에 두 전체 버전이 보존되고 embedded 선택. 단순 SemVer 일치를 거부 | 기존 Rust unit 확장, NOT_RUN |
| R38-DAEMON-02 | D1 | server 버전이 CLI와 정확히 같음 | 같은 경계 호출 | 안내 없음; 기존 정상 shared 선택 보존 | 기존 Rust unit, NOT_RUN |
| R38-DAEMON-03 | D1 | server version=None | 같은 경계 호출 | 버전 미확인을 표시하고 embedded 선택; unknown을 일치로 간주하지 않음 | 기존 Rust unit, NOT_RUN |
| R38-DAEMON-04 | D1,D2,D3 | 자동 start 비활성, 암시적 default socket, fixture의 initialize userAgent가 이전 build | 실제 `app_server_connection::connect`에 fixture socket 전달 | connect가 이 shared client를 반환하지 않음; init 뒤 config/feature/thread 요청 없음; 이 client 연결만 닫힘; 상위 embedded fallback. shared 서버 프로세스와 설치 파일 보존 | 기존 transport fixture를 이용한 integration, NOT_RUN |
| R38-DAEMON-05 | D1,D2 | 암시적 default socket, initialize에 현재 full build 또는 version 미확인 | 04와 같은 경계에서 각 입력 실행 | 현재 build는 연결 유지; 미확인은 연결 거부. oracle은 fixture에 명시한 userAgent와 반환 결과 | integration, NOT_RUN |
| R38-DAEMON-06 | D2,D3 | 자동 start가 현재 build를 반환한 뒤 실제 handshake는 old/missing build; 최초 handshake current 뒤 재연결은 old/missing | production과 같은 `automatic_daemon_target`을 거쳐 보호된 실제 UDS에 연결하고, 기존 socket fixture에서 같은 endpoint로 재연결 | 사전 관측에 의존해 다른 build에 attach하지 않음. 업데이트·restart·shutdown RPC 없음 | integration, NOT_RUN |
| R38-DAEMON-07 | D4 | 명시 `Remote` target의 다른 version 서버 | 실제 connect 호출 | remote의 기존 명시 연결 정책을 보존; 암시적 로컬 fallback 조건을 적용하지 않음 | integration, NOT_RUN |
| R38-DAEMON-08 | D4 | picker가 기존 backend ID용 LocalDaemon target의 fallback=false로 변환 | 연결·복원 경로 추적 및 기존 해당 fixture | 다른 backend에 암묵적으로 새 작업을 만들지 않음; 불일치 처리 정책과 일반 암시적 선택을 구별 | 기존 복원 회귀, NOT_RUN |
| R38-DAEMON-09 | D3,D5 | 선택된 package가 CLI와 다름; 공백·한국어 포함 경로 | 기존 i18n mismatch formatter를 ko/en으로 호출 | 두 실행 파일 경로·현재 embedded source·보존 사실·정확한 no-force 명령 보존. 표시용 경로를 번역하지 않음 | 기존 i18n unit 확장, NOT_RUN |
| R38-DAEMON-10 | D5 | 실제 shared handshake mismatch, CLI/source/socket 경로 제공 | ko/en 안내 출력 | 어느 backend로 실행하는지와 관측 source·두 전체 버전·실제 확인한 경로만 표시. `Use /daemon`만으로 원인을 대신하지 않음 | i18n/기존 notice rendering 회귀, NOT_RUN |

## 기존 경로와 검증 한계

- `startup_orchestration.rs` 자동 start는 이미 `automatic_daemon_version_warning`으로 known full build 일치를 확인한다. stopped old package의 `CliDaemonMismatch`도 기존 embedded fallback 대상이다. 추측에 따른 교체는 하지 않는다.
- `lib.rs::maybe_probe_default_daemon_socket`는 transport probe만 수행하며 버전을 확인하지 않는다. `daemon_startup::compatibility_warning`의 feature check 전에 실제 연결 버전 guard가 필요하다.
- 구현 전 `app_server_connection.rs::connect`는 초기 연결·재연결의 공통 경계에서 mismatch를 tracing 경고만 남겼다. 담당 범위 확장 후 암시적 `LocalDaemon(allow_embedded_fallback=true)`의 실제 handshake를 거부하도록 수정했다. explicit `Remote`/`LocalDaemon(false)`의 연결 정책은 보존했다.
- 자동 start 성공 target은 기존 `false`에서 `true`로 전환했다. 이 target은 사용자 지정 endpoint가 아니므로 동일 handshake 검사를 적용하며, start 이후 소켓이 바뀌면 기존 embedded fallback을 허용한다. 기존 picker의 backend-ID 복원은 여전히 `false`다.
- Desktop launcher의 `--no-daemon`과 Desktop 자체 shared attach는 별도 경로다. 이 TUI 사례의 결과를 Desktop 검증으로 확대하지 않는다.
- 실행 담당자는 기존 Cargo runner, 격리된 `CODEX_HOME`·socket·fixture 서버만 사용한다. 운영 설정·daemon·인증·타 세션 자료는 시험 입력으로 읽지 않는다.

## 구현 연결 및 실행 인계

### D5 실제 표시 경계 보완 계획 (독립감사 후, 제품 수정 전)

기존 06·10의 handshake 거부/formatter 검증만으로 실제 화면 전달을 입증하지 못했다. 초기 `start_app_server`는 mismatch를 버리고 embedded로 전환했으며, `reconnect`는 mismatch를 재시도하다 단계별 timeout으로 바꿨다. 아래 사례를 먼저 추가하고 결과는 실행 전 `NOT_RUN`으로 유지한다.

| 사례 | 전제·입력 | 절차·실제 경계 | 기대 결과·oracle | 상태 |
| --- | --- | --- | --- | --- |
| R38-DAEMON-11 | 자동선택 preflight는 current build, 최종 handshake는 old 또는 version missing; 시작 안내에 기존 경고 있음 | 실제 `start_app_server`의 embedded fallback 결과를 `run_ratatui_app`에서 쓰는 동일 startup-warning 전달 경계로 넘기고 기존 startup history event를 렌더 | active backend가 Embedded이고, 시작 경고에 current CLI executable/source·전체 client/server version 또는 unknown·socket·no-force 명령이 모두 남는다. 기존 경고도 보존. fixture는 initialize/initialized 외 RPC를 거부 | NOT_RUN |
| R38-DAEMON-12 | 진행 대화 ID·편집 중 draft·queue가 있는 implicit LocalDaemon이 old 또는 version missing으로 교체됨 | 실제 `reconnect` 호출 후 production reconnect 실패 표시 경계에서 history event를 렌더 | deadline까지 재시도하지 않고 typed local-version refusal만 반환·표시. 전체 version·안전한 endpoint·CLI source/path·no-force 안내 보존. 기존 ID/draft/queue 유지; resume/start/fork/turn/lifecycle RPC와 타 backend 전환 없음 | NOT_RUN |
| R38-DAEMON-13 | 임의 transport 오류 문자열 또는 credential/query/fragment가 있는 WebSocket endpoint; explicit target 포함 | startup fallback과 reconnect의 같은 오류 전달 경계 호출 | 일반 transport error를 startup UI에 복사하지 않음. reconnect는 기존 stage-only timeout 유지. typed refusal의 endpoint만 기존 sanitizer를 거치며 credential/query/fragment가 표시되지 않음. explicit Remote/LocalDaemon(false)는 기존 정책 보존 | NOT_RUN |

실제 UI 표시를 검사하는 사례 이름에 `daemon_startup` 또는 `startup_orchestration`을 포함해 기존 직렬 시험 필터에서도 새 경계가 실행되도록 한다. 오류 문자열 전체를 안전하다고 가정하지 않고, 생성 위치가 제한된 typed local-version refusal만 UI용 진단으로 취급한다.

기존 Rust 테스트 모듈을 재사용했다. 새 runner나 의존성은 추가하지 않았다.

| 사례 | 구현된 테스트 경계 | 실행 상태 |
| --- | --- | --- |
| 01–03 | `startup_orchestration::tests::automatic_daemon_requires_the_full_known_cli_version` | NOT_RUN |
| 04–05,07 | `daemon_startup_tests::local_daemon_version_metadata_limits_only_implicit_attachment` / `daemon_startup_tests::daemon_startup_falls_back_only_for_implicit_endpoints` | NOT_RUN |
| 05–08 | `daemon_startup_tests::default_socket_version_policy_applies_on_initial_and_reconnect` | NOT_RUN |
| 06 | `startup_orchestration::tests::automatic_daemon_rechecks_replaced_server_after_successful_start` | NOT_RUN |
| 09 | `i18n::tests::daemon_selection_mismatch_warning_preserves_paths_and_guidance_in_both_languages` | NOT_RUN |
| 10 | `i18n::tests::daemon_connection_mismatch_preserves_runtime_source_and_explicit_guidance` / `history_cell::tests::local_daemon_version_guidance_uses_selected_ui_language` | NOT_RUN |
| 11 | `daemon_startup_tests::daemon_startup_falls_back_only_for_implicit_endpoints`가 실제 fallback의 warning out-parameter → production `emit_startup_warning` → `InsertHistoryCell` 렌더를 검사 | NOT_RUN |
| 12 | `app::tests::disconnect_tests::daemon_startup_reconnect_version_refusal_reaches_history_without_restoring_work`가 실제 reconnect → production `finish_failed_reconnect` → history event 렌더와 ID/draft/queue 보존을 검사 | NOT_RUN |
| 13 | 기존 11의 일반 transport 오류 경고 미추가 oracle + `app::tests::disconnect_tests::daemon_startup_reconnect_transport_errors_keep_stage_only_diagnostic`; 12에서 합성 credential/query/fragment 비노출도 검사 | NOT_RUN |

D5 보완 구현은 `LocalVersionMismatch`의 private 진단만 UI로 전달한다. 초기 fallback은 `run_ratatui_app`의 기존 startup warning에 추가하며, `App::run`은 동일 `emit_startup_warning` 경계로 표시한다. 재연결은 typed refusal만 즉시 반환하고 기존 실패 UI 경계를 호출한다. 다른 오류는 기존 재시도·stage-only timeout을 유지한다. archive/picker 호출자는 UI warning을 소비하지 않으므로 기존 동작을 보존하도록 빈 warning sink를 전달한다.

UDS fixture는 자기 임시 directory의 private socket과 합성 initialize 응답만 사용한다. shared 서버와 무관한 client transport 종료 뒤에도 같은 listener가 다음 연결을 받으며, `initialized` 외 application RPC는 assertion 실패다. 이 oracle는 lifecycle/thread RPC 무발행을 검증하며 실제 진행 작업의 보존 E2E를 대신하지 않는다.

실행 담당자가 격리된 빌드 사본에서 기존 `cargo test -p codex-tui --lib <해당 필터> -- --test-threads=1`을 직렬 실행하고 결과를 위 사례에 연결한다. 구현자는 Cargo/시험/빌드/운영 명령을 실행하지 않았다. 정적 diff 검사는 동적 시험의 PASS가 아니다.

2026-11-06 통합 시험 1차: 11개 중 10 PASS, 12번 사례는 reconnect 전에 fixture의 Enter가 operation을 발생시켜 준비 조건에서 FAIL했다. 원시 결과는 보존한다. 실제 진행 중 TurnStarted 상태에서 기존 queue_user_message_with_options 경계로 대기 입력을 준비하고, 준비 전 초기 operation만 비운다. 준비 후·재연결 후 operation 없음과 ID/draft/queue 보존 oracle는 유지한다. 제품 코드·대기 정책은 바꾸지 않는다. 재실행 전 결과는 NOT_RUN이다.

통합 재시험 2차는 다른 테스트 모듈의 private turn helper를 잘못 참조해 E0425 compile 오류(exit101)였다. 실제 시험 0개이며 제품 실패나 통과로 계산하지 않는다. 동일한 protocol Turn 구조체를 fixture에 명시하고 조건·oracle 그대로 3차 실행한다.


## Windows 최종 개발 시험 결과

통합 4차에서 `daemon_startup` 11개, `startup_orchestration` 2개, `i18n::tests::daemon_` 2개, `history_cell::tests::local_daemon` 2개와 기존 backend 소유권 필터 1개가 각각 exit0이다. 01~13 사례의 위에 명시한 실제 fixture·메시지 경계가 통과했다. 초기 fallback history와 실제 reconnect 거부 history, source·version·안전한 endpoint 안내, draft/queue/ID 보존을 포함한다. 최초 fixture 준비 실패와 E0425 컴파일 실패는 보존했다. 공유 운영 프로세스의 강제 전환·실제 진행 작업 전체 E2E는 이 격리 시험의 보장 범위가 아니다.
