# R38 Stop 설정 교정 시험

## engineering-test-plan — 실행 전 계획

대상은 `scripts/install/normalize_stop_hook_config.py`의 함수와 CLI다. 기존
`scripts/install/test_install_sh.py`와 같은 표준 라이브러리 `unittest`를 사용한다.
2026-11-06 구현·시험 전에 사례를 작성했다. 실제 사용자 설정, R37, 운영 DB,
다른 세션의 로그·인증 파일은 열거나 수정하지 않는다. 기존 설치 스크립트에는 연결하지 않는다.

요구 R1: 명시한 파일의 `hooks.Stop[*].hooks[*].additionalContextLimit`만 제거한다.
R2: 기본 dry-run이며 `apply=True`/`--apply`만 쓰기를 허용한다.
R3: 적용 전 원본 그대로의 새 백업을 만들며 기존 백업을 덮어쓰지 않는다.
R4: 계획 후 내용·파일 신원이 바뀌거나 다른 작성자가 사용 중이면 적용을 거절한다.
R5: 인코딩·BOM·줄바꿈과 제거 대상 외 원문을 보존한다.
R6: 파싱·검증·백업 실패 시 원본을 변경하지 않는다.

공통 전제: Windows, 네트워크를 사용하지 않는 자체 `TemporaryDirectory`, 직접 만든 JSON만 사용한다.
실제 파일 I/O 및 CLI subprocess를 검사하며 배타 핸들 충돌도 실제 핸들로 재현한다.
인위적 변경 시점·백업 실패만 mock으로 주입한다. JSON 기대값은 수기로 작성하고,
바이트 보존은 수기 문자열 또는 원본 바이트와 비교한다. 실패는 assertion 실패와 실행 오류를 구분한다.

| 사례 | 요구 | 입력·전제 | 절차 | 기대 결과 / pass-fail oracle | 실제 결과 |
| --- | --- | --- | --- | --- | --- |
| SH01 | R1,R2 | Stop handler에 값 8192 | 기본 함수 호출 | 제거 예정 수 1, 원본 바이트 동일, 백업 없음 | PASS |
| SH02 | R1,R3 | 여러 Stop handler, 다른 event와 중첩 동명 필드 | 명시 apply | 지정 필드만 제거된 수기 JSON과 일치, command/env/timeout/unknown와 다른 event 값 유지, 백업은 원본 바이트 | PASS |
| SH03 | R1,R5 | 대상 속성이 첫째·중간·마지막·유일 속성, compact·pretty JSON, escaped key | 각각 apply | 유효 JSON, 남는 값과 수기 원문 일치 | PASS (7입력) |
| SH04 | R5 | UTF-8/BOM, UTF-16 LE/BE/BOM, UTF-32 LE/BE/BOM, LF/CRLF 혼합 | 각각 apply | 원본 codec/BOM 유지, 제거한 key/value/comma 외 바이트 동일 | PASS (10입력) |
| SH05 | R1,R2 | 대상 없음, 다른 event에만 동명 필드 | 각각 apply | 제거 수 0, 원본 동일, 불필요 백업 없음 | PASS (3입력) |
| SH06 | R6 | 잘못된 JSON, duplicate key, NaN, 잘못된 encoding | 각각 apply | 명시적 오류, 원본 동일, 백업 없음 | PASS (6입력) |
| SH07 | R1,R6 | 비객체 root 또는 잘못된 Stop/hooks/handler shape | 각각 apply | 명시적 오류, 원본 동일, 백업 없음 | PASS (6입력) |
| SH08 | R4 | dry-run digest 이후 내용 변경 | 이전 digest로 apply | concurrent/stale 오류, 새 내용 보존, 백업 없음 | PASS |
| SH09 | R4 | 읽은 snapshot 뒤 파일 변경 또는 같은 bytes의 파일 교체 | apply의 배타 open 직전 변경 주입 | 동시 변경 오류, 변경본 보존, 백업 없음 | PASS (2입력) |
| SH10 | R4 | 다른 실제 handle이 파일을 배타적으로 열고 있음 | 실제 Windows handle 유지 후 apply | 최초 읽기 EACCES / 직접 open WinError32, 원본 동일, 백업 없음 | PASS (oracle 교정 후) |
| SH11 | R3,R6 | 백업 생성 실패 | apply에 백업 실패 주입 | I/O 오류, 원본 동일 | PASS |
| SH12 | R2 | 자체 fixture 경로만 CLI 전달 | CLI 기본 실행, 이후 --apply | 기본 exit 0와 원본 동일, 명시 apply exit 0와 교정·백업, 출력에 command/env 원문 없음 | PASS |
| SH13 | R6 | 없는 경로, directory, symlink 판별 branch | 각각 함수/CLI 호출, symlink 판별만 mock | 명시적 오류, 새 설정 생성 없음. 실제 symlink 생성은 권한을 가정하지 않아 NOT_RUN | PASS (명시한 component 경계) |
| SH14 | R1,R3 | 이미 교정된 동일 파일 | 명시 apply 두 번 | 첫 백업 보존, 둘째 제거 수 0와 새 백업 없음 | PASS |
| SH15 | R4 | 적용 중 다른 작성자 open 시도 | 백업 작성 시점에 실제 두번째 writer open | Windows 공유 위반, 정상 교정과 원본 백업 | PASS |
| SH16 | R4,R6 | 쓰기용 Windows CreateFileW 호출 | API 호출만 spy로 캡처하고 오류 반환 | FILE_FLAG_OPEN_REPARSE_POINT·share=0·OPEN_EXISTING 전달. 성공 경로 실제 핸들은 SH10/15에서 별도 확인 | PASS |
| SH17 | R4,R6 | 자체 fixture symlink 생성 권한이 있는 경우 | initial is_symlink와 rb open 사이 링크 교체 재현 | 쓰기 전에 reparse/신원 불일치 거절, 원본·링크 대상 불변, 백업 없음. 초기 읽기의 링크 추종 여부는 별도 관측 | NOT_RUN: WinError1314로 SKIP |

재현 명령(`$verifiedPython`에는 미리 확인한 Python 실행 파일의 절대 경로를 지정한다):

```powershell
& $verifiedPython -I -S -B -X utf8 './scripts/install/test_normalize_stop_hook_config.py' -v
```

## engineering-test-proof — 사전 한계

새 독립 helper이며 기존 운영 설정을 바꾼 결함 버전은 실행하지 않는다.
기계적 red/green 대신 실제 JSON 경계·백업·sharing 위반·stale 입력을 위 사례로 확인한다.
운영 설정 적용, 설치 연결, 앱 실행, Stop hook 실행·종료 판정과 warning 소멸은 NOT_RUN이다.
적용은 Windows 배타 핸들 아래에서 원본 백업 후 같은 파일에 쓴다. 쓰는 도중 프로세스/디스크
장애의 crash atomicity는 보장하지 않으며 남겨둔 원본 백업으로 복구해야 한다.

## engineering-test-result

첫 실행은 위 명령, exit 1, `Ran 15 tests in 0.428s`, SH01–09·SH11–15 통과,
SH10 assertion 실패였다. 실제 거절은 발생했으나 Python 최초 `open("rb")`의
`PermissionError(errno=13, winerror=None)`를 직접 `CreateFileW` 오류와 같다고
가정한 oracle 오류였다. 동일한 실패를 제품 결함의 red로 보지 않는다.

원시 traceback은 로컬 검증 자료에 보존하고 공개 소스에는 포함하지 않는다.
실패 assertion은 기대한 WinError32와 실제 `None`의 차이였다.

SH10은 최초읽기의 EACCES와 직접 배타 open의 WinError32를 각각 확인하도록
교정했다. 이후 결과는 아래에 별도 기록한다.

두번째 실행: 같은 절대 인터프리터·argv, exit 0.

두번째 실행에서 15개가 통과했다(0.359초).

사례별 실제 결과는 위 표와 테스트의 같은 SH ID로 연결된다. SH10의 두 handle은
같은 프로세스 안에서 열었으며 OS sharing 판정은 실제로 실행했다. 별도 프로세스가
파일을 보유하는 시험은 NOT_RUN이다. 실물 symlink와 crash/power-loss 복구 시험도
NOT_RUN이다. 모든 fixture는 자체 빌드 영역의 임시 폴더에만 만들었고
`TemporaryDirectory`가 정상 반환했다. 원시 실행 출력은 로컬에 보존하며,
운영 파일과 기존 백업은 이 시험의 입력이 아니다.

## 후속 reparse 경계 확인

추가 사례 SH16/17을 먼저 표에 적고 세번째 실행을 수행했다. helper 제품 코드는
변경하지 않았다. 기존 `CreateFileW`의 `dwFlagsAndAttributes=0x00200080`은
`FILE_FLAG_OPEN_REPARSE_POINT(0x00200000) | FILE_ATTRIBUTE_NORMAL(0x80)`이다.
실제 호출 인자를 캡처한 SH16이 이 플래그, share=0, OPEN_EXISTING을 확인했다.
쓰기 핸들의 `fstat` reparse 속성·파일 신원·바이트 대조는 백업 이전에 실행된다.

반면 최초 `Path.is_symlink()`와 `Path.open("rb")` 사이 교체는 초기 읽기에서
링크를 따라갈 수 있는 코드상 간격이다. 이 문제를 쓰기 핸들의 신원 대조가 초기
읽기까지 보호한다고 설명하지 않는다. 자체 fixture의 실물 symlink 생성 시도가
`WinError1314`(권한 부족)로 실패해 실제 경쟁 경계는 재현하지 못했다. 권한 확대나
대체 운영 경로로 검사하지 않았으며 이 초기 읽기의 경쟁 방지는 미검증·남은 제한이다.

세번째 실행: 위와 같은 절대 인터프리터·argv, exit 0. 16 PASS / 1 SKIP이며
17개 전체 통과로 계산하지 않는다.

전체 17개 중 SH16을 포함한 16개가 통과했고 SH17은 권한 부족으로 실행하지
못했다(0.771초). 원시 출력은 로컬 검증 자료로 보존했다.

이 산출물은 아직 독립 helper다. R38 installer 연결과 운영 설정 교정의 완료를
뜻하지 않는다.

## 추가 요청: reload_plugin 진단과 표시 — 실행 전 계획

추가 요구 PR-R1: unsupported Stop handler 속성 경고에 hook 자체가 유지되고 해당
handler의 `additionalContextLimit`만 제거하면 된다는 안내를 넣는다. 경고 원문·경로는
유지하고 경고 억제·전체 hook 비활성·자동 설정 수정은 하지 않는다.
PR-R2: slash 명령의 결과·partial warnings·invalid acknowledgement·실패·stale 안내를
기존 i18n message API와 두 `cli.ftl`로 제공한다. 서버 diagnostic/path는 번역하지 않는다.
PR-R3: 기존 embedded-server fixture로 실제 reload의 경고 전달·재연결 요청과 파일
불변을 검사하고, fixture에서 대상 속성만 명시 제거한 다음 reload의 경고 해소와 hook
유지를 검사한다. MCP readiness와 reconnect 요청을 같은 것으로 판정하지 않는다.

공통 전제: canonical R38 소스만 편집하며 운영 설정·R37·daemon·인증·다른 세션
로그는 접근하지 않는다. 이번 추가작업은 실행시험·Cargo·빌드·commit·push 금지이므로
아래 사례를 작성해도 실제 결과는 NOT_RUN으로 남긴다. 기존 helper 시험 결과를 이
추가 변경의 시험 결과로 전용하지 않는다. 기존 파일의 CRLF/LF를 각각 유지한다.

| 사례 | 요구 | 전제·입력 | 절차 | 기대 결과 / oracle | 실제 |
| --- | --- | --- | --- | --- | --- |
| PR01 | PR-R1 | discovery의 Stop command handler에 unsupported 속성 | 기존 discovery 단위사례 확장 | handler/entry command 유지, 경로와 기존 diagnostic 및 속성만 제거하라는 안내 존재 | NOT_RUN |
| PR02 | PR-R2 | EN/KO locale, 원시 서버 경고와 Windows 경로 | 같은 결과 formatter에 warning 유무 입력 | 각 locale 결과·partial 안내, 경고 원문/경로 byte-equivalent 문자열 유지, 준비 미확인 명시 | NOT_RUN |
| PR03 | PR-R3 | 자체 hooks.json에 Stop handler 속성8192, 다른 event·command·env·timeout | 기존 실제 embedded fixture로 targeted reload 후 hooks/list | 원 thread ACK/reconnect 요청 true, warning 전달, readiness 단정 없음, 설정 bytes 동일, hook list에 Stop handler 유지 | NOT_RUN |
| PR04 | PR-R3 | PR03과 동일 fixture의 해당 속성만 명시 제거 | 두번째 targeted reload와 hooks/list | 해당 warning 해소, 원 thread ACK/reconnect 요청 true, 교정본 bytes 불변, 다른 모든 JSON 값과 Stop command 유지 | NOT_RUN |
| PR05 | PR-R2 | unsupported backend·invalid ACK·stale 화면 소유권 | 기존 실패/stale 사례와 message catalog 기대값 보강 | 각 locale 안내와 오류 원인 보존, stale일 때 RPC 0, reconnect 요청을 readiness로 표현하지 않음 | NOT_RUN |
| PR06 | PR-R2 | 자연어 session-control ReloadPlugins 경로 | slash와 동일 formatter 재사용 | 결과·경고·invalid ACK·오류가 같은 locale 계약을 따름 | NOT_RUN |

예정 필터: `codex-hooks unsupported_event_warns_and_ignores_additional_context_limit`,
`codex-tui app::tests::plugin_reload_tests` 및 카탈로그 기존 parity 검사. 이번 작업에서
해당 명령을 실행하지 않는다. 기존 `start_recording_app_server_with_history`는
실제 embedded server로 요청을 전달하므로 새 서버 harness를 만들지 않는다.

작성한 구체 필터는 `reload_plugin_reports_unsupported_stop_setting_without_removing_the_hook`
(PR03/04, 직접 ACK·slash 표시를 교정 전후 각각 확인),
`reload_plugin_messages_localize_without_translating_server_diagnostics`(PR02/05),
기존 `reload_plugin_routes_captured_thread_and_unsupported_backend_fails_closed`와
`reload_plugin_and_fast_queued_before_screen_or_backend_replacement_sends_zero_rpcs`다.
PR01은 기존 discovery 필터에 handler entry/command와 구체 교정 안내 assertion을
추가했다. PR06은 자연어 경로가 동일 formatter를 쓰도록 수정했으며 별도 자연어 RPC
실행은 하지 않았다. invalid ACK의 메시지는 카탈로그 사례이고 서버가 잘못된 ACK를
반환하는 실서버 fixture는 추가하지 않았다.

새 actual fixture는 app 소유 TempDir와 `codex_home` 일치를 먼저 검사한다.
직접 ACK 확인과 slash 표시 확인으로 교정 전후 총 4회의 reload와 2회의 hooks/list를
요청하도록 작성했다. JSON 교정은 fixture handler의 대상 key 하나만 명시 제거한다.
공유 history helper는 80열로 줄을 나누므로 원시 진단 문자열 assertion에는 같은 실제
history cell의 4096열 렌더링을 사용한다. 좁은 화면의 시각 레이아웃 시험은 별도다.

작성 시 정적 확인은 소유 파일 `git diff --check` exit 0이며, 이번 추가 변경의
컴파일·시험·실제 앱 검증은 실행하지 않았다. 기존 helper의 PASS/SKIP와 이 추가
요구의 NOT_RUN을 합산하지 않는다. 운영 hooks.json의 경고가 해소됐다고 보고하지 않는다.


## 재로드 추가 요구의 Windows 개발 결과

`codex-hooks additional_context_limit` 2개와 통합 4차 `codex-tui reload_plugin` 6개, catalog 8개가 exit0이다. PR01·PR02와 실제 embedded server의 PR03·PR04는 해당 원시 assertion을 확인했다. Stop handler의 동일 identity·command·timeout·enabled 상태, 원 thread ACK, 설정 bytes 불변과 대상 속성 제거 후 해당 warning 해소를 확인했다. PR05의 unsupported/stale 경계는 기존 실제 경로이고 invalid ACK는 catalog 검사다. PR06은 같은 formatter 재사용을 정적으로 확인했으며 별도 자연어 RPC와 잘못된 ACK를 보내는 실서버 시험은 NOT_RUN이다. 이 차이를 전체 경로 PASS로 확대하지 않는다. 운영 hooks.json은 수정하지 않았다.
