# Modified for Arcvera-Core 1.0.1 (2026-11-13); see MODIFICATIONS.md.
# Application-owned CLI help and parse-error presentation. Command tokens and values remain unchanged.

cli-help-app-cmd-workspace-path-to-open-in-the-desktop-app = 기존 작업 공간 인자(Desktop 연동 비활성화)

cli-help-app-cmd-override-the-app-installer-download-url-advanced = 기존 설치 URL 인자(무시됨, 설치 비활성화)

cli-cleanup-current = 현재 대화 UUID. 생략하면 CODEX_THREAD_ID를 사용합니다

cli-cleanup-parent = 정리 범위의 부모 UUID. 생략하면 현재 대화입니다

cli-cleanup-threads = 정리할 정확한 UUID. 반복할 수 있으며 생략하면 미리보기의 모든 종료 후보입니다

cli-cleanup-apply = 미리보기 후 확인을 받아 실제로 정리합니다. 생략하면 읽기 전용입니다

cli-cleanup-confirm = 미리보기의 선택 항목 삭제를 명시적으로 승인합니다

cli-cleanup-about = 현재 부모 아래 로컬 2세대 서브에이전트의 종료 대화를 미리 보고 정리합니다.

cli-cleanup-long-about = 미리보기가 기본이며 각 항목의 reason에 정리·보존 사유를 표시합니다. --apply는 미리보기 뒤 명시 확인을 받습니다. 메모리에 없는 과거 종료 항목은 참조 안전성을 입증할 수 없어 reference_participants_unverified로 보존합니다. 해당 항목의 운영 정리 지원은 미완입니다. 실행 중이거나 상태가 불명확한 항목, 다른 세대·원격 소유·이전 방식 항목은 보존합니다. --thread를 반복하면 정확한 선택 집합만 정리하며 선택하지 않은 후손은 보존합니다.

cli-help-doctor-options-for-building-a-local-codex-diagnostic-report = 로컬 Codex 진단 보고서 생성 옵션

cli-help-doctor-options-for-building-a-local-codex-diagnostic-report-details = 로컬 Codex 진단 보고서 생성 옵션.

    `--network-only`를 선택하지 않으면 전체 진단을 실행합니다. 사람이 읽는 출력에는 기본적으로 상세 진단을 포함하며 --summary는 터미널 출력을 간략하게 표시합니다.

cli-help-doctor-internal-isolated-filesystem-probe-exits-before-loading-configuration = 내부 격리 파일시스템 검사. 설정을 읽기 전에 종료합니다

cli-help-doctor-emit-a-redacted-machine-readable-report = 민감 정보를 가린 기계 판독용 보고서 출력

cli-help-doctor-run-config-network-environment-provider-http-and-responses = 설정, 네트워크 환경, 제공자 HTTP 및 Responses WebSocket 검사 실행

cli-help-doctor-limit-database-integrity-scans-when-collecting-a-feedback = 피드백 첨부 파일 수집 시 데이터베이스 무결성 검사 제한

cli-help-doctor-only-show-grouped-check-rows-and-the-final = 그룹별 검사 행과 최종 집계만 표시

cli-help-doctor-expand-long-lists-in-detailed-human-output = 상세 출력의 긴 목록 펼치기

cli-help-doctor-disable-ansi-color-in-human-output = 사람이 읽는 출력에서 ANSI 색상 사용 안 함

cli-help-doctor-use-ascii-status-labels-and-separators-in-human = 사람이 읽는 출력의 상태 라벨과 구분자로 ASCII 사용

cli-help-doctor-machine-readable-doctor-output-shared-by-human-and = 일반 출력과 JSON 렌더러가 공유하는 기계 판독용 doctor 출력

cli-help-doctor-machine-readable-doctor-output-shared-by-human-and-details = 일반 출력과 JSON 렌더러가 공유하는 기계 판독용 doctor 출력.

    각 검사가 자체 분류, 상태, 상세, 조치 및 소요 시간을 가지는 평면 스키마입니다. 지원 도구는 렌더러의 섹션 구조를 몰라도 개별 행을 필터링하거나 민감 정보를 가릴 수 있습니다.

cli-help-doctor-one-diagnostic-result-in-the-doctor-report = doctor 보고서의 진단 결과 한 항목

cli-help-doctor-one-diagnostic-result-in-the-doctor-report-summaries = doctor 보고서의 진단 결과 한 항목.

    요약은 간략한 일반 출력에 사용할 수 있습니다. 상세에는 로컬 경로나 명령 출력이 포함될 수 있으며 렌더링 또는 JSON 직렬화 전에 민감 정보를 가립니다.

cli-help-doctor-structured-cause-remedy-metadata-for-a-non-ok = 정상이 아닌 doctor 검사에 대한 구조화된 원인·해결 방법

cli-help-doctor-structured-cause-remedy-metadata-for-a-non-ok-details = 정상이 아닌 doctor 검사에 대한 구조화된 원인·해결 방법.

    일반 출력은 문제 항목으로 경고와 실패를 설명합니다. 행 제목은 문제를, 상세 행은 측정값과 기대값을, 해결 방법은 다음 조치를 표시합니다.

cli-help-doctor-json-support-report-emitted-by-codex-doctor-json = `codex doctor --json`이 출력하는 JSON 지원 보고서

cli-help-doctor-json-support-report-emitted-by-codex-doctor-json-details = `codex doctor --json`이 출력하는 JSON 지원 보고서.

    보고서는 검사 ID를 키로 사용하므로 지원 도구는 배열을 탐색하지 않고 `checks["terminal.metadata"]` 같은 경로에 접근할 수 있습니다. 일반 출력은 행을 재정렬하거나 그룹화할 수 있지만 표시 변경과 무관하게 JSON 구조는 유지해야 합니다.

cli-help-doctor-one-redacted-check-in-the-json-support-report = JSON 지원 보고서에서 민감 정보를 가린 검사 한 항목

cli-help-doctor-one-redacted-issue-in-the-json-support-report = JSON 지원 보고서에서 민감 정보를 가린 문제 한 항목

cli-help-doctor-json-detail-value-that-preserves-repeated-detail-keys = 반복된 상세 키를 새 이름 없이 보존하는 JSON 상세 값

cli-help-lib-opaque-json-value-from-codex-sandbox-state-to = `codex/sandbox-state`에서 받아 그대로 적용할 불투명 JSON 값

cli-help-lib-add-a-readable-root-to-the-supplied-sandbox = 제공된 샌드박스 상태에 읽기 가능한 루트 추가. 여러 루트를 지정하려면 반복합니다

cli-help-lib-disable-direct-network-access-in-the-supplied-sandbox = 제공된 샌드박스 상태에서 직접 네트워크 접근 비활성화

cli-help-lib-named-permissions-profile-to-apply-from-the-active = 활성 설정 계층에서 적용할 이름 있는 권한 프로필

cli-help-lib-layer-codex-home-name-config-toml-on-top = 기본 사용자 설정 위에 $ARCVERA_HOME/<name>.config.toml 적용

cli-help-lib-working-directory-used-for-profile-resolution-and-command = 프로필 해석과 명령 실행에 사용할 작업 디렉터리

cli-help-lib-include-managed-requirements-while-resolving-an-explicit-permissions = 명시한 권한 프로필을 해석할 때 관리 요구사항 포함

cli-help-lib-allow-the-sandboxed-command-to-bind-connect-af = 이 경로 아래의 AF_UNIX 소켓에 샌드박스 명령의 바인딩·연결 허용. 상대 경로는 현재 디렉터리 기준이며 여러 경로를 허용하려면 반복합니다

cli-help-lib-while-the-command-runs-capture-macos-sandbox-denials = 명령 실행 중 `log stream`으로 macOS 샌드박스 거부를 수집하고 종료 후 출력

cli-help-lib-full-command-args-to-run-under-seatbelt = seatbelt에서 실행할 전체 명령 인수

cli-help-lib-full-command-args-to-run-under-the-linux = Linux 샌드박스에서 실행할 전체 명령 인수

cli-help-lib-full-command-args-to-run-under-windows-restricted = Windows 제한 토큰 샌드박스에서 실행할 전체 명령 인수

cli-help-main-codex-cli = Codex CLI

cli-help-main-codex-cli-if-no-subcommand-is-specified-options = Codex CLI

    하위 명령을 지정하지 않으면 옵션을 대화형 CLI에 전달합니다.

cli-help-main-browse-all-agent-sessions-on-the-shared-local = 공유 로컬 app-server 데몬의 모든 에이전트 세션 탐색

cli-help-main-internal-forward-a-local-tcp-socket-through-an = 내부용: HTTP/3 CONNECT 프록시를 통해 로컬 TCP 소켓 전달

cli-help-main-run-codex-non-interactively = 비대화형으로 Codex 실행

cli-help-main-run-a-code-review-non-interactively = 비대화형으로 코드 검토 실행

cli-help-main-manage-login = 로그인 관리

cli-help-main-remove-stored-authentication-credentials = 저장된 인증 정보 제거

cli-help-main-manage-external-mcp-servers-for-codex = Codex 외부 MCP 서버 관리

cli-help-main-manage-codex-plugins = Codex 플러그인 관리

cli-help-main-experimental-run-the-app-server-or-related-tooling = [실험적] 앱 서버 또는 관련 도구 실행

cli-help-main-experimental-manage-the-app-server-daemon-with-remote = [실험적] 원격 제어가 활성화된 app-server 데몬 관리

cli-help-main-launch-the-desktop-app-opens-the-app-installer = Arcvera 릴리스 안내(Desktop 실행·설치 비활성화)

cli-help-main-generate-shell-completion-scripts = 셸 자동 완성 스크립트 생성

cli-help-main-update-codex-to-the-latest-version = Codex를 최신 버전으로 업데이트

cli-help-main-diagnose-local-codex-installation-config-auth-and-runtime = 로컬 Codex 설치, 설정, 인증 및 실행 상태 진단

cli-help-main-run-commands-within-a-codex-provided-sandbox = Codex 샌드박스에서 명령 실행

cli-help-main-debugging-tools = 디버깅 도구

cli-help-main-execpolicy-tooling = Execpolicy 도구

cli-help-main-apply-the-latest-diff-produced-by-codex-agent = Codex 에이전트가 생성한 최신 diff를 로컬 작업 트리에 `git apply`로 적용

cli-help-main-resume-a-previous-interactive-session-picker-by-default = 이전 대화형 세션 이어가기(기본은 선택기, 최신 세션은 --last 사용)

cli-help-main-queue-a-message-for-an-existing-session = 기존 세션의 대기열에 메시지 추가

cli-cleanup-command = 현재 부모 아래 종료된 서브에이전트를 미리 보거나 확인 후 정리합니다

cli-help-main-archive-a-saved-session-by-id-or-session = ID 또는 세션 이름으로 저장된 세션 보관

cli-help-main-permanently-delete-a-saved-session-by-id-or = ID 또는 세션 이름으로 저장된 세션 영구 삭제

cli-help-main-inspect-or-migrate-legacy-local-sessions-to-paginated = 이전 로컬 세션을 검사하거나 페이지 단위 대화 이력으로 마이그레이션

cli-help-main-experimental-migrate-configured-sqlite-databases-to-delete-mode = [실험적] 설정된 SQLite 데이터베이스를 제자리에서 DELETE 모드로 마이그레이션

cli-help-main-unarchive-a-saved-session-by-id-or-session = ID 또는 세션 이름으로 저장된 세션의 보관 해제

cli-help-main-fork-a-previous-interactive-session-picker-by-default = 이전 대화형 세션 분기(기본은 선택기, 최신 세션은 --last 사용)

cli-help-main-experimental-browse-tasks-from-codex-cloud-and-apply = [실험적] Codex Cloud 작업 탐색 및 로컬 변경 적용

cli-help-main-internal-run-the-responses-api-proxy = 내부용: Responses API 프록시 실행

cli-help-main-internal-relay-stdio-to-a-unix-domain-socket = 내부용: 표준 입출력을 Unix 도메인 소켓으로 중계

cli-help-main-experimental-run-the-standalone-exec-server-service = [실험적] 독립 exec-server 서비스 실행

cli-help-main-inspect-feature-flags = 기능 플래그 확인

cli-help-main-shell-to-generate-completions-for = 자동 완성 스크립트를 생성할 셸

cli-help-main-render-the-raw-model-catalog-as-json = 원본 모델 카탈로그를 JSON으로 출력

cli-help-main-tooling-helps-debug-the-app-server = 앱 서버 디버깅 도구

cli-help-main-render-the-model-visible-prompt-input-list-as = 모델에 표시되는 프롬프트 입력 목록을 JSON으로 출력

cli-help-main-replay-a-rollout-trace-bundle-and-write-reduced = rollout 추적 묶음을 재생하고 축약 상태 JSON 저장

cli-help-main-internal-reset-local-memory-state-for-a-fresh = 내부용: 새로 시작하도록 로컬 메모리 상태 초기화

cli-help-main-optional-user-prompt-to-append-after-session-context = 세션 문맥 뒤에 추가할 사용자 프롬프트(선택)

cli-help-main-optional-image-s-to-attach-to-the-user = 사용자 프롬프트에 첨부할 이미지(선택)

cli-help-main-skip-refresh-and-dump-only-the-bundled-catalog = 새로 고침 없이 이 실행 파일에 포함된 카탈로그만 출력

cli-help-main-error-out-when-config-toml-contains-fields-that = config.toml에 이 Codex 버전이 인식하지 못하는 필드가 있으면 오류로 종료

cli-help-main-trace-bundle-directory-containing-manifest-json-and-trace = manifest.json과 trace.jsonl이 있는 추적 묶음 디렉터리

cli-help-main-output-path-for-reduced-rollouttrace-json-defaults-to = 축약 RolloutTrace JSON 출력 경로. 기본값은 TRACE_BUNDLE/state.json입니다

cli-help-main-the-agents-overview-requires-a-shared-server-this = 에이전트 개요에는 공유 서버가 필요하므로 이 옵션을 사용할 수 없습니다

cli-help-main-use-this-directory-for-new-tasks-on-a = 원격 서버의 새 작업에 사용할 디렉터리

cli-help-main-disable-alternate-screen-mode = 대체 화면 모드 사용 안 함

cli-help-main-session-id-uuid-or-session-name-uuids-take = 세션 ID(UUID) 또는 세션 이름. UUID로 해석되면 우선합니다. 생략하면 --last로 최근 기록된 세션을 선택합니다

cli-help-main-continue-the-most-recent-session-without-showing-the = 선택기를 표시하지 않고 최신 세션 이어가기

cli-help-main-show-all-sessions-disables-cwd-filtering-and-shows = 모든 세션 표시(cwd 필터 비활성화 및 CWD 열 표시)

cli-help-main-include-non-interactive-sessions-in-the-resume-picker = 이어가기 선택기와 --last 선택에 비대화형 세션 포함

cli-help-main-session-id-uuid-or-session-name-uuids-take-details = 세션 ID(UUID) 또는 세션 이름. UUID로 해석되면 우선합니다

cli-help-main-delete-without-prompting-session-must-be-a-uuid = 확인 없이 삭제. SESSION은 UUID여야 합니다

cli-help-main-conversation-session-id-uuid-when-provided-forks-this = 대화·세션 ID(UUID). 지정하면 해당 세션을 분기합니다. 생략하면 --last로 최근 기록된 세션을 선택합니다

cli-help-main-fork-the-most-recent-session-without-showing-the = 선택기를 표시하지 않고 최신 세션 분기

cli-help-main-tui-arguments-for-session-commands-where-a-parsed = 파싱된 프롬프트가 명시적 세션 ID를 뜻하는 세션 명령용 TUI 인수

cli-help-main-tui-arguments-for-session-commands-where-a-parsed-details = 파싱된 프롬프트가 명시적 세션 ID를 뜻하는 세션 명령용 TUI 인수.

    `--last PROMPT`는 허용하고 `--last SESSION_ID PROMPT`는 거부합니다.

cli-help-main-full-command-args-to-run-under-the-host = 호스트 샌드박스에서 실행할 전체 명령 인수

cli-help-main-check-execpolicy-files-against-a-command = 명령에 대해 execpolicy 파일 검사

cli-help-main-experimental-use-custom-oauth-issuer-base-url-advanced = 실험적: 사용자 지정 OAuth 발급자 기본 URL 사용(고급). OAuth 발급자 기본 URL 재정의(고급)

cli-help-main-experimental-use-custom-oauth-client-id-advanced = 실험적: 사용자 지정 OAuth 클라이언트 ID 사용(고급)

cli-help-main-show-login-status = 로그인 상태 표시

cli-help-main-load-frozen-cloud-skills-from-a-json-snapshot = 실시간 스킬 서비스 대신 JSON 스냅샷에서 고정된 클라우드 스킬 읽기

cli-help-main-omit-to-run-the-app-server-specify-a = 생략하면 앱 서버 실행. 도구를 사용하려면 하위 명령을 지정합니다

cli-help-main-transport-endpoint-url-supported-values-stdio-default-unix = 전송 엔드포인트 URL. 지원 값: `stdio://`(기본값), `unix://`, `unix://PATH`, `ws://IP:PORT`, `off`

cli-help-main-use-stdio-as-the-transport-equivalent-to-listen = 표준 입출력을 전송 수단으로 사용(`--listen stdio://`와 동일)

cli-help-main-enable-remote-control-for-this-app-server-process = 저장 설정을 바꾸지 않고 이 app-server 프로세스의 원격 제어 활성화

cli-help-main-start-with-remote-control-disabled-preserving-saved-registrations = 저장된 등록과 설정을 유지하며 원격 제어를 끈 상태로 시작합니다

cli-help-main-save-loaded-threads-during-managed-daemon-shutdown = 관리 데몬 종료 시 로드된 대화 저장

cli-help-main-controls-whether-analytics-are-enabled-by-default = 분석 정보 수집의 기본 활성화 여부 제어

cli-help-main-controls-whether-analytics-are-enabled-by-default-analytics = 분석 정보 수집의 기본 활성화 여부를 제어합니다.

    app-server는 기본적으로 분석 정보 수집이 비활성화됩니다. 사용자는 config.toml의 `analytics` 섹션에서 명시적으로 동의해야 합니다.

    VSCode IDE 확장 같은 자사 제품 사용 시에는 이 플래그로 분석 정보 수집을 기본 활성화합니다. 사용자는 config.toml에 다음을 설정하여 거부할 수 있습니다:

    ```toml [analytics] enabled = false ```

    자세한 내용: https://developers.openai.com/codex/config-advanced/#metrics

cli-help-main-manage-the-local-app-server-daemon = 로컬 app-server 데몬 관리

cli-help-main-proxy-stdio-bytes-to-the-running-app-server = 표준 입출력 바이트를 실행 중인 app-server 제어 소켓으로 전달

cli-help-main-experimental-generate-typescript-bindings-for-the-app-server = [실험적] 앱 서버 프로토콜의 TypeScript 바인딩 생성

cli-help-main-experimental-generate-json-schema-for-the-app-server = [실험적] 앱 서버 프로토콜의 JSON Schema 생성

cli-help-main-internal-generate-internal-json-schema-artifacts-for-codex = [내부용] Codex 도구의 내부 JSON Schema 산출물 생성

cli-help-main-install-durable-local-app-server-management-for-ssh = SSH를 통한 사용을 위해 지속 가능한 로컬 app-server 관리 설치

cli-help-main-start-the-local-app-server-daemon-if-it = 실행 중이 아니면 로컬 앱 서버 데몬 시작

cli-help-main-use-only-the-selected-installation-preserve-settings-and = 선택한 설치만 사용하고 설정과 업데이터 상태 보존

cli-help-main-restart-the-local-app-server-daemon = 로컬 앱 서버 데몬 재시작

cli-help-main-update-the-daemon-package-may-interrupt-running-work = 데몬 패키지 업데이트(진행 중인 작업을 중단할 수 있음)

cli-help-main-copy-and-pin-this-cli-package = 이 CLI 패키지 복사 및 고정

cli-help-main-confirm-replacing-the-daemon-package-without-an-interactive = 대화형 확인 없이 데몬 패키지 교체 승인

cli-help-main-never-force-shutdown-on-an-unconfirmed-stop-keep = 강제 종료하지 않음. 중지가 확인되지 않으면 선택한 패키지를 유지하고 자동 재시도하지 않습니다

cli-help-main-enable-remote-control-for-future-starts-and-a = 이후 시작과 현재 실행 중인 관리 데몬의 원격 제어 활성화

cli-help-main-disable-remote-control-for-future-starts-and-a = 이후 시작과 현재 실행 중인 관리 데몬의 원격 제어 비활성화

cli-help-main-stop-the-local-app-server-daemon = 로컬 앱 서버 데몬 중지

cli-help-main-print-local-cli-and-running-app-server-versions = 로컬 CLI 및 실행 중인 app-server 버전을 JSON으로 출력

cli-help-main-observe-the-selected-installation-and-daemon-without-starting = 시작하거나 복구하지 않고 선택한 설치와 데몬 상태 확인

cli-help-main-internal-run-the-detached-pid-backed-standalone-updater = [내부용] pid로 추적되는 분리된 독립 업데이터 루프 실행

cli-help-main-check-support-for-daemon-owned-packages-without-starting = 업데이터를 시작하지 않고 데몬 소유 패키지 지원 확인

cli-help-main-authorize-one-production-restoration-of-this-selected-release = 선택한 릴리스의 운영 환경 복원을 한 번 승인

cli-help-main-path-to-the-app-server-unix-domain-socket = 연결할 app-server Unix 도메인 소켓 경로

cli-help-main-launch-the-managed-app-server-with-remote-control = 원격 제어가 활성화된 관리 app-server 실행

cli-help-main-output-directory-where-ts-files-will-be-written = .ts 파일을 저장할 출력 디렉터리

cli-help-main-optional-path-to-the-prettier-executable-to-format = 생성 파일을 포맷할 Prettier 실행 파일 경로(선택)

cli-help-main-include-experimental-methods-and-fields-in-the-generated = 생성 결과에 실험적 메서드와 필드 포함

cli-help-main-output-directory-where-the-schema-bundle-will-be = 스키마 묶음을 저장할 출력 디렉터리

cli-help-main-output-directory-where-internal-json-schema-artifacts-will = 내부 JSON Schema 산출물을 저장할 출력 디렉터리

cli-help-main-path-to-the-unix-domain-socket-to-connect = 연결할 Unix 도메인 소켓 경로

cli-help-main-check-for-an-opendiy-package-update-without-applying = 적용 없이 OpenDIY 패키지 업데이트 확인

cli-help-main-show-the-selected-opendiy-package-and-automatic-update = 선택한 OpenDIY 패키지 및 자동 업데이트 상태 표시

cli-help-main-enable-automatic-opendiy-package-updates-every-60-minutes = OpenDIY 패키지 자동 업데이트 활성화(기본 60분 간격)

cli-help-main-disable-automatic-opendiy-package-updates = OpenDIY 패키지 자동 업데이트 비활성화

cli-help-main-automatic-update-interval-in-minutes-opendiy-only-requires = 자동 업데이트 간격(분, OpenDIY 전용, --enable-updates 필요)

cli-help-main-enable-a-feature-repeatable-equivalent-to-c-features = 기능 활성화(반복 가능). `-c features.<name>=true`와 동일

cli-help-main-disable-a-feature-repeatable-equivalent-to-c-features = 기능 비활성화(반복 가능). `-c features.<name>=false`와 동일

cli-help-main-connect-the-tui-to-a-remote-app-server = TUI를 원격 앱 서버 엔드포인트에 연결

cli-help-main-connect-the-tui-to-a-remote-app-server-details = TUI를 원격 앱 서버 엔드포인트에 연결합니다.

    허용 형식: `ws://host:port`, `wss://host:port`, `unix://`, `unix://PATH`.

cli-help-main-name-of-the-environment-variable-containing-the-bearer = 원격 앱 서버 WebSocket에 보낼 bearer 토큰이 있는 환경 변수 이름

cli-help-main-list-known-features-with-their-stage-and-effective = 알려진 기능의 단계 및 적용 상태 목록 표시

cli-help-main-enable-a-feature-in-config-toml = config.toml에서 기능 활성화

cli-help-main-disable-a-feature-in-config-toml = config.toml에서 기능 비활성화

cli-help-main-feature-key-to-update-for-example-unified-exec = 업데이트할 기능 키(예: unified_exec)

cli-help-main-read-the-api-key-from-stdin-e-g = 표준 입력에서 API 키 읽기(예: `printenv OPENAI_API_KEY | codex login --with-api-key`)

cli-help-main-read-the-access-token-from-stdin-e-g = 표준 입력에서 액세스 토큰 읽기(예: `printenv CODEX_ACCESS_TOKEN | codex login --with-access-token`)

cli-help-main-deprecated-previously-accepted-the-api-key-directly-now = (지원 중단 예정) 이전에는 API 키를 직접 받았으나 이제 --with-api-key 사용 안내 후 종료합니다

cli-help-marketplace-cmd-add-a-local-or-git-marketplace-to-the = 설정된 마켓플레이스 소스에 로컬 또는 Git 마켓플레이스 추가

cli-help-marketplace-cmd-list-plugin-marketplaces-codex-is-currently-considering-and = Codex가 현재 고려하는 플러그인 마켓플레이스 및 루트 목록 표시

cli-help-marketplace-cmd-refresh-configured-git-marketplace-snapshots = 설정된 Git 마켓플레이스 스냅샷 새로 고침

cli-help-marketplace-cmd-refresh-configured-git-marketplace-snapshots-omit-marketplace-name = 설정된 Git 마켓플레이스 스냅샷을 새로 고칩니다.

    MARKETPLACE_NAME을 생략하면 설정된 모든 Git 마켓플레이스를 업그레이드합니다.

cli-help-marketplace-cmd-remove-a-configured-marketplace-source-by-name = 설정된 마켓플레이스 소스를 이름으로 제거

cli-help-marketplace-cmd-marketplace-source-a-local-path-owner-repo-ref = 마켓플레이스 소스: 로컬 경로, owner/repo[@ref], HTTPS Git URL 또는 SSH Git URL

cli-help-marketplace-cmd-git-ref-to-fetch-for-git-marketplace-sources = Git 마켓플레이스 소스에서 가져올 Git ref

cli-help-marketplace-cmd-sparse-checkout-path-for-git-marketplace-sources-can = Git 마켓플레이스 소스의 sparse checkout 경로. 반복할 수 있습니다

cli-help-marketplace-cmd-output-add-result-as-json = 추가 결과를 JSON으로 출력

cli-help-marketplace-cmd-output-marketplace-list-as-json = 마켓플레이스 목록을 JSON으로 출력

cli-help-marketplace-cmd-optional-configured-marketplace-name-to-upgrade-omit-to = 업그레이드할 설정된 마켓플레이스 이름(선택). 생략하면 모든 Git 마켓플레이스를 업그레이드합니다

cli-help-marketplace-cmd-output-upgrade-result-as-json = 업그레이드 결과를 JSON으로 출력

cli-help-marketplace-cmd-configured-marketplace-name-to-remove = 제거할 설정된 마켓플레이스 이름

cli-help-marketplace-cmd-output-remove-result-as-json = 제거 결과를 JSON으로 출력

cli-help-marketplace-cmd-examples-codex-plugin-marketplace-add-path-to-marketplace = 예:
      codex plugin marketplace add ./path/to/marketplace
      codex plugin marketplace add owner/repo --ref main
      codex plugin marketplace add https://github.com/owner/repo --sparse plugins/foo

cli-help-marketplace-cmd-examples-codex-plugin-marketplace-upgrade-codex-plugin-marketplace = 예:
      codex plugin marketplace upgrade
      codex plugin marketplace upgrade debug

cli-help-marketplace-cmd-example-codex-plugin-marketplace-remove-debug = 예:
      codex plugin marketplace remove debug

cli-help-mcp-cmd-subcommands-list-list-configured-servers-with-json-get = 하위 명령: - `list` — 설정된 서버 목록(`--json` 지원) - `get` — 서버 하나 표시(`--json` 지원) - `add` — `~/.arcvera/config.toml`에 서버 실행 항목 추가 - `remove` — 서버 항목 삭제 - `login` — OAuth로 MCP 서버 인증 - `logout` — MCP 서버 OAuth 인증 정보 제거

cli-help-mcp-cmd-output-the-configured-servers-as-json = 설정된 서버를 JSON으로 출력

cli-help-mcp-cmd-name-of-the-mcp-server-to-display = 표시할 MCP 서버 이름

cli-help-mcp-cmd-output-the-server-configuration-as-json = 서버 설정을 JSON으로 출력

cli-help-mcp-cmd-name-for-the-mcp-server-configuration = MCP 서버 설정 이름

cli-help-mcp-cmd-command-to-launch-the-mcp-server-use-url = MCP 서버를 실행할 명령. 스트리밍 HTTP 서버는 --url을 사용합니다

cli-help-mcp-cmd-environment-variables-to-set-when-launching-the-server = 서버 실행 시 설정할 환경 변수. 표준 입출력 서버에서만 유효합니다

cli-help-mcp-cmd-url-for-a-streamable-http-mcp-server = 스트리밍 HTTP MCP 서버 URL

cli-help-mcp-cmd-optional-environment-variable-to-read-for-a-bearer = bearer 토큰을 읽을 환경 변수(선택). 스트리밍 HTTP 서버에서만 유효합니다

cli-help-mcp-cmd-optional-oauth-client-identifier-to-use-for-this = 이 MCP 서버에 사용할 OAuth 클라이언트 식별자(선택)

cli-help-mcp-cmd-optional-oauth-client-secret-for-the-pre-registered = 사전 등록된 클라이언트의 OAuth 클라이언트 비밀 값(선택)

cli-help-mcp-cmd-oauth-client-registration-strategy-for-the-immediate-login = 즉시 로그인에만 적용할 OAuth 클라이언트 등록 전략

cli-help-mcp-cmd-optional-oauth-resource-parameter-to-include-during-mcp = MCP 로그인에 포함할 OAuth resource 매개변수(선택)

cli-help-mcp-cmd-name-of-the-mcp-server-configuration-to-remove = 제거할 MCP 서버 설정 이름

cli-help-mcp-cmd-name-of-the-mcp-server-to-authenticate-with = OAuth로 인증할 MCP 서버 이름

cli-help-mcp-cmd-print-the-authorization-url-and-accept-the-callback = 브라우저를 열지 않고 인증 URL을 출력하고 콜백 URL 입력받기

cli-help-mcp-cmd-comma-separated-list-of-oauth-scopes-to-request = 요청할 OAuth scope의 쉼표 구분 목록

cli-help-mcp-cmd-oauth-client-registration-strategy-for-this-login-only = 이번 로그인에만 적용할 OAuth 클라이언트 등록 전략

cli-help-mcp-cmd-name-of-the-mcp-server-to-deauthenticate = 인증을 해제할 MCP 서버 이름

cli-help-mcp-login-retry-provider-rejected-discovered-scopes-once-retaining-the = 선택한 입력 모드를 유지하며 제공자가 거부한 검색 scope로 한 번 재시도

cli-help-migrate-nfs-db-use-a-local-directory-with-space-for-the = 데이터베이스와 WAL 파일을 담을 여유 공간이 있는 로컬 디렉터리 사용. 기본값은 시스템 임시 디렉터리입니다

cli-help-migrate-nfs-db-stop-sqlite-clients-on-every-host-and-keep = 모든 호스트의 SQLite 클라이언트를 중지하고 전환이 끝날 때까지 유지하세요.
    NFS 마운트는 클라이언트 간 POSIX 잠금을 지원해야 합니다(nolock/local_lock=all 금지).
    이 명령은 설정된 데이터베이스를 제자리에서 교체하고 WAL 백업을 보존합니다.

cli-help-migrate-rollouts-publish-the-migration-without-this-flag-the-command = 마이그레이션 적용. 이 플래그가 없으면 대상 세션만 보고합니다

cli-help-migrate-rollouts-restrict-inspection-or-migration-to-one-or-more = 검사 또는 마이그레이션을 하나 이상의 대화 ID로 제한

cli-help-migrate-rollouts-limit-aggregate-rollout-read-and-write-throughput-in = rollout 전체 읽기·쓰기 처리량 제한(초당 MiB)

cli-help-migrate-rollouts-emit-the-complete-per-thread-report-as-json = 대화별 전체 보고서를 JSON으로 출력

cli-help-migrate-rollouts-print-one-line-for-every-inspected-rollout = 검사한 rollout마다 한 줄 출력

cli-help-opendiy-update-actions-supported-by-the-packaged-updater = 패키지 업데이터가 지원하는 동작

cli-help-plugin-cmd-install-a-plugin-from-a-configured-or-remote = 설정된 또는 원격 마켓플레이스에서 플러그인 설치

cli-help-plugin-cmd-install-a-plugin-from-a-configured-or-remote-details = 설정된 또는 원격 마켓플레이스에서 플러그인을 설치합니다.

    `PLUGIN@MARKETPLACE` 또는 `PLUGIN`과 `--marketplace MARKETPLACE`를 지정하세요.

cli-help-plugin-cmd-list-plugins-available-from-configured-and-remote-marketplaces = 설정된 및 원격 마켓플레이스에서 사용 가능한 플러그인 목록 표시

cli-help-plugin-cmd-add-list-upgrade-or-remove-configured-plugin-marketplaces = 설정된 플러그인 마켓플레이스 추가·목록·업그레이드·제거

cli-help-plugin-cmd-uninstall-a-plugin-and-remove-its-local-cache = 플러그인을 제거하고 로컬 캐시 삭제

cli-help-plugin-cmd-uninstall-a-plugin-and-remove-its-local-cache-details = 플러그인을 제거하고 로컬 캐시를 삭제합니다.

    `PLUGIN@MARKETPLACE` 또는 `PLUGIN`과 `--marketplace MARKETPLACE`를 지정하세요.

cli-help-plugin-cmd-plugin-selector-to-install-either-plugin-marketplace-or = 설치할 플러그인 선택자: PLUGIN@MARKETPLACE 또는 PLUGIN과 --marketplace

cli-help-plugin-cmd-marketplace-name-to-use-when-plugin-does-not = PLUGIN에 @MARKETPLACE가 없을 때 사용할 마켓플레이스 이름

cli-help-plugin-cmd-output-install-result-as-json = 설치 결과를 JSON으로 출력

cli-help-plugin-cmd-only-list-plugins-from-this-marketplace-name = 이 마켓플레이스 이름의 플러그인만 표시

cli-help-plugin-cmd-output-plugin-list-as-json = 플러그인 목록을 JSON으로 출력

cli-help-plugin-cmd-include-uninstalled-marketplace-plugins-in-the-json-output = JSON 출력에 설치되지 않은 마켓플레이스 플러그인 포함

cli-help-plugin-cmd-plugin-selector-to-remove-either-plugin-marketplace-or = 제거할 플러그인 선택자: PLUGIN@MARKETPLACE 또는 PLUGIN과 --marketplace

cli-help-plugin-cmd-examples-codex-plugin-add-sample-debug-codex-plugin = 예:
      codex plugin add sample@debug
      codex plugin add sample --marketplace debug

cli-help-plugin-cmd-examples-codex-plugin-list-codex-plugin-list-marketplace = 예:
      codex plugin list
      codex plugin list --marketplace debug
      codex plugin list --json
      codex plugin list --available --json

cli-help-plugin-cmd-examples-codex-plugin-remove-sample-debug-codex-plugin = 예:
      codex plugin remove sample@debug
      codex plugin remove sample --marketplace debug

cli-help-queue-cmd-session-uuid-or-exact-session-name = 세션 UUID 또는 정확한 세션 이름

cli-help-queue-cmd-message-text-to-queue = 대기열에 추가할 메시지 내용

cli-help-queue-cmd-after-queueing-try-to-resume-the-thread-on = 대기열에 추가한 뒤 로컬 데몬 또는 선택한 --remote 서버에서 대화를 이어가도록 시도합니다. 실패하면 경고를 표시하고 명령은 성공으로 종료합니다

cli-help-remote-control-cmd-emit-machine-readable-json = 기계가 읽을 수 있는 JSON 출력

cli-help-remote-control-cmd-run-an-embedded-app-server-in-the-foreground = Ctrl-C를 누를 때까지 내장 app-server를 전경에서 실행

cli-help-remote-control-cmd-start-the-app-server-daemon-with-remote-control = 원격 제어가 활성화된 app-server 데몬 시작

cli-help-remote-control-cmd-stop-the-app-server-daemon = app-server 데몬 중지

cli-help-remote-control-cmd-create-and-print-a-short-lived-manual-pairing = 유효 시간이 짧은 수동 페어링 코드 생성 및 출력

cli-help-remote-control-cmd-read-relay-status-from-the-existing-shared-daemon = 기존 공유 daemon을 새로 시작하지 않고 relay 상태를 조회합니다

cli-help-sandbox-setup-set-up-the-elevated-windows-sandbox = 권한을 높인 Windows 샌드박스 설정

cli-help-sandbox-setup-windows-user-that-will-run-codex-after-managed = 관리 배포 후 Codex를 실행할 Windows 사용자

cli-help-sandbox-setup-use-the-current-windows-user-as-the-codex = 현재 Windows 사용자를 Codex 사용자로 사용

cli-help-sandbox-setup-codex-home-for-the-codex-user-required-with = Codex 사용자의 ARCVERA_HOME. --user와 함께 사용해야 합니다

cli-help-sandbox-uninstall-remove-the-legacy-windows-sandbox-s-machine-wide = 이전 Windows 샌드박스의 컴퓨터 전체 계정 및 네트워크 규칙 제거

cli-help-sandbox-uninstall-run-as-administrator-after-stopping-apps-using-the = 이전 샌드박스와 설치된 프로비저닝 서비스를 사용하는 앱을 중지한 뒤 관리자 권한으로 실행하세요.
    샌드박스 디렉터리와 파일시스템 권한을 포함하여 Codex 홈을 보존합니다.

cli-desktop-cron-control = 인증된 로컬 통로로 Desktop이 소유한 예약 제어를 켭니다

cli-help-cli-internal-launching-cli-that-handles-daemon-updates-after = 내부용: TUI 종료 후 데몬 업데이트를 처리하는 시작 CLI

cli-help-cli-optional-user-prompt-to-start-the-session = 세션을 시작할 사용자 프롬프트

cli-help-cli-internal-resume-a-specific-recorded-session-by-id = 내부용: ID(UUID)로 특정 기록된 세션 이어가기. 최상위 `codex resume <SESSION_ID>`가 설정하며 공개 플래그가 아닙니다

cli-help-cli-internal-show-all-sessions-disables-cwd-filtering-and = 내부용: 모든 세션 표시(cwd 필터 비활성화 및 CWD 열 표시)

cli-help-cli-internal-include-non-interactive-sessions-in-resume-listings = 내부용: 이어가기 목록에 비대화형 세션 포함

cli-help-cli-internal-open-the-daemon-wide-agents-overview-instead = 내부용: 대화를 시작하는 대신 데몬 전체 에이전트 개요 열기

cli-help-cli-internal-the-launching-cli-verified-that-the-local = 내부용: 시작 CLI가 로컬 데몬의 관리 백엔드를 확인함

cli-help-cli-internal-fork-a-specific-recorded-session-by-id = 내부용: ID(UUID)로 특정 기록된 세션 분기. 최상위 `codex fork <SESSION_ID>`가 설정하며 공개 플래그가 아닙니다

cli-help-cli-configure-when-the-model-requires-human-approval-before = 명령 실행 전에 모델이 사용자 승인을 요청할 조건 설정

cli-help-cli-enable-live-web-search-when-enabled-the-native = 실시간 웹 검색 활성화. 모델이 기본 Responses `web_search` 도구를 사용할 수 있습니다(호출별 승인 없음)

cli-help-cli-disable-alternate-screen-mode-runs-the-tui-in = 대체 화면 모드 사용 안 함

    터미널 스크롤 이력을 보존하는 인라인 모드로 TUI를 실행합니다.

cli-help-cli-run-without-the-shared-background-server-even-if = 공유 백그라운드 서버가 실행 중이어도 연결하지 않고 실행

cli-help-cli-action-to-perform-if-omitted-runs-a-new = 실행할 동작. 생략하면 새 비대화형 세션을 실행합니다

cli-help-cli-source-classification-for-newly-created-or-forked-threads = 새로 만들거나 분기한 대화의 출처 분류

cli-help-cli-request-an-experimental-cyber-access-program-for-this = 이번 응답의 실험적 Cyber 접근 프로그램 요청(OpenAI 제공자 전용). 생략하면 서버 기본값을 사용합니다. review는 지원하지 않으며 fork에는 프롬프트가 필요합니다

cli-help-cli-allow-running-codex-outside-a-git-repository = Git 저장소 밖에서 Codex 실행 허용

cli-help-cli-run-without-persisting-session-files-to-disk = 세션 파일을 디스크에 저장하지 않고 실행

cli-help-cli-do-not-load-codex-home-config-toml-auth = `$ARCVERA_HOME/config.toml`을 읽지 않음. 인증에는 계속 `ARCVERA_HOME`을 사용합니다

cli-help-cli-do-not-load-user-or-project-execpolicy-rules = 사용자 또는 프로젝트의 execpolicy `.rules` 파일을 읽지 않음

cli-help-cli-path-to-a-json-schema-file-describing-the = 모델 최종 응답 형식을 정의한 JSON Schema 파일 경로

cli-help-cli-specifies-color-settings-for-use-in-the-output = 출력 색상 설정 지정

cli-help-cli-print-events-to-stdout-as-jsonl = 이벤트를 표준 출력에 JSONL로 출력

cli-help-cli-specifies-file-where-the-last-message-from-the = 에이전트의 마지막 메시지를 저장할 파일 지정

cli-help-cli-initial-instructions-for-the-agent-if-not-provided = 에이전트의 초기 지시. 인수로 지정하지 않거나 `-`를 사용하면 표준 입력에서 읽습니다. 표준 입력이 파이프이고 프롬프트도 있으면 표준 입력을 `<stdin>` 블록으로 추가합니다

cli-help-cli-resume-a-previous-session-by-id-or-pick = ID로 이전 세션을 이어가거나 --last로 최신 세션 선택

cli-help-cli-fork-a-previous-session-by-id-into-a = ID로 이전 세션을 새 세션으로 분기

cli-help-cli-run-a-code-review-against-the-current-repository = 현재 저장소에 대한 코드 검토 실행

cli-help-cli-conversation-session-id-uuid-or-thread-name-to = 분기할 대화·세션 ID(UUID) 또는 대화 이름

cli-help-cli-optional-image-s-to-attach-to-the-prompt = 분기 후 보낼 프롬프트에 첨부할 이미지(선택)

cli-help-cli-optional-prompt-to-send-after-forking-if-is = 분기 후 보낼 프롬프트(선택). `-`를 사용하면 표준 입력에서 읽습니다

cli-help-cli-conversation-session-id-uuid-or-thread-name-uuids = 대화·세션 ID(UUID) 또는 대화 이름. UUID로 해석되면 우선합니다. 생략하면 --last로 최근 기록된 세션을 선택합니다

cli-help-cli-resume-the-most-recent-recorded-session-newest-without = ID 지정 없이 최근 기록된 최신 세션 이어가기

cli-help-cli-show-all-sessions-disables-cwd-filtering = 모든 세션 표시(cwd 필터 비활성화)

cli-help-cli-optional-image-s-to-attach-to-the-prompt-details = 이어가기 후 보낼 프롬프트에 첨부할 이미지(선택)

cli-help-cli-prompt-to-send-after-resuming-the-session-if = 세션을 이어간 후 보낼 프롬프트. `-`를 사용하면 표준 입력에서 읽습니다

cli-help-cli-review-staged-unstaged-and-untracked-changes = 스테이징·미스테이징·추적하지 않는 변경 검토

cli-help-cli-review-changes-against-the-given-base-branch = 지정한 기준 브랜치와의 변경 검토

cli-help-cli-review-the-changes-introduced-by-a-commit = 커밋이 만든 변경 검토

cli-help-cli-optional-commit-title-to-display-in-the-review = 검토 요약에 표시할 커밋 제목(선택)

cli-help-cli-custom-review-instructions-if-is-used-read-from = 사용자 지정 검토 지시. `-`를 사용하면 표준 입력에서 읽습니다

cli-help-cli-submit-a-new-codex-cloud-task-without-launching = TUI를 열지 않고 새 Codex Cloud 작업 제출

cli-help-cli-show-the-status-of-a-codex-cloud-task = Codex Cloud 작업 상태 표시

cli-help-cli-list-codex-cloud-tasks = Codex Cloud 작업 목록 표시

cli-help-cli-list-accessible-codex-cloud-environments = 접근 가능한 Codex Cloud 환경 목록 표시

cli-help-cli-apply-the-diff-for-a-codex-cloud-task = Codex Cloud 작업의 diff를 로컬에 적용

cli-help-cli-show-the-unified-diff-for-a-codex-cloud = Codex Cloud 작업의 unified diff 표시

cli-help-cli-task-prompt-to-run-in-codex-cloud = Codex Cloud에서 실행할 작업 프롬프트

cli-help-cli-target-environment-identifier-see-codex-cloud-to-browse = 대상 환경 식별자(`codex cloud`에서 탐색)

cli-help-cli-number-of-assistant-attempts-best-of-n = 어시스턴트 시도 횟수(best-of-N)

cli-help-cli-git-branch-to-run-in-codex-cloud-defaults = Codex Cloud에서 실행할 Git 브랜치(기본값은 현재 브랜치)

cli-help-cli-codex-cloud-task-identifier-to-inspect = 검사할 Codex Cloud 작업 식별자

cli-help-cli-filter-tasks-by-environment-identifier = 환경 식별자로 작업 필터링

cli-help-cli-maximum-number-of-tasks-to-return-1-20 = 반환할 최대 작업 수(1-20)

cli-help-cli-pagination-cursor-returned-by-a-previous-call = 이전 호출이 반환한 페이지 커서

cli-help-cli-emit-json-instead-of-plain-text = 일반 텍스트 대신 JSON 출력

cli-help-cli-codex-cloud-task-identifier-to-apply = 적용할 Codex Cloud 작업 식별자

cli-help-cli-attempt-number-to-apply-1-based = 적용할 시도 번호(1부터 시작)

cli-help-cli-codex-cloud-task-identifier-to-display = 표시할 Codex Cloud 작업 식별자

cli-help-cli-attempt-number-to-display-1-based = 표시할 시도 번호(1부터 시작)

cli-help-shared-options-optional-image-s-to-attach-to-the-initial = 초기 프롬프트에 첨부할 이미지(선택)

cli-help-shared-options-model-the-agent-should-use = 에이전트가 사용할 모델

cli-help-shared-options-use-open-source-provider = 오픈 소스 제공자 사용

cli-help-shared-options-specify-which-local-provider-to-use-lmstudio-or = 사용할 로컬 제공자 지정(lmstudio 또는 ollama). --oss와 함께 지정하지 않으면 설정 기본값을 사용하거나 선택기를 표시합니다

cli-help-shared-options-select-the-sandbox-policy-to-use-when-executing = 모델이 생성한 셸 명령을 실행할 때 사용할 샌드박스 정책 선택

cli-help-shared-options-route-approval-requests-through-automatic-review-using-the = workspace-write 샌드박스를 사용하여 승인 요청을 자동 검토로 전달

cli-help-shared-options-skip-all-confirmation-prompts-and-execute-commands-without = 모든 확인을 건너뛰고 샌드박스 없이 명령 실행. 매우 위험합니다. 외부 샌드박스로 보호되는 환경에서만 사용해야 합니다

cli-help-shared-options-run-enabled-hooks-without-requiring-persisted-hook-trust = 이번 실행에 저장된 훅 신뢰 승인을 요구하지 않고 활성 훅 실행. 위험합니다. 훅 출처를 사전 검증하는 자동화에만 사용해야 합니다

cli-help-shared-options-tell-the-agent-to-use-the-specified-directory = 지정한 디렉터리를 에이전트 작업 루트로 사용

cli-help-shared-options-run-the-session-in-a-new-managed-git = 새 관리 Git worktree에서 세션 실행

cli-help-shared-options-additional-directories-that-should-be-writable-alongside-the = 기본 작업 공간과 함께 쓰기를 허용할 추가 디렉터리

cli-help-approval-mode-cli-arg-the-model-decides-when-to-ask-the-user = 사용자 승인 요청 시점을 모델이 결정합니다

cli-help-approval-mode-cli-arg-never-ask-for-user-approval-execution-failures-are = 사용자 승인을 요청하지 않습니다. 실행 실패는 즉시 모델에 반환합니다

cli-help-config-override-cli-option-that-captures-arbitrary-configuration-overrides-specified = `-c key=value`로 지정한 임의 설정 재정의를 받는 CLI 옵션. 호출 코드가 오른쪽 값을 해석하도록 양쪽을 파싱하지 않고 유지합니다

cli-help-config-override-override-a-configuration-value-that-would-otherwise-be = `~/.arcvera/config.toml`에서 읽을 설정값 재정의. 중첩 값은 점 경로(`foo.bar.baz`)를 사용합니다. `value` 부분은 TOML로 파싱하며 실패하면 원문 문자열을 리터럴로 사용합니다

cli-help-config-override-override-a-configuration-value-that-would-otherwise-be-details = `~/.arcvera/config.toml`에서 읽을 설정값을 재정의합니다. 중첩 값은 점 경로(`foo.bar.baz`)를 사용합니다. `value` 부분은 TOML로 파싱하며 실패하면 원문 문자열을 리터럴로 사용합니다.

    예: - `-c model="o3"` - `-c 'sandbox_permissions=["disk-full-read-access"]'` - `-c shell_environment_policy.inherit=all`

cli-help-command-linux-pid-namespace-isolate-default-or-inherit-inherit = Linux PID 네임스페이스: isolate(기본값) 또는 inherit. inherit는 같은 UID의 다른 프로세스에 신호를 보낼 수 있으므로 전용 환경을 구성할 때만 활성화하세요

cli-help-command-allow-permitted-private-ip-destinations-to-use-the = 허용된 사설 IP 목적지에 설정된 업스트림 프록시 사용 허용. 요청 프로토콜에 유효한 업스트림 프록시가 없으면 직접 연결합니다. 루프백은 로컬에 유지됩니다. 이 플래그는 업스트림 라우팅을 강제하지 않습니다

cli-help-command-maximum-number-of-requests-to-process-concurrently-on = 각 연결에서 동시에 처리할 최대 요청 수

cli-help-command-transport-endpoint-url-supported-values-ws-ip-port = 전송 엔드포인트 URL. 지원 값: `ws://IP:PORT`(기본값), `stdio`, `stdio://`

cli-help-command-register-this-exec-server-as-a-remote-environment = 지정한 기본 URL을 사용하여 이 exec-server를 원격 환경으로 등록

cli-help-command-transport-used-for-the-remote-executor-connection = 원격 실행기 연결에 사용할 전송 수단

cli-help-command-environment-id-to-attach-to-when-registering-remotely = 원격 등록 시 연결할 환경 ID

cli-help-command-human-readable-environment-name = 사람이 읽을 수 있는 환경 이름

cli-help-command-use-agent-identity-auth-from-codex-access-token = 원격 등록에 CODEX_ACCESS_TOKEN의 Agent Identity 인증 사용

cli-help-command-read-rotating-executor-credentials-from-stdin-exit-when = 표준 입력에서 교체되는 실행기 인증 정보를 읽고 파이프가 닫히면 종료

cli-help-command-sign-direct-registration-and-websocket-handshake-requests-with = Direct 등록 및 WebSocket 핸드셰이크 요청에 AWS SigV4 서명

cli-help-command-aws-profile-used-for-sigv4-authentication = SigV4 인증에 사용할 AWS 프로필

cli-help-command-aws-signing-region-uses-the-sdk-region-chain = AWS 서명 리전. 생략하면 SDK 리전 체인을 사용합니다

cli-help-command-aws-signing-service = AWS 서명 서비스

cli-help-command-exit-when-the-parent-owned-standard-input-pipe = 부모가 소유한 표준 입력 파이프가 닫히면 종료

cli-help-command-register-an-existing-websocket-exec-server-as-a = 기존 WebSocket exec-server를 원격 환경으로 등록

cli-help-command-destination-exec-server-websocket-url = 대상 exec-server WebSocket URL

cli-help-startup-config-sign-out-using-the-managed-auth-path-before = 새 시작 요구사항을 읽기 전에 관리 인증 경로로 로그아웃

cli-help-startup-config-validate-configuration-and-print-requirements-as-json-without = 앱 서버를 시작하지 않고 설정을 검증하고 요구사항을 JSON으로 출력

cli-help-startup-config-clientinfo-json-used-for-configuration-auth-request-identity = 설정·인증 요청의 클라이언트 식별에 사용할 ClientInfo JSON

cli-help-code-mode-host-selects-the-code-mode-host-for-a-single = 단일 app-server 프로세스의 code-mode 호스트 선택

cli-help-code-mode-host-connect-to-a-remote-code-mode-host-instead = 로컬 호스트를 시작하는 대신 원격 code-mode 호스트에 연결

cli-help-code-mode-host-process-scoped-transport-used-to-reach-the-code = code-mode 호스트 연결에 사용할 프로세스 범위 전송 수단

cli-help-code-mode-host-start-and-own-the-default-local-code-mode = 기본 로컬 code-mode 호스트 시작 및 소유

cli-help-code-mode-host-share-an-http-2-grpc-connection-to-the = 지정한 원격 code-mode 호스트에 HTTP/2 gRPC 연결 공유

cli-help-lib-authentication-mode-for-incoming-websocket-connections = 수신 WebSocket 연결의 인증 모드

cli-help-lib-absolute-path-to-the-capability-token-file = capability-token 파일의 절대 경로

cli-help-lib-hex-encoded-sha-256-digest-of-the-capability = capability 토큰의 16진수 인코딩 SHA-256 해시

cli-help-lib-absolute-path-to-the-shared-secret-file-for = 서명된 JWT bearer 토큰의 공유 비밀 파일 절대 경로

cli-help-lib-expected-issuer-for-signed-jwt-bearer-tokens = 서명된 JWT bearer 토큰의 예상 발급자

cli-help-lib-expected-audience-for-signed-jwt-bearer-tokens = 서명된 JWT bearer 토큰의 예상 대상

cli-help-lib-maximum-clock-skew-when-validating-signed-jwt-bearer = 서명된 JWT bearer 토큰 검증 시 허용할 최대 시계 오차

cli-help-lib-loaded-upgrade-policy-the-default-permits-unauthenticated-connections = 로드된 업그레이드 정책. 기본값은 미인증 연결을 허용하며 각 리스너는 바인딩 주소에 그 정책을 허용할지 결정합니다

cli-help-execpolicycheck-arguments-for-evaluating-a-command-against-one-or = 하나 이상의 execpolicy 파일로 명령을 평가할 인수

cli-help-execpolicycheck-paths-to-execpolicy-rule-files-to-evaluate-repeatable = 평가할 execpolicy 규칙 파일 경로(반복 가능)

cli-help-execpolicycheck-pretty-print-the-json-output = JSON 출력 들여쓰기

cli-help-execpolicycheck-resolve-absolute-program-paths-against-basename-rules-gated = 로드된 정책 파일의 `host_executable()` 정의에 따라 절대 프로그램 경로를 basename 규칙으로 해석

cli-help-execpolicycheck-command-tokens-to-check-against-the-policy = 정책에 대해 검사할 명령 토큰

cli-help-lib-cli-arguments-for-the-proxy = 프록시 CLI 인수

cli-help-lib-port-to-listen-on-if-not-set-an = 수신할 포트. 지정하지 않으면 임시 포트를 사용합니다

cli-help-lib-path-to-a-json-file-to-write-startup = 시작 정보를 한 줄로 기록할 JSON 파일 경로. { "{" }"port": <u16>{ "}" }를 포함합니다

cli-help-lib-enable-http-shutdown-endpoint-at-get-shutdown = GET /shutdown HTTP 종료 엔드포인트 활성화

cli-help-lib-absolute-url-the-proxy-should-forward-requests-to = 프록시가 요청을 전달할 절대 URL(기본값은 OpenAI)

cli-help-lib-directory-where-request-response-dumps-should-be-written = 요청·응답 덤프를 JSON으로 저장할 디렉터리

cli-help-lib-authenticate-to-the-openai-api-using-github-actions = GitHub Actions 워크로드 ID로 OpenAI API 인증. 이 모드의 표준 입력은 API 키 대신 GitHub OIDC 요청 토큰입니다

cli-help-lib-audience-configured-on-the-openai-workload-identity-provider = OpenAI 워크로드 ID 제공자에 설정된 audience

cli-help-lib-openai-workload-identity-provider-id = OpenAI 워크로드 ID 제공자 ID

cli-help-lib-openai-service-account-id-selected-by-the-provider = 제공자 매핑이 선택한 OpenAI 서비스 계정 ID

cli-help-lib-https-token-request-url-from-github-s-runner = GitHub 실행기가 제공하는 ACTIONS_ID_TOKEN_REQUEST_URL의 HTTPS 토큰 요청 URL

cli-help-lib-https-token-request-url-from-github-s-runner-details = GitHub 실행기가 제공하는 ACTIONS_ID_TOKEN_REQUEST_URL의 HTTPS 토큰 요청 URL.

    신뢰하는 실행기 환경의 값을 직접 사용해야 하며 워크플로 입력이나 저장소 내용에서 가져오면 안 됩니다. 프록시는 GitHub 요청 인증 정보를 이 URL로 보냅니다.

cli-help-lib-minimal-openai-responses-proxy = 최소 OpenAI Responses 프록시

cli-help-lib-https-origin-of-the-http-3-proxy = HTTP/3 프록시의 HTTPS origin

cli-help-lib-file-containing-one-exact-approved-https-proxy-origin = 각 줄에 정확히 하나의 승인된 HTTPS 프록시 origin이 있는 파일

cli-help-lib-connect-target-authority-including-a-nonzero-port = 0이 아닌 포트를 포함한 CONNECT 대상 authority

cli-help-lib-read-the-initial-bearer-from-stdin = 표준 입력에서 초기 bearer 읽기

cli-help-lib-read-replacement-bearers-from-stdin-and-stop-when = 표준 입력에서 교체 bearer를 읽고 제어 파이프가 닫히면 중지

cli-help-lib-read-a-json-list-of-extension-header-name = 첫 bearer 전에 확장 헤더 이름·값 쌍의 JSON 목록 읽기

cli-help-lib-emit-credential-safe-json-diagnostics-on-stderr-instead = 사람이 읽는 오류 대신 인증 정보를 보호한 JSON 진단을 표준 오류에 출력

cli-print-help = 도움말 표시

cli-print-version = 버전 표시

cli-print-help-long-hint = 도움말 표시(자세한 내용은 '--help')

cli-print-help-short-hint = 도움말 표시(요약은 '-h')

cli-help-command = 이 메시지 또는 지정한 하위 명령의 도움말 표시

cli-help-subcommands = 하위 명령의 도움말 표시

cli-commands-heading = 명령어

cli-arguments-heading = 인수

cli-options-heading = 옵션

cli-local-server-heading = 로컬 서버 옵션(--remote 미사용)

cli-queue-config-help = 시작 설정값 재정의(key=value). 대기열 메시지는 해당 대화의 모델을 사용합니다.

cli-possible-values = 가능한 값:

cli-default-value = 기본값: { $value }

cli-environment = 환경 변수: { $value }

cli-usage-heading = 사용법:

cli-error-heading = 오류:

cli-error-repeated-argument = '{ $arg }' 인수는 여러 번 사용할 수 없습니다.

cli-error-conflict = '{ $arg }'은(는) 다음 항목과 함께 사용할 수 없습니다: { $prior }

cli-error-equals = '{ $arg }'에 값을 지정하려면 등호가 필요합니다.

cli-error-missing-value = '{ $arg }'에 값이 필요합니다.

cli-error-invalid-value = '{ $arg }'의 값 '{ $value }'은(는) 올바르지 않습니다.

cli-error-invalid-subcommand = 인식할 수 없는 하위 명령 '{ $subcommand }'입니다.

cli-error-missing-required = 필수 인수가 없습니다: { $arg }

cli-error-missing-subcommand = '{ $subcommand }'에 하위 명령이 필요합니다.

cli-error-invalid-utf8 = 인수에 올바르지 않은 UTF-8이 포함되어 있습니다.

cli-error-too-many-values = '{ $arg }'에 예상하지 않은 추가 값 '{ $value }'이(가) 있습니다.

cli-error-too-few-values = '{ $arg }'에 최소 { $minimum }개의 값이 필요하지만 { $actual }개를 받았습니다.

cli-error-wrong-number-values = '{ $arg }'에 { $expected }개의 값이 필요하지만 { $actual }개를 받았습니다.

cli-error-unknown-argument = 예상하지 않은 인수 '{ $arg }'입니다.

cli-error-other = 명령행을 처리하지 못했습니다.

cli-error-valid-values = 가능한 값: { $value }

cli-error-valid-subcommands = 하위 명령: { $value }

cli-error-suggested-subcommand = 다음 하위 명령을 의도하셨습니까? { $value }

cli-error-suggested-argument = 다음 인수를 의도하셨습니까? { $value }

cli-error-suggested-value = 다음 값을 의도하셨습니까? { $value }

cli-error-context = 상세: { $value }

cli-error-try-help = 자세한 내용은 '--help'를 사용하세요.

cli-feature-enabled = config.toml에서 기능 `{ $feature }`을 활성화했습니다.

cli-feature-disabled = config.toml에서 기능 `{ $feature }`을 비활성화했습니다.

cli-error-unknown-feature = 알 수 없는 기능 플래그: { $feature }

cli-feature-under-development-warning = 개발 중인 기능을 활성화했습니다: { $feature }. 개발 중인 기능은 완성되지 않았으며 예측할 수 없는 동작을 할 수 있습니다. 이 경고를 표시하지 않으려면 { $path }에 `suppress_unstable_features_warning = true`를 설정하세요.

cli-aliases = 별칭: { $value }

cli-help-first-installation-only = 수명 관리 잠금 안에서 설치·설정·복구 상태가 모두 없는 경우에만 설치

cli-plugin-reload-requested = { $thread } 대화의 플러그인 설정을 다시 읽었습니다. 다음 캡처에서 MCP 재연결을 요청했으며, 서버 준비 완료는 아직 확인하지 않았습니다.

cli-plugin-reload-partial-warnings = { $message } 일부 새로고침 경고:
    { $warnings }

cli-plugin-reload-invalid-ack = { $thread } 대화의 플러그인 다시 읽기 응답이 원래 대화와 일치하지 않거나 재연결 요청을 확인하지 않았습니다.

cli-plugin-reload-failed = { $thread } 대화의 플러그인 다시 읽기에 실패했습니다: { $error }

cli-plugin-reload-stale = 대화·백엔드 변경 또는 재연결 전의 플러그인 다시 읽기 요청이므로 전송하지 않았습니다. 표시된 대화에서 /reload_plugin을 다시 실행하세요.
