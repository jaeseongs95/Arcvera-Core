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

기존 Rust 테스트 모듈을 재사용했다. 새 runner나 의존성은 추가하지 않았다.

| 사례 | 구현된 테스트 경계 | 실행 상태 |
| --- | --- | --- |
| 01–03 | `startup_orchestration::tests::automatic_daemon_requires_the_full_known_cli_version` | NOT_RUN |
| 04–05,07 | `daemon_startup_tests::local_daemon_version_metadata_limits_only_implicit_attachment` / `daemon_startup_tests::daemon_startup_falls_back_only_for_implicit_endpoints` | NOT_RUN |
| 05–08 | `daemon_startup_tests::default_socket_version_policy_applies_on_initial_and_reconnect` | NOT_RUN |
| 06 | `startup_orchestration::tests::automatic_daemon_rechecks_replaced_server_after_successful_start` | NOT_RUN |
| 09 | `i18n::tests::daemon_selection_mismatch_warning_preserves_paths_and_guidance_in_both_languages` | NOT_RUN |
| 10 | `i18n::tests::daemon_connection_mismatch_preserves_runtime_source_and_explicit_guidance` / `history_cell::tests::local_daemon_version_guidance_uses_selected_ui_language` | NOT_RUN |

UDS fixture는 자기 임시 directory의 private socket과 합성 initialize 응답만 사용한다. shared 서버와 무관한 client transport 종료 뒤에도 같은 listener가 다음 연결을 받으며, `initialized` 외 application RPC는 assertion 실패다. 이 oracle는 lifecycle/thread RPC 무발행을 검증하며 실제 진행 작업의 보존 E2E를 대신하지 않는다.

실행 담당자가 격리된 빌드 사본에서 기존 `cargo test -p codex-tui --lib <해당 필터> -- --test-threads=1`을 직렬 실행하고 결과를 위 사례에 연결한다. 구현자는 Cargo/시험/빌드/운영 명령을 실행하지 않았다. 정적 diff 검사는 동적 시험의 PASS가 아니다.
