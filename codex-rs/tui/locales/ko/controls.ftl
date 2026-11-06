# Application-owned chat controls and exit guidance.
controls-back = 뒤로
controls-goal-edit = 목표 편집
controls-goal-edit-help = 목표를 입력하고 Enter를 누르세요
controls-goal-resume-title = 일시 중지한 목표를 재개할까요?
controls-goal-resume = 목표 재개
controls-goal-resume-help = 활성 상태로 바꾸고 대기 중일 때 계속 진행합니다
controls-goal-leave-paused = 일시 중지 유지
controls-goal-leave-paused-help = 일시 중지를 유지합니다. 나중에 /goal resume으로 재개하세요
controls-goal = 목표
controls-goal-status = 상태:{ " " }
controls-goal-objective = 목표 내용:{ " " }
controls-goal-time = 사용 시간:{ " " }
controls-goal-tokens = 사용 토큰:{ " " }
controls-goal-budget = 토큰 예산:{ " " }
controls-goal-active-commands = 명령: /goal edit, /goal pause, /goal clear
controls-goal-paused-commands = 명령: /goal edit, /goal resume, /goal clear
controls-goal-finished-commands = 명령: /goal edit, /goal clear
controls-goal-active = 활성
controls-goal-paused = 일시 중지
controls-goal-stalled = 진행 불가
controls-goal-usage-limited = 사용량 제한
controls-goal-budget-limited = 예산 제한
controls-goal-complete = 완료
controls-voice-settings = 음성 설정
controls-voice-devices = 오디오 장치 설정
controls-voice-choose = 음성 선택
controls-voice-select = 음성 선택
controls-voice-next = 다음 음성 대화부터 적용됩니다.
controls-voice-input-device = 입력 장치
controls-voice-output-device = 출력 장치
controls-voice-system-default = 시스템 기본값
controls-voice-sound-devices = 오디오 장치
controls-voice-ambiguous = 장치 이름이 중복됩니다. 시스템 기본값을 사용하세요.
controls-voice-input-channels = 입력 채널
controls-voice-all-channels = 모든 채널 (믹싱)
controls-voice-channel-help = 믹싱할 입력 채널을 하나 이상 선택하세요.
controls-voice-channel-required = 입력 채널을 하나 이상 선택하세요.
controls-ide-off = IDE 컨텍스트가 꺼져 있습니다.
controls-ide-on = IDE 컨텍스트가 켜져 있습니다.
controls-ide-usage = 사용법: /ide [on|off|status]
controls-ide-skipped = 이 메시지에는 IDE 컨텍스트를 포함하지 않았습니다.
controls-ide-future = 이후 메시지에 현재 IDE 선택 영역과 열린 탭이 포함됩니다.
controls-ide-connected = IDE에 연결되었습니다.
controls-ide-enable-failed = IDE 컨텍스트를 켤 수 없습니다.
controls-clear-disabled = 작업이 진행 중일 때는 Ctrl+L을 사용할 수 없습니다.
controls-copy-no-response = 복사할 에이전트 응답이 없습니다
controls-rename-thread = 대화 이름 변경
controls-name-thread = 대화 이름 지정
controls-name-help = 이름을 입력하고 Enter를 누르세요
controls-name-empty = 대화 이름은 비워 둘 수 없습니다.
controls-name-generating = 제목 제안 생성 중…
controls-name-suggested = 이 대화에서 제안한 제목
controls-exit-running = 작업이 아직 실행 중입니다
controls-exit-choose = 현재 작업을 어떻게 처리할지 선택하세요.
controls-exit-cancel = 작업 취소
controls-exit-cancel-help = 현재 작업을 중지하고 Codex에 머무릅니다
controls-exit-background = 백그라운드에서 실행
controls-exit-background-help = 작업을 계속 실행하면서 Codex를 종료합니다
controls-exit = 종료
controls-exit-help = 현재 작업을 중지하고 Codex를 종료합니다
controls-exit-stop-shortcut = 설정된 중지 단축키를 사용하세요
controls-exit-disconnected = 이 작업과의 연결을 해제했습니다. 실행 중인 작업은 계속됩니다.
controls-exit-disconnected-fatal = 이 작업과의 연결을 해제했습니다. 작업이 계속 실행 중일 수 있습니다.
controls-exit-disconnected-stopped = 이 작업과의 연결을 해제했습니다. 현재 턴을 중지했습니다.
controls-exit-reconnect = 다시 연결하려면 다음을 실행하세요:
controls-exit-continue = 이 세션을 계속하려면 다음을 실행하세요:
controls-goal-value = 목표: { $value }
controls-copy-failed = 복사 실패: { $value }
controls-voice-microphone = 마이크: { $value }
controls-voice-input = 입력 { $value }
controls-voice-inputs = 입력 { $value }
controls-voice-saved = 음성을 { $value }(으)로 설정했습니다. 다음 음성 대화부터 적용됩니다.
controls-exit-press-key = { $value } 키를 누르세요
controls-exit-stop-turn = 현재 턴을 중지하려면 { $command } 명령을 실행하고 이 작업을 선택한 다음 { $hint }.
controls-exit-archived = 보관된 세션: { $value }
controls-exit-select = 또는 { $command } 명령을 실행하고 { $value } 항목을 선택하세요.
controls-exit-session-id = 세션 ID: { $value }
controls-editor-missing = 외부 편집기를 열 수 없습니다. Codex 시작 전에 $VISUAL 또는 $EDITOR를 설정하세요.
controls-editor-failed = 편집기 열기 실패: { $value }
controls-redraw-failed = 대화 기록 다시 그리기 실패: { $value }
controls-switch-failed = 보조 대화 전환 실패: { $value }
controls-clear-failed = 터미널 UI 지우기 실패: { $value }
controls-review-steer-unavailable = /review 중에는 방향 지시 메시지를 보낼 수 없습니다. 검토를 취소하려면 지금 Ctrl+C를 누르세요.
controls-tokens = 토큰 사용량: 합계={ $total } 입력={ $input }{ $cached } 출력={ $output }{ $reasoning }
controls-tokens-so-far = 현재까지 토큰 사용량: 합계={ $total } 입력={ $input }{ $cached } 출력={ $output }{ $reasoning }
controls-tokens-cached = { " " }(+ 캐시 { $value })
controls-tokens-reasoning = { " " }(추론 { $value })
controls-ide-open-project = Codex 확장이 활성화된 VS Code 또는 Cursor에서 이 프로젝트를 여세요.
controls-ide-no-context-retry = IDE 확장이 컨텍스트를 제공하지 않았습니다. /ide를 다시 실행하세요.
controls-ide-too-large-retry = 선택한 IDE 컨텍스트가 너무 큽니다. IDE에서 큰 선택 영역을 해제하고 /ide를 다시 실행하세요.
controls-ide-request-retry = Codex가 IDE 컨텍스트를 요청하지 못했습니다. /ide를 다시 실행하세요.
controls-ide-read-retry = Codex가 IDE 컨텍스트를 읽지 못했습니다. /ide를 다시 실행하세요.
controls-ide-too-large = 선택한 IDE 컨텍스트가 너무 큽니다. IDE에서 큰 선택 영역을 해제하세요.
controls-ide-timeout = IDE 컨텍스트 대기 시간이 초과되었습니다. 이후 메시지에서도 계속 시도합니다.
controls-ide-changed = Codex가 컨텍스트를 요청하는 동안 IDE 연결이 변경되었습니다. 이후 메시지에서도 계속 시도합니다.
controls-ide-extension-timeout = IDE 확장이 제한 시간 안에 응답하지 않았습니다. 이후 메시지에서도 계속 시도합니다.
controls-ide-incompatible = 연결된 IDE 확장이 이 IDE 컨텍스트 요청과 호환되지 않습니다.
controls-ide-unsupported-client = 연결된 IDE 클라이언트가 IDE 컨텍스트 요청을 지원하지 않습니다.
controls-ide-lost-connection = Codex가 컨텍스트를 요청하는 동안 IDE 연결이 끊겼습니다. 이후 메시지에서도 계속 시도합니다.
controls-ide-unexpected-response = Codex가 예상하지 못한 IDE 컨텍스트 응답을 받았습니다. 이후 메시지에서도 계속 시도합니다.
controls-ide-no-context-future = IDE 확장이 컨텍스트를 제공하지 않았습니다. 이후 메시지에서도 계속 시도합니다.
controls-ide-read-future = Codex가 IDE 컨텍스트를 읽지 못했습니다. 이후 메시지에서도 계속 시도합니다.
controls-ide-platform-unsupported = 이 플랫폼에서는 IDE 컨텍스트가 지원되지 않습니다
controls-copy-confirmed = { $value } 클립보드에 복사됨
controls-copy-unconfirmed = 복사 확인 안 됨. /export로 대화를 저장하세요
controls-copy-pending = { $value } 복사 중…
controls-copy-busy = 이미 복사 중입니다


cleanup-unsupported = 선택한 서버는 서브에이전트 정리 API를 지원하지 않습니다. 삭제한 대화는 0개입니다. 지원하는 서버를 --remote로 명시하거나, 지원하는 서버로 전환한 뒤 다시 사용하세요.
cleanup-selection-preserved = 선택한 항목은 종료 상태를 확정할 수 없어 보존됩니다: { $id }
cleanup-confirmation-required = 실제 정리에는 터미널 확인 또는 --apply --confirm이 필요합니다.
cleanup-confirmation-token = 정리
cleanup-confirmation-prompt = 위 미리보기에서 선택한 { $count }개 대화를 영구 삭제하려면 '{ $token }'를 입력하세요:
cleanup-partial-failure = 일부 항목을 보존했거나 정리 결과를 확정하지 못했습니다. 출력된 항목별 결과를 확인하세요.

controls-compact-pending = 현재 턴이 끝난 뒤 압축을 한 번 시도합니다. 아직 시작하지 않았으며, 대화가 사용 중이거나 대기 메시지가 있으면 실행하지 않습니다.

controls-compact-started = 압축을 시작했습니다. 완료 여부는 압축 이벤트로 알립니다.

controls-compact-busy = 대화가 사용 중이거나 대기 메시지가 있어 압축을 시작하지 않았습니다.

controls-compact-not-started = 압축을 시작하지 않았습니다:

controls-compact-failed = 압축 요청 실패:

controls-compact-timeout = 시간초과로 압축 결과를 확인하지 못했습니다. 자동으로 다시 요청하지 마세요.

controls-memory-disabled = 메모리가 꺼져 있거나 사용할 수 없습니다. 메모리 활성화를 명시적으로 요청하세요. 읽기 설정만 저장해서는 기능이 켜지지 않으며, 환경·관리 설정이 활성화를 제한할 수 있습니다.
controls-memory-consolidation-failed = 메모리 통합 요청 실패: { $error }
controls-memory-consolidation-admitted = 통합 요청의 접수 상태입니다. accepted는 완료가 아닙니다. memory_view에서 실제 작업 상태를 확인하세요. 작업 완료도 특정 노트의 반영을 보장하지 않습니다.
controls-memory-note-scope-unsupported = 이 서버는 다른 범위를 지정한 메모리 저장을 지원한다고 확인되지 않았습니다. 노트는 저장하지 않았습니다.
controls-memory-note-scope-unconfirmed = 메모리 저장 응답의 실제 범위를 확인할 수 없습니다. 결과를 조회하고 자동 재전송하지 마세요.
language-save-legacy-cleanup-failed = 클라이언트 언어 선택은 저장했지만 기존 공용 설정 키 제거에 실패했습니다: { $error }. 자동으로 재전송하지 마세요.
daemon-auto-version-fallback = 공유 백그라운드 서버 버전 { $server }이(가) 현재 CLI { $client }와 다르거나 확인되지 않았습니다. 공유 서버를 변경하거나 다시 시작하지 않고 현재 CLI의 백엔드로 실행합니다.

controls-memory-overridden = 메모리 설정을 저장했지만 다른 설정이 덮어썼습니다. 현재 실행 환경의 활성화는 확인되지 않았습니다. 자동으로 다시 요청하지 마세요.

controls-memory-runtime-differs = 메모리 설정을 저장했지만 현재 실행 상태와 다릅니다. 세션의 명시 설정이나 관리·환경 제한이 활성화를 막을 수 있습니다. 자동으로 다시 요청하지 마세요.

controls-memory-confirmed = 메모리 설정과 현재 실행 상태를 확인했습니다. 읽기는 이후 문맥에, 생성은 새 처리에 적용되며 실행 중인 처리는 끝날 수 있습니다. 요약 생성이나 모델 턴을 시작하지 않았습니다.

diagnostics-status = 현재 실행의 시작 모드와 저장 설정을 구분해 표시합니다. 일반 로깅만 켜도 캐시 관측이 켜지는 것은 아닙니다.

diagnostics-saved-next-launch = 다음 실행에 적용할 설정을 저장했습니다. 현재 실행은 시작 모드를 유지합니다. 일반 로깅만 켜도 캐시 관측이 켜지는 것은 아닙니다.

diagnostics-home-failed = 로컬 진단 설정 경로를 확인하지 못했습니다: { $error }

diagnostics-operation-failed = 진단 설정 처리에 실패했습니다: { $error }. 변경을 다시 시도하기 전에 상태를 조회하세요.

diagnostics-settings-busy = 다른 작업이 진단 설정을 변경하고 있습니다. 상태를 조회한 뒤 변경을 다시 시도하세요.

diagnostics-operation-unconfirmed = 진단 설정 처리 결과를 확인하지 못했습니다: { $error }. 변경을 다시 시도하기 전에 상태를 조회하세요.

diagnostics-cache-wire-requires-response = 캐시 wire 관측에는 응답 관측이 필요합니다. 두 항목을 명시적으로 선택하세요.

controls-memory-command-hint = /memory enable on|off · /memory edit <정정 내용> · /memory read on|off · /memory generate on|off
controls-memory-usage = 사용법: /memory view | enable on|off | edit <정정 내용> | read on|off | generate on|off | scope project|global
daemon-auto-selected-mismatch = 선택된 공유 서버 실행 파일 { $managed_path }은(는) 현재 CLI { $cli_path }와 다릅니다. 이번 대화는 현재 CLI에 포함된 서버를 사용하며 공유 설치본은 보존했습니다. 명시적으로 선택을 바꾸려면 `codex app-server daemon update --from-cli --yes --no-force`를 실행하세요.
daemon-implicit-version-mismatch = 공유 서버 { $endpoint }에서 확인한 버전 { $server }이(가) 현재 CLI { $client }와 다르거나 확인되지 않아 연결을 거부했습니다. 현재 CLI 실행 파일: { $cli_path }; --no-daemon으로 이 CLI에 포함된 백엔드를 사용할 수 있습니다. 공유 서버는 변경하거나 다시 시작하지 않았습니다. 소켓 경로만으로 실행 중인 서버 실행 파일 경로는 확인할 수 없습니다. 명시적으로 이 CLI의 서버를 선택하려면 `codex app-server daemon update --from-cli --yes --no-force`를 실행하세요.
daemon-version-manage-hint = /daemon으로 로컬 백그라운드 서버를 관리할 수 있습니다.
daemon-version-update-warning = 업데이트하면 진행 중이거나 대기 중인 작업이 중단될 수 있습니다.
daemon-running-embedded = 현재 CLI에 포함된 백엔드로 실행합니다. 공유 백그라운드 서버를 사용하지 않는 이유: { $reason }.
