<!-- Modified for OpenDIY Codex R37 (2026-11-04); see MODIFICATIONS.md. -->
# R37 CLI와 관리 daemon 선택 시험 계획

대상: R36 기준 `1b441423`에서 분기한 R37. 운영 패키지·daemon·사용자 데이터는 변경하지 않는다. `test-engineering`, `ponytail`, `cs-engineering`을 적용한다. 기존 Rust `tokio::test`/`tempfile` fixture와 daemon crate runner를 사용한다. Cargo 실행은 통합 담당자가 직렬 수행한다.

## 요구와 불변조건

- D01: 자동 시작에서 멈춘 구버전 `current`를 새 CLI가 다시 실행하지 않는다. 설치된 패키지는 자동 교체하지 않고, 기존 embedded/foreground 경로로 호출 CLI의 backend를 사용한다.
- D02: 실행 중이거나 시작 중인 공유 서버와 active/queued 작업은 검사 때문에 종료·재시작하지 않는다. 자동 관리 서버 선택에서는 실제 handshake 서버 버전이 호출 CLI의 전체 버전과 다르거나 미확인이면 현재 CLI의 embedded/foreground backend로 실행하며 이유를 알린다. 명시 remote 연결의 기존 호환 정책과 agents overview의 조회 대상은 유지한다.
- D03: 패키지 비교와 시작 선택은 기존 operation lock 아래 수행한다. 비교의 I/O 실패를 mismatch나 정상 일치로 바꾸지 않는다.
- D04: 명시 `update --from-cli --yes --no-force`만 기존 complete-package staging/검증/선택 경로를 사용한다. shutdown 미확인/timeout이면 기존 선택을 유지하며 자동 재시도나 강제종료를 하지 않는다.

CS 연결: `CONC-LOCK-003`의 기존 operation→installer lock 순서를 유지한다. `CONC-CANCEL-004`에 따라 shutdown 요청과 완료를 구분하며 기존 no-force receipt를 보존한다. 새 잠금·설치기·runner는 만들지 않는다.

확인한 lock 호출 그래프:

```text
start_with_features -> operation lock -> prepare(Missing)
  -> (최초 설치에 필요한 installer lock을 함수 반환 전에 해제)
  -> installer lock -> current 재조회 -> digest 비교 -> backend 시작/ready
update_from_cli -> 확인 -> operation lock -> dedicated installer lock -> legacy installer lock
migration -> operation lock -> installer lock
updater -> installer 자식 실행/종료(자식 lock 해제) -> try_restart_if_running
  -> operation try-lock -> (pruning에서 installer lock)
```

`prepare_install.rs`, `migration.rs`, `manual_update.rs`, `update_loop.rs`, `lib.rs`와 로컬 `scripts/install/install.sh`/`install.ps1`의 획득·반환 경계를 확인했다. 설치 스크립트는 installer lock 아래 선택을 바꾸며 operation lock을 기다리지 않는다. updater는 자식 종료를 기다린 뒤 재시작 lock을 얻는다. 최초 준비보다 먼저 바깥 installer lock을 잡으면 재진입 대기가 생기므로 준비 후 재획득한다. 외부에서 다운로드하는 설치 스크립트의 미래 버전까지 보장하는 분석은 아니다.

## 사례와 독립 oracle

| ID | 요구 | 입력·사전조건·공개 경계 | 기대 결과와 판정 | 수준 |
| --- | --- | --- | --- | --- |
| D-T01 | D01/D03 | 격리 HOME, stopped current에 CLI와 다른 바이트, `start_with_features`의 동일 내부 시작 경계 | `CliDaemonMismatch`, 양쪽 실제 경로와 명시 update 안내; package/settings/recovery/실행 marker 불변 | Rust component |
| D-T02 | D01 | 다른 물리 경로의 동일 바이트 CLI/selected | mismatch 없음; 경로 차이만으로 거부하지 않음 | Rust component |
| D-T03 | D01 | 최초 HOME, selected 없음 | mismatch 없음; 기존 최초 complete-package 설치 정책으로 진행 | Rust component/기존 prepare tests |
| D-T04 | D02 | existing ready socket, 서로 다른 CLI/selected; 합성 active/queue 데이터 | 검사 성공, 기존 서버 유지; 기존 호환성 경고·실제 task 보존은 통합시험으로 별도 확인 | Rust component + native integration |
| D-T05 | D02/D03 | managed starting PID reservation, 서로 다른 CLI/selected | 검사 성공; 시작 중 프로세스를 replacement 대상으로 삼지 않음 | Rust component |
| D-T06 | D03 | selected 파일 조회/CLI read 실패 | 원래 I/O 오류; typed mismatch 아님; start/selection 변경 없음 | Rust component |
| D-T07 | D01/D03 | installer lock 보유 중 CLI 시작을 poll한 뒤 설치자가 selected 바이트 변경 | 시작이 lock 해제 전 완료되지 않음; 해제 뒤 새 선택의 mismatch 탐지·recovery 보존; operation lock 대기는 기존 lock test로 별도 확인 | Rust component + native integration |
| D-T08 | D04 | 명시 complete CLI update, stopped 또는 idle server | 검증된 패키지 선택·pin, running이면 graceful 완료 후 교체; 동일 바이트/버전 확인 | 기존 prepare fixture + native integration |
| D-T09 | D02/D04 | 명시 no-force update, active/queue server 또는 graceful 미지원 | graceful-only만 요청; 미확인/timeout 구별; 선택 불변·강제종료/자동 재시도 없음 | 기존 backend/prepare tests + native integration |
| D-T10 | D01 | 실제 TUI/remote-control caller에서 typed mismatch; CLI와 selected 경로에 공백·한글 포함, en/ko 각각 선택 | 전체 CLI 시작 실패 대신 embedded/foreground; 양쪽 실행 경로를 원문 그대로 표시하고 불일치 원인·공유 설치본 보존·명시 `update --from-cli --yes --no-force` 안내 유지; 기존 shared server 미접속 | shared formatter unit + caller/native integration |
| D-T11 | D02 | 자동 관리 선택의 실제 `app_server_version`이 R28, CLI 전체 버전 R37; 디스크의 managed 버전은 임의로 다르게 설정 | managed 경로/버전으로 추정하지 않고 embedded/foreground 선택·실제 버전 경고; 기존 active/queue 서버 변경 없음 | caller unit + native integration |
| D-T12 | D02 | 실제 handshake 버전 없음 | 정상 일치로 해석하지 않고 미확인 이유와 함께 embedded/foreground 선택 | caller unit |
| D-T13 | D02 | 실제 handshake 버전과 CLI 전체 버전 일치; 다른 경로의 패키지 | 기존 관리 연결 유지; embedded를 강제하지 않음 | caller unit |
| D-T14 | D02 | 명시 `--remote`의 다른 버전 서버 또는 이미 embedded 선택 | 자동 관리 선택 guard를 적용하지 않음; 추가 daemon 시작/재시작 없이 기존 경로 유지 | 기존 명시 remote 시험 + native integration |
| D-T15 | D02 | 실제 R28 handshake와 managed PID reservation을 가진 격리 서버, autoUpdateEnabled true/false, 기존 updater empty PID receipt | 실제 `start_for_cli` 성공 후 actual R28을 보존하되 updater 관리 생략; receipt·설정·선택 불변. 이 guard를 제거하면 기존 stop이 stale updater receipt를 지우므로 실패해야 함 | Rust component/native integration |
| D-T16 | D02 | 자동 CLI 경로의 actual version 미확인/다름/정확 일치와 명시 lifecycle | 자동 미확인/다름은 updater 관리 생략, 자동 정확 일치와 명시 lifecycle은 기존 정책 유지 | Rust unit |

D-T10 경고 회귀 보완(시험 전): typed error의 `cli_path`/`managed_path`를 두 caller가 빈 i18n 인수로 버려 양쪽 실행 경로가 UI에서 누락됐다. 기존 i18n 카탈로그를 재사용하는 공유 경고 formatter를 두 caller에 연결한다. `i18n::tests::daemon_selection_mismatch_warning_preserves_paths_and_guidance_in_both_languages`는 공백·한글 Windows 경로와 en/ko 고정 원인·보존 문구, 명시 update 명령을 독립 oracle로 검사한다. 경로가 빠지거나 서로 바뀐 label에 표시되지 않아야 하며, locale마다 원문 경로를 보존해야 한다. 시작 선택·실제 버전 guard·명시 update 정책은 변경하지 않는다. 새 unit의 실제 실행과 red/green은 Cargo 금지로 **NOT_RUN**이며 기존 native 시험 결과를 이 보완의 PASS로 사용하지 않는다.

기대값은 설치·실행 경로 계약과 고정 fixture 바이트에서 정한다. digest 구현을 복제해 기대값을 계산하지 않는다. 함수 단위 검사는 Windows 실제 실행본, TUI 렌더, 실제 active/queued 보존, Desktop 공동시험을 대신하지 않는다.

## 실행 계획과 현재 결과

### 실행 검사 순서 결함과 사전 회귀 사례

`daemon-tests-04-valid-pe`의 실제 결과를 직접 읽었다. full active-old 사례는 유효 PE 입력에서 updater false/true 모두 완료했고, suite는 6 PASS/2 FAIL/exit 101이었다. 실패는 stopped mismatch의 typed error 부재와 installer lock 대기 전에 끝난 시작이었다. 이전 invalid fixture 실행에서 Windows가 지원되지 않는 16비트 응용프로그램 창을 표시했다는 사용자 관측도 root에게 인계받았다. 비 PE 데이터를 실제 실행 경계에 둔 시험기 잘못을 보존하며 이 기록을 새 PASS로 덮지 않는다.

확인한 source 순서: `prepare_install::prepare(Missing)`의 existing-current 분기는 `ensure_managed_codex_bin`을 호출하고, 이 함수는 Windows에서 `ensure_detached_launch`로 선택 실행 파일을 실제 probe한다. 새 automatic CLI digest guard와 installer lock보다 앞이다. 따라서 stopped mismatched binary가 guard 전에 실행 검사되며, mismatch 오류·installer 대기를 검증하려던 두 사례가 먼저 다른 오류로 끝났다.

최소 변경 계약: 기존 file 존재 검사와 Windows 실행 probe를 분리한다. prepare의 existing-current에서는 같은 파일 존재 검사와 같은 오류 문구를 유지하되 실행하지 않는다. 호출자의 나중 `ensure_managed_codex_bin`은 계속 Windows probe를 수행한다. automatic CLI 경로는 operation→installer lock→최신 current→digest guard→Windows probe/시작 순서다. 최초 설치 prepare는 installer lock을 반환 전에 해제하며 재진입 lock을 만들지 않는다. 명시 lifecycle의 실제 실행 검사와 설정·recovery 정책을 지우지 않는다.

- D-T17: stopped mismatch와 installer wait의 실제 `start_for_cli` fixture는 현재 test executable의 유효 PE 복사본만 사용한다. 서로 다른 identity는 PE overlay로 만든다. 모든 파일의 streamed identity snapshot을 전후 비교한다. typed mismatch가 Windows probe보다 먼저 발생하고 선택·settings·recovery·PID가 보존되어야 한다.
- D-T18: installer가 lock 아래 새로운 유효 PE overlay identity를 선택하면 시작은 lock 해제 뒤 그 최신 identity와 CLI 차이를 판정해야 한다. installer가 선택한 파일의 최종 identity도 이후 보존되어야 한다.
- D-T19: 실제 `prepare` 호출자 전부에서 preparation 뒤 file 검사만으로 launch하지 않고 기존 실행 검사를 거치는지 정적으로 대조한다. 최초 설치·existing install·명시 restart/bootstrap의 기존 시험도 회귀 확인한다.

위 순서 변경과 새 executable fixture의 실제 실행은 아직 NOT_RUN이다. 비교만 하는 component fixture의 text는 실행 경계에 전달하지 않는다. Windows 창을 막기 위해 실제 실행될 수 있는 fixture에 text `.exe`를 다시 사용하지 않는다.

정적 호출 대조(이번 후보, daemon crate 전체 검색): `prepare_install::prepare`의 production 호출은 3곳이다. `lib.rs`의 `start_with_overrides`는 prepare 563→automatic installer lock/identity guard→실행 검사 592→시작 605(기존 설치 전용 경로는 같은 검사 후 PID backend 시작), `restart_with_settings`는 prepare 636→실행 검사 639→시작 660, `bootstrap_locked`는 prepare 1006→실행 검사 1009→backend 시작 1038이다. 따라서 file-only 준비 뒤 실제 실행 검사를 생략하는 production 호출자는 이 3곳 검색에서 0개다.

명시 restart에서는 실행 검사를 updater 중지보다 앞으로 두어, 기존 prepare 단계의 조기 실행 검사 실패가 updater 중지보다 먼저였던 안전 순서를 보존했다. bootstrap의 검사도 settings 저장·기존 backend 중지보다 먼저다. 별도 명시 package replacement 경로의 `prepare_from_package`는 Missing-only file 검사 변경 대상이 아니며, 실행 중 backend 교체 전에 release의 `ensure_detached_launch`를 직접 수행하는 기존 경계를 유지한다. Missing 모드의 신규 staging은 backend 없음 조건을 검증하므로 prepare 자체가 실행 서버를 띄우는 경로가 아니다.

- `cargo test -p codex-app-server-daemon launch::tests` — 신규 선택 검사.
- `cargo test -p codex-app-server-daemon` — 기존 first/existing install·no-force·updater·PID 회귀.
- 통합 담당자는 새 후보로 실제 CLI 자동 시작 fallback, `--no-daemon`, 명시 complete-package update, active/queued 보존을 격리 HOME에서 확인한다.
- 계획 작성 시 새 동적 시험은 **NOT_RUN**이었다. 아래 첫 실행 결과와 보완 상태를 구별한다. Cargo와 실제 실행본 시험은 통합 담당자 소유이며 과거 R36 PASS를 재사용하지 않는다. red/green/mutation은 허용 runner로 실제 실행한 쌍만 기록한다.

### 첫 실행의 fixture 실패와 보완

통합 담당자가 실행한 `daemon-tests-01`은 exit 101이었다. 신규 6개 사례 모두 Windows 시험 상태 폴더의 상속 ACL 때문에 `socket directory is not private to the current user`로 fixture 준비 중 실패했다고 인계받았다. 제품의 mismatch/시작 판정에 도달한 근거는 0개이며 이 결과를 제품 로직의 red 또는 성공으로 해석하지 않는다.

원인과 보완: 합성 `app-server-daemon` 폴더를 `std::fs::create_dir_all`로 먼저 만들면 제품 API가 기존 비공개 폴더로 인정하지 않는다. 기존 daemon/PID 시험과 동일하게 `codex_uds::prepare_private_socket_directory`가 아직 없는 자기 fixture 폴더를 owner-only ACL로 생성하게 바꾼다. 제품의 ACL 검사를 완화하거나 운영 경로의 ACL을 바꾸지 않는다. 보완 뒤 재실행 결과는 아직 **NOT_RUN**이며 첫 실패 기록은 보존한다.

### 두 번째 실행의 정지와 새로운 판별 검사

통합 담당자가 직접 확인한 `daemon-tests-02`는 컴파일을 마쳤고 정책 unit 1개 통과 후 `automatic_old_server_preserves_updater_state_for_both_update_settings`에서 정지했다. 담당자가 자기 runner/자식의 소유 관계를 확인하고 시험만 취소했으며 receipt exit 130과 `timeout-observation.json`을 보존했다고 인계받았다. 이 결과를 제품 로직 실패나 8개 통과로 세지 않는다.

새 source 확인: `InitializeResponse.codex_home`은 `AbsolutePathBuf`인데 fixture가 상대 문자열 `synthetic`을 보냈다. 또한 `DaemonSettings::save`는 updater 설정을 새로 쓰지 않으므로 false/true 전제는 실제 파일과 달랐다. 다음 한 번의 검사는 절대 HOME으로 실제 DTO 역직렬화를 사전 검증하고, raw 합성 updater 설정을 쓴 뒤 실제 load 값으로 전제를 확인한다. 선언하지 않은 capability는 지원으로 주장하지 않는다.

fixture의 accept, WebSocket handshake, initialize request/response, initialized notification, 시작 경계와 서버 task 완료에 단계별 stderr marker와 timeout을 둔다. 시험이 실패해도 서버 task는 소유 guard의 Drop으로 abort하고 listener를 해제한다. Windows의 취소 불가능한 native blocking I/O까지 async timeout으로 끝났다고 주장하지 않으며, 외부의 소유 runner timeout도 유지한다. 이 변경 뒤 실행은 **NOT_RUN**이다.
