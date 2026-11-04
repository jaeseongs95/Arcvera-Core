# R37 footer 서브에이전트 선택 사전 사례

요구: 하단에 보이는 서브에이전트를 클릭 또는 키보드로 선택하여 기존 대화 전환 경로로 진입한다. 출력은 기존 최대 세 detail 행/스크롤/작은 터미널 정책을 유지한다. 삭제·중단·새 세션 생성과 선택을 혼합하지 않는다.

## UI 계약과 CS 조건

기존 Alt+S가 footer를 펼칠 때 명시 선택 focus와 ID 기반 highlight를 부여한다. focus에서 ↑/↓로 이동, Enter로 진입, Esc로 composer 복귀한다. 문자 입력은 footer focus를 해제한 뒤 기존 composer에 전달한다. focus가 없거나 detail이 실제로 그려지지 않은 작은 화면에서는 Enter를 가로채지 않는다. Alt+Up queue 편집, 기존 /subagents picker와 thread 선택의 권한·실행 경로를 보존한다.

본문/이름/행 번호는 identity가 아니다. entry는 실제 ThreadId를 보유하며 마지막으로 그린 row→ID만 click hit-test에 사용한다. summary/순서/scroll/root 변경은 이전 hit map을 무효화한다. queued AppEvent는 root와 target ID를 함께 담고 현재 root·visible membership을 재검증한 뒤 기존 select_agent_thread를 호출한다. 선택된 항목 삭제/숨김/완료 만료는 다른 항목으로 암묵 이동하지 않는다. backend·모델·운영 데이터는 footer 렌더에서 읽지 않는다.

## engineering-test-plan

기존 Rust tests, ratatui Buffer, BottomPane/App event 채널을 재사용한다. Cargo는 root가 안정 후보에서 직렬 실행한다. pre-fix red는 실행하지 않았으며 기존 테스트 PASS를 R37 결과로 세지 않는다.

| case | 입력·사전조건·절차 | 독립 oracle·거부할 오류 |
| --- | --- | --- |
| S01 | 15개 실제 ID, approval/input/failed/running 우선순위; 실제 Buffer를 비영점 terminal Rect에 그림→detail row left-click | 그 행에 그려진 ID의 root-bound selection 1개, 실제 이름정렬/원래 spawn index 혼동 거부 |
| S02 | 15개, scroll 후 다시 render→visible 두 번째 행 click | 스크롤 후 실제 visible ID로 event; 과거 offset target 거부 |
| S03 | Alt+S→render→Down→render→Enter | highlight ID 선택 event; draft·cursor 보존, prompt 제출 0개 |
| S04 | focus Esc→Enter / 기본 nofocus Enter | composer 기존 제출 유지, 선택 event 없음 |
| S05 | selected 항목 삭제/hidden, 상태 우선순위 변경, root 변경 뒤 이전 좌표 click·queued event | stale click/event 거부; 다른 row를 대신 선택하지 않음 |
| S06 | 펼친 뒤 width47 또는 작은 height로 축소·render→이전 detail 좌표/Enter | hidden detail 선택 없음; 기존 composer 입력 보존 |
| S07 | Alt+Up·popup/approval/질문 활성·문자입력 | 기존 keymap/우선순위 유지, footer focus가 composer 편집을 삼키지 않음 |
| S08 | 기존 app 선택 실제 경계 | root+visible 검증 뒤 select_agent_thread를 재사용; 저장한 draft/queue·backend turn 보존, 삭제/중단 작업 없음 |

## 검증 범위

component: footer 공개 handle_key/handle_mouse+render의 선택 identity, mapping, invalidation. BottomPane: 실제 AppEvent와 composer draft/Enter routing. App: root/hidden target 재검증과 기존 thread 선택 연동. mock terminal 좌표는 실제 ratatui Rect+MouseEvent를 사용하되 실제 사용자의 터미널/운영 세션을 조작하지 않는다. 최종 실제 실행본/terminal mouse capture는 root 통합 검증 및 fresh 독립감사 대상이다. 초기 실행 상태 전부 NOT_RUN.

## 구현 후보와 검증 인계

기존 footer entry에 실제 ThreadId를 추가하고 별도 backend 목록을 만들지 않았다. sort/scroll 뒤 실제로 그린 최대 3행의 ID만 hit map에 보관한다. metadata 변경/scroll/root 변경/숨김 render는 이전 mapping을 무효화한다. terminal origin이 (0,0)이 아닌 Buffer 좌표를 사용하는 시험을 추가했다.

`SelectFooterAgent { root_thread_id, thread_id }`는 현재 root와 footer visible membership을 다시 검사한 뒤 기존 `select_agent_thread`로 전달된다. 기존 misalignment 전환 차단도 같은 event에 적용했다. 렌더 중에는 backend를 읽지 않으며 선택이 삭제·중단을 호출하지 않는다.

Alt+S/summary click은 명시 footer focus를 부여하고 marker/highlight를 그린다. ↑↓/Enter/Esc는 해당 focus에서만 소비한다. hidden detail은 focus를 해제한다. 문자·수정키와 다른 shortcut은 focus를 해제하고 기존 입력으로 넘긴다. 선택 event를 보낸 뒤에는 선택 ID를 소비하고 focus를 유지하여 비동기 전환 전 반복 Enter가 draft를 제출하지 않게 한다. Esc는 ChatWidget의 interrupt/queued-steer 검사보다 먼저 footer로 전달하며, owned transcript도 search/selection 우선순위 뒤 footer 선택키를 넘긴다.

실제 그려진 footer가 있고 modal/popup/expanded question이 없을 때만 inline mouse capture를 요청한다. root teardown/숨김/overlay render는 painted geometry를 제거하므로 capture는 기존 Inherit/Disabled 정책으로 돌아간다. 실제 terminal driver가 보내는 MouseEvent를 쓰는 기존 입력 경로를 재사용한다.

추가 회귀 filter 후보:

- `footer_click_selects_painted_ids_after_priority_sort_and_scroll_at_terminal_offset`
- `footer_keyboard_focus_preserves_identity_across_reorder_and_rejects_removed_hidden_rows`
- `footer_focus_enter_escape_and_text_keys_preserve_composer_submission_contract`
- `footer_selection_public_keys_preserve_draft_and_escape_never_interrupts_running_turn`
- `footer_mouse_capture_and_raw_terminal_coordinates_follow_painted_visibility`
- `footer_selection_uses_owned_terminal_routing_and_existing_thread_switch_with_stale_guards`

실행 상태는 아직 모두 NOT_RUN이다. 마지막 사례는 격리 embedded app-server의 실제 thread ID와 Tui/owned transcript 입력 경로를 사용하며, 실제 운영 터미널·사용자 세션을 조작하는 시험이 아니다. source 안정화 뒤 root가 Cargo와 통합시험 결과를 연결한다.

최종 후보 추가 경계: 실제 inline `App::handle_tui_event` Mouse arm에 footer의 painted bounds에 한정한 위임을 연결했다. App input의 empty-composer backtrack과 external-writer navigation보다 footer 선택키를 먼저 처리하지만 fork-in-progress/modal/popup/question의 기존 차단은 유지한다. Escape 반환 이후 held Repeat가 turn interrupt/backtrack으로 새어 나가지 않도록 기존 ActivationKeyGuard를 재사용하고, 새 대화 진입 뒤 Enter 반복도 같은 기존 guard로 막는다. 일반 문자·수정키와 focus 없는 기본 Enter의 의미는 기존대로다.

마지막 정적 검사: 승인된 소유 Rust 파일만 `rustfmt --edition 2024 --config skip_children=true` 실행, exit 0(기존 imports_granularity unstable 경고). 11개 Rust 파일과 영어/한국어 locale의 CRLF·UTF-8(no BOM)를 유지했고 scoped `git diff --check` exit 0. root 직렬 Cargo와 실제 실행본/터미널 통합 검증은 아직 NOT_RUN이다. 실제 사용자의 운영 세션을 조작하지 않았다.

## P1 terminal capture 사전 추가 사례

fresh review가 확인한 공백: `Tui::set_mouse_capture`는 active alternate screen에서만 실제 명령을 썼다. inline/no-alt에서는 policy 필드만 갱신해 실제 마우스 보고가 켜지지 않았다. 앞선 MouseEvent 주입 시험은 이 terminal 출력 경계를 검증하지 못했다. 이를 PASS 근거로 확대하지 않는다.

| case | 실제 경계·절차 | oracle |
| --- | --- | --- |
| S09 | 격리 child test process의 실제 Tui에서 no-alt/inline Enabled → Inherit | stdout terminal mode 명령을 vt100에 replay, main screen 유지·mouse any-motion on→none. 필드 값 비교만으로 통과하지 않음 |
| S10 | 같은 inline capture에서 editor restore/복귀·panic cleanup/복구·최종 restore | handoff/cleanup 때 off, 복귀 때 요청 정책대로 on, 마지막 none; alternate screen 진입 없음 |
| S11 | inline suspend의 준비/실제 resume action 출력 | suspend 전 mouse off, inline viewport resume 뒤 정책에 따라 on; owned/overlay 기존 복원 보존 |
| S12 | terminal input mode apply 중 부분 쓰기/flush 오류 | 원래 오류 전달, mouse 활성 추적은 cleanup이 재시도할 수 있게 유지; 기존 failure fixture 재사용 |

수정 방향: 기존 AlternateScreen이 화면 진입과 별도로 이미 추적하는 mouse/input configured 상태와 shared cleanup 경로를 재사용한다. 새 session state/별도 terminal owner를 만들지 않는다. inline 정책을 실제 backend에 적용하고 editor·panic·suspend 복귀에서도 screen 전환 없이 다시 적용한다. 기존 shell/editor handoff에서 capture 해제를 먼저 수행한다. 운영 터미널/사용자 세션을 시험하지 않는다. Cargo와 실제 실행은 root 직렬 검증 전까지 NOT_RUN이다.

### P1 수정 후보와 실행 상태

`Tui::set_mouse_capture`가 inline/no-alt에서도 실제 backend로 기존 input apply를 호출하도록 바꿨다. shared state는 기존 AlternateScreen의 mouse_active/input_configured를 그대로 사용한다. main `set_modes`의 기존 tmux probe 반환값을 기록하여 inline에서도 mouse=off를 존중하며 새 probe를 추가하지 않는다. inactive restore도 configured 상태를 무효화하고 input release를 수행하여 같은 Enabled 요청이 handoff 뒤 누락되지 않게 했다. inactive 화면에는 alternate-scroll을 enable하지 않는다.

editor 복귀와 caught-panic 복구는 inline이면 capture만 다시 적용하며 alternate screen으로 진입하지 않는다. Unix resume의 RealignViewport도 같은 capture 정책을 재적용한다. 초기/중간/최종 오류와 부분 write의 기존 추적·best-effort cleanup을 제거하지 않았다.

새/확장 시험: Unix `tui::alternate_screen::tests::inline_mouse_capture_reapplies_after_restore_and_honors_main_tmux_policy`, `tui::owned_screen_tests::inline_mouse_capture_emits_real_terminal_modes_without_entering_alt_screen`; Windows 기존 `tui::alternate_screen::windows_tests::mouse_capture_restores_console_mode_and_encoding` 확장. Unix child는 실제 Tui backend stdout을 vt100에 단계별 replay하며 실제 PTY raw-mode/editor를 실행하는 시험은 아니다. Windows child는 기존 별도 console fixture에서 실제 ENABLE_MOUSE_INPUT과 원래 console mode를 검사하며 editor handoff·cleanup/recover 경로도 호출한다. 정상 종료·panic 후 복구는 공유 실제 복원 함수를 사용하고 운영 프로세스에 panic을 주입하지 않는다.

수정 전 red와 추가 Cargo/console/PTY 시험은 이 담당에서 실행하지 않았다. 사전 사례 뒤 제품 3파일과 기존 시험 3파일만 수정했다. 소유 6파일의 Rust 1.99.1 rustfmt(exit 0, 기존 unstable 옵션 경고)와 scoped git diff --check(exit 0)를 확인하고 CRLF를 보존했다. root가 R37 후보를 직렬 검증하기 위해 코드 편집을 freeze한다. 앞선 MouseEvent 주입 PASS나 이번 source/format 검사를 terminal 동작 PASS로 보고하지 않는다.

## TUI02 실제 회귀 실패 후 추가 사례 (수정 전)

root가 R37 TUI02 실제 실행 76개 중 74 PASS/2 FAIL(exit101)을 보고했다. 이 담당은 해당 compile.log를 지정 절대경로로 직접 확인한 뒤 원시 결과와 결속한다. 기존 두 assertion은 유지하며 기대 문구/너비 조건을 약화하지 않는다.

- S13: 15/100개 footer, 100열 펼침·focus 상태에서 접기 키 `alt+s`와 선택 조작을 동시에 표시한다. 긴 scroll 안내가 선택 안내와 겹쳐 필수 조작 전체가 사라지면 실패다. 기존 bounded_agent_footer test가 실패 oracle이며 새 언어별 80/100/120열 경계가 필요하면 추가한다.
- S14: resume picker의 영어/한국어 검색 prompt/query/error를 0/1/12/24/40열로 렌더한다. 한글·中·emoji·결합문자를 포함한 query 원문은 보존하고 실제 표시 line.width()는 폭 이하이어야 한다. 좁은 빈 prompt와 오류 문구에도 같은 경계를 적용하며 기존 120열 원문 출력은 보존한다. 기존 localized_picker_preserves_query_and_fits_footer_display_widths를 실패 oracle로 재사용한다.

source 원인 분리: footer는 scroll hint를 먼저 붙이고 selection hint를 덧붙인 뒤 전체가 맞지 않으면 hint 전체를 버린다. 검색 행은 grapheme 개수를 자르는 truncate_text에 terminal cell 폭을 전달한다. 별개 원인이므로 한 수정으로 합치거나 기존 oracle 기대값만 교체하지 않는다. 후속 Cargo 실행은 root 직렬 검증 전까지 NOT_RUN이다.

### TUI02 실패 직접 확인과 수정 후보

지정 근거 `D:/codex/build/official-cli-patch-20261028/r37-development-01/tui-footer-tests-02/receipt.json` 및 `compile.log`를 직접 읽었다. receipt의 대상 source는 현재 R37 worktree이며 sourceDiffSha256 전후가 `f856984885eff0675c376b0f0be0bd2ee06ddbdfd2ebba84a1f31b440591d29c`로 동일하다. 명령은 `cargo test --profile dev-small --offline --locked --target x86_64-pc-windows-msvc -j2 -p codex-tui --lib footer_ -- --test-threads=1 --nocapture`, 종료 101, 74 PASS/2 FAIL/0 ignored였다. 로그에서 footer `text.contains("alt+s")`와 resume `search_line(...).width() <= width`의 assertion 실패를 각각 확인했다. 이는 수정 전 실제 실패 근거이며 아래 후보의 PASS는 아니다.

수정은 두 소스 파일에 한정했다. footer는 필수 접기 안내→focus 선택 안내→선택적 위치/scroll 안내 순서로 각 안내를 실제 너비 예산 안에서 추가한다. 선택 안내를 더한 뒤 모든 안내를 버리는 경로를 제거했고 기존 oracle 문구는 그대로다. resume search_line은 기존 display-width/grapheme-safe styled line helper를 사용하여 query/prompt/error를 terminal cell 폭에 맞춘다. query 원문과 120열 전체 문자열·error 색상은 보존한다.

새 경계 회귀 2개: `footer_focused_hints_keep_collapse_and_selection_before_optional_scroll_in_both_languages`, `search_line_bounds_prompt_query_and_error_by_terminal_cells_without_mutating_query`. 영어/한국어 80/100/120열 접기·Enter/Esc 안내, 0/1/2/3/12/24/40/120열 prompt/query/error의 cell 폭과 원문·색상 보존을 검사한다. 기존 실패 시험 2개의 기대값·너비 조건은 변경하지 않았다. 소유 2파일의 rustfmt와 scoped git diff --check는 종료 0, 기존 CRLF를 유지했다. 이 담당은 Cargo를 실행하지 않았고 수정 후 동작은 root 직렬 검증 전까지 NOT_RUN이다. 코드 편집을 다시 freeze한다.

## Windows console helper 숨김 사전 사례

root 제공 `console-visibility-api-probe/receipt.json`을 직접 확인했다(executed=false, exitCode=1). 설치된 stable rustc에서 show_window는 E0658로 거부됐다는 root 관측을 따른다. nightly/feature나 새 WinAPI launcher를 추가하지 않는다.

- S15: Windows 시험 parent는 내장 Windows PowerShell 5.1을 CREATE_NO_WINDOW, -NoProfile, -NonInteractive로 실행한다. 고정 script가 Start-Process -WindowStyle Hidden으로 별도 console child를 만들며 -NoNewWindow는 쓰지 않는다. exe/작업 경로/출력 파일은 환경 변수로 전달하고 script 문자열에 삽입하지 않는다.
- S16: child는 자기 Process handle의 WaitForExit(20000)만 기다리고 timeout 시 그 handle만 Kill하여 5초 안에 회수를 확인한다. 실패·도구 부재·capture 파일 누락은 명확히 FAIL이다. exit code 0과 실제 child stdout의 `1 passed`라는 기존 oracle은 유지한다.
- S17: stdout/stderr는 fixture의 TempDir 파일로만 수집하고 PSModulePath는 자식 PowerShell 환경에서 SystemRoot/System32/WindowsPowerShell/v1.0/Modules로 고정한다. 운영 환경·설정·다른 프로세스는 변경하지 않는다.

이 담당은 이 helper나 실제 console 시험을 실행하지 않는다. root가 숨김 동작과 console mode 시험을 별도로 확인한다.

### Hidden fixture 후보

Windows test parent만 변경했다. stable 표준 CommandExt의 CREATE_NO_WINDOW로 내장 WinPS 5.1을 띄우고 고정 script의 Start-Process -WindowStyle Hidden에 실제 test executable을 전달한다. 경로/출력 파일은 env data이며 shell source에 문자열 삽입하지 않는다. child는 새 console을 보유하고 부모는 해당 Process handle을 유지한다. 20초 timeout과 5초 Kill 회수는 자기 handle에만 적용하며, exit code 누락·PowerShell 부재·capture 누락은 FAIL이다. child exit code 0 및 실제 stdout의 `1 passed` oracle을 유지했다. tempfile 소유 경로는 시험 수명 종료 시 기존 TempDir 정리를 사용한다.

확인된 개발가이드의 WinPS 5.1 PSModulePath 제약을 자식 환경에만 적용했다. 제품 파일·운영 환경·설정은 바꾸지 않았다. 소유 Windows 시험 파일 rustfmt 및 scoped git diff --check는 종료 0이며 CRLF 유지. Cargo·helper·console 실행은 0회로 root 실제 검증이 남는다. 코드 편집 freeze.

## R37 hover 발견성 사전 사례

기본 요약은 클릭 가능 안내를 영어/한국어로 제공한다. 마지막 paint Rect와 detail row→ThreadId map 안의 actionable 행에 MouseMoved로 진입하면 행을 강조하고 summary는 펼치기/접기, detail은 대화 이동 힌트를 보인다. 같은 행 안의 이동은 상태를 바꾸거나 추가 redraw를 요청하지 않는다. 이탈은 원복하며 hover는 선택·composer focus·turn·검색/선택 interaction을 변경하지 않는다.

| case | 사전 입력과 독립 oracle |
| --- | --- |
| H01 | collapsed footer를 실제 Buffer에 그림: 기본 click/클릭 안내, 한 행 높이; summary MouseMoved 진입 후 강조와 localized expand action, 이동만으로 selection event 없음 |
| H02 | expanded 실제 detail 좌표 진입: 해당 ID 행 강조와 localized open action; 같은 행의 다른 column은 hover 변화 false; 이탈은 1회 변화 뒤 원복 |
| H03 | scroll/sort/root/resize/삭제/hidden paint: 이전 hover와 row map 무효화, 다시 그리기 전 옛 detail 좌표는 hover 불가 |
| H04 | en/ko 및 width 0/1/14/24/47/80/120: 좁은 셀 경계 밖 sentinel 불변, 기존 최대 3 detail 행, 24 이상에서 /subagents와 클릭 안내 보존 |
| H05 | 실제 ChatWidget mouse API와 inline/owned App 경로: modal 우선, draft/keyboard focus 보존, hover가 검색·선택을 닫거나 turn을 중단하지 않음 |

기존 선택 6개·폭·입력 assertion은 보존한다. 이 단계는 사례를 소스에 추가한 상태이며 Cargo/시험 실행은 금지되어 pre-fix red 및 구현 후 결과 모두 NOT_RUN이다. 실제 OS pointer hover 완료를 의미하지 않는다.

### hover 구현 후보·동결 인계

요약에는 기본 click/클릭 안내를 붙이고, summary hover는 click expand/collapse(클릭 펼치기/접기), detail hover는 click open chat(클릭 대화 이동)을 표시한다. 기존 reverse/underline style만 사용하며 pointer-shape escape나 새 mouse-capture 정책을 추가하지 않았다.

hover는 Summary 또는 마지막으로 그린 ThreadId로 보관한다. 같은 Rect/row map의 재렌더는 hover를 유지하고, geometry/ID map 변경·scroll·keyboard 이동·summary/root metadata 변경·삭제·숨김은 hover를 초기화한다. 상세 ID가 현재 visible하지 않으면 hover 대상으로 채택하지 않는다. 기존 최대 3행·좁은 terminal 축소 정책은 유지한다.

BottomPane은 hover 대상 변화 때만 frame을 요청한다. inline은 기존 ChatWidget footer mouse API에 MouseMoved/이탈을 연결하고, owned transcript는 hover를 기존 click의 검색/선택 종료 처리보다 먼저 전달한다. hover는 composer focus·draft·Enter guard·선택 event를 바꾸지 않는다. modal/popup/question 활성 시 hover를 거부하고 렌더에서 hover를 정리한다.

추가한 시험 5개 정확 selector:

- `bottom_pane::subagent_status::tests::footer_hover_reveals_localized_actions_without_selecting_or_repeated_changes`
- `bottom_pane::subagent_status::tests::footer_hover_invalidates_scroll_reorder_resize_removed_and_root_rows`
- `bottom_pane::subagent_status::tests::footer_hover_discoverability_keeps_narrow_width_and_three_detail_rows`
- `bottom_pane::tests::footer_hover_requests_one_frame_per_row_transition_and_preserves_draft`
- `app::thread_routing::subagent_status_tests::footer_hover_public_inline_owned_routes_preserve_interaction_and_modal_priority`

위 시험 중 component 발견성·stale mapping·폭 사례 3개는 제품 구현 전에 소스에 추가했다. 같은 행에서 frame 요청 0개·이탈 시 1개인지를 기존 FrameRequester::test_channel로 확인하는 사례와 실제 App inline/owned MouseMoved·검색/선택·modal·활성 turn 사례를 추가했다. 한국어 Buffer oracle은 넓은 글자의 padding cell을 건너뛰고 실제 글자열을 읽는다.

직접 수행한 정적 보존 대조: 기존 panel 시험 전체, BottomPane 시험 전체, App 선택 시험 전체의 원문을 보존했다. 기존 assertion 삭제/완화가 없으며 active_reconnect_tests.rs 동결 SHA-256도 유지했다. UTF-8(no BOM)와 CRLF를 확인했다. Cargo·Rust compiler·rustfmt·시험·실제 OS hover 실행은 전부 NOT_RUN이다. 이 소스 후보나 추가 시험 선언을 실제 hover 완료/PASS로 해석하지 않는다.

동결 source/locale SHA-256:

| 파일 | SHA-256 |
| --- | --- |
| codex-rs/tui/src/bottom_pane/subagent_status.rs | 3da30c30a263c7545349e19fb45f7e9d92ed94e35e4ac459033376fdd18ecce9 |
| codex-rs/tui/src/bottom_pane/mod.rs | 4ea88909f2fc99624d040377755b714562fc9767d4cf048472cdf0766c1330b4 |
| codex-rs/tui/src/chatwidget/interaction.rs | 3c0b7db9b30132211529778239d766b3faa0ee117afe687dc511e6ca8dc14a22 |
| codex-rs/tui/src/app/owned_transcript.rs | 8032ee02d89f24d8baa0e763d55109ba186306978a395f2270c88a961caebf2c |
| codex-rs/tui/src/app/subagent_status_tests.rs | d05b5d1e2a1a50a63a6c5185d1b780a40b5e3b166e8af8c4cf284ae21fffc421 |
| codex-rs/tui/locales/en/r36-agents.ftl | 86e812337597b308b9bb19653840c64202e9833f6909ee17dcf037e6660e9017 |
| codex-rs/tui/locales/ko/r36-agents.ftl | 9281a798abb2ccea0c7ed78ac5dc93cdf939c661fa5467b171fc20757a078534 |
