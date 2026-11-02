# R33 graceful 완료 통지 경계 수정 후보의 검증 범위

현재 고정 소스는 `36ae395a552fd55962e19409cbf29b1fdf03b0ec`, 이전 공개 기준은 `5300c7238a23cd29d29e4ffdfbcaf3ac53366886`다. 이전 `3d9` 후보에서 달라진 제품 소스는 `bespoke_event_handling.rs`, `outgoing_message.rs`, `thread_status.rs`의 3경로다. 완료·중단 알림의 쓰기 확인이 끝날 때까지 drain 책임을 유지하고 기존 idle 알림 순서를 보존하는 후속 개발 후보다. 제품 버전은 `0.166.0+native-r33.20261102`를 유지한다.

개발용 시험 컴파일은 종료 코드 0으로 완료했고 표적 시험 34개가 중복 없이 통과했다. 개발용 서버 컴파일은 진행 중이며 새 후보의 runtime·Desktop 공동 시험·최종 독립감사는 미완료다. 새 release 빌드·패키지 생성은 시작하지 않았다. 이전 `3d9` package03의 graceful 완료 통지 누락과 최종 독립감사 FAIL 기록은 그대로 보존하며 전체 수용·릴리스·설치를 보류한다. 이전 후보의 시험·빌드·부분 Desktop 공동 시험 통과를 새 후보의 통과나 결함 해소로 확대하지 않는다.

## 현재 후보의 동결과 red 근거

동결 목록은 54경로이며 SHA-256은 `3d3171fe1058f04f540343deb61f4ccd9ffe946814de8ceaa77d3021848ab084`다. 원본 Windows 작업파일 54개와 줄바꿈을 정규화한 고정 Git blob 54개를 직접 대조했다. 개발용 시험 컴파일은 2026-11-02 12:30:28 UTC에 종료 코드 0으로 완료됐고 Cargo가 보고한 소요 시간은 1분 25초였다.

회귀시험을 먼저 추가한 `dafb5af70c5d4eb4f43ff1075e859c6b72b2301b`의 같은 시험 실행 파일(SHA-256 `e208da3986bb9584baa5bf513a7a66c118207dc75c2603e03e758d2acd05c537`)로 다음 exact 시험을 실행했다. 프로젝트의 `RUST_MIN_STACK=8388608` 조건을 적용한 두 실행의 receipt·stdout·stderr를 대조했다.

| red 실행 | 통과 | 실패 | 종료 코드 |
| --- | ---: | ---: | ---: |
| `turn_complete_stays_running_until_terminal_write` | 0 | 1 | 101 |
| `turn_interrupted_stays_running_until_terminal_write` | 0 | 1 | 101 |

두 실행은 terminal 쓰기 확인을 보류한 상태에서 drain count가 기대값 1 대신 0이 되는 assertion에서 실패했다. 스택 조건을 적용하지 않아 stack overflow로 종료한 앞선 실행은 유효한 red 근거로 세지 않는다. 이 red 결과를 후속 후보의 green 검증으로 대체하지 않는다.

## 현재 후보의 표적 green 시험

개발용 시험 실행 파일 SHA-256은 `a03f1c4d8f3e95d15548c18387056ac425bb8ba776fa3673d677589b782b2fd4`다. 컴파일 receipt와 두 실행 receipt의 소스 pin·실행 파일 해시를 연결하고 stdout·stderr 해시 및 통과한 시험 이름을 직접 대조했다.

| 표적 범위 | 통과 | 실패 | 종료 코드 |
| --- | ---: | ---: | ---: |
| 완료 알림 쓰기 보류 exact | 1 | 0 | 0 |
| 중단 알림 쓰기 보류 exact | 1 | 0 | 0 |
| terminal sender | 4 | 0 | 0 |
| thread status | 20 | 0 | 0 |
| 완료 payload | 3 | 0 | 0 |
| 중단 payload | 1 | 0 | 0 |
| graceful 재요청 exact (보완 실행) | 1 | 0 | 0 |
| admitted request·force exact (보완 실행) | 1 | 0 | 0 |
| turn admission (보완 실행) | 2 | 0 | 0 |

첫 실행의 6그룹에서 30개가 통과했고 마지막 `lib_tests::` 필터는 0개를 선택했다. 첫 runner의 전체 종료 코드는 1이며 이 빈 필터를 검증 오류로 보존한다. 제품 시험 실패나 통과한 시험으로 세지 않는다. 올바른 shutdown exact 2개와 admission 2개를 별도 실행해 4개 통과·종료 0을 확인했다. 두 실행에서 통과한 시험 이름은 중복 없이 총 34개이며 위 red 2개와 같은 exact 선택도 green에서 각각 통과했다. 이 결과는 개발용 library의 표적 경계에 한정하며 전체 시험·새 제품 패키지·실제 drain runtime·Desktop 조합·최종 감사의 통과를 뜻하지 않는다.

## 이번 공개 소스의 출처

source 9,412개·공식 대비 수정 558개를 고정 Git blob에서 수집한다. 기존 R32 기본 고지와 R33 개별 고지 51경로를 보존하고 이번 3경로를 더해 R33 고지는 총 54경로다. 원본 Apache 고지·LICENSE·NOTICE와 내장 프롬프트 38개의 원본·Windows 바이트 해시는 유지한다. 프롬프트 catalog의 source commit만 현재 후보에 연결한다. 원시 로그·실행 파일·개인 경로는 공개하지 않는다. README는 기존 구조를 유지하면서 후보와 검증 범위를 갱신하고 루트 문서 해시에 다시 결속한다. 출처 검사는 제품 컴파일·실행이나 릴리스 승인을 대신하지 않는다.

## 이전 3d9 후보 기록

현재 고정 소스는 `3d9eb86445925caa920dd2f958496cf1aa4ad7e2`, 이전 공개 기준은 `b1a8780e283c3f3514387ec209ded2d78c6e91b7`이다. 이전 private 후보 `0c588be6c85098f0ff139ec7a9bc119c1da71f20` 이후 추가·변경은 7경로이고 새 파일은 1개다. Windows 공통 spawn의 표준 핸들 전달을 `HANDLE_LIST`로 제한하는 `windows_spawn.rs`와 관련 PID·분리 실행 fixture를 포함한다.

앞선 package02의 중첩 launcher PIPE EOF 경계는 실패했다. 현재 후보의 개발용 시험 컴파일은 종료 0이고 daemon 전체 66개 통과·0개 실패·native fixture 1개 무시, nested stdio 개별 재실행 1개 통과를 확인했다. 최적화 CLI·앱 서버·launcher 빌드와 package03 생성·해시 결속을 확인했다. 자연 EOF 8경계·격리 GUI 복원 범위의 Desktop 공동 시험은 통과했으나 graceful 완료 통지 실패로 전체 수용·릴리스·설치를 보류한다. 최종 독립감사는 이 차단 결함으로 FAIL이다. 실행 패키지 공개·실제 예약 등록·모바일·장기 부하는 미검증이다. 아래 이전 후보의 73개 시험과 PowerShell·패키징·전달 준비 결과를 현재 후보 전체의 통과 근거로 확대하지 않는다.

## 최적화 빌드와 package03 준비

고정 소스 `3d9eb86445925caa920dd2f958496cf1aa4ad7e2`의 최적화 빌드는 2026-11-02 19:24 KST에 종료 0으로 완료됐고 Cargo가 보고한 소요 시간은 133분 06초였다. 전달 receipt와 빌드 receipt의 source pin·해시를 대조했으며 실제 패키지의 실행 파일 3개, ZIP 안의 같은 실행 파일 3개, manifest와 ZIP 해시가 일치했다.

| 산출물 | SHA-256 |
| --- | --- |
| CLI `codex.exe` | `9929bfee9aa214f3201e64d9b0c8d3f8bc2b23b4958f2653e14407c9ebfbc3b7` |
| `codex-app-server.exe` | `84cc6ea86b032a9d51223856080ff874562ae5b352723a5d80f4e30a7564ae4c` |
| `opendiy-codex-launcher.exe` | `e46d052f1a507f6cb9c54bd70ed31403f695bdaeda0077934ee0ebaa63a665b0` |
| package03 `package-receipt.json` | `f5375798f23b04fa17a69bd7dbf8ccfe200d6cb5b126720a852e138a3d591436` |
| package03 ZIP | `5f22fe74a312a584d7fc9538c6d75facdd843727aa0b44d88bfb6fb192966b8a` |

패키지 생성 시점의 전달 receipt에서 `runtimeVerified`, `installed`, `published`는 모두 `false`였다. 후속 package03 graceful drain 검사 c3에서는 종료 요청 ACK 3개가 일치했지만 `turn/completed`가 관측되지 않은 채 서버가 종료 코드 0으로 끝나 FAIL이었다. 실제 패키지 서버의 SHA-256도 위 값과 일치했다. 제품 빌드·패키지 생성은 완료했다. 자연 EOF 8경계·격리 GUI 복원 범위의 Desktop 공동 시험은 통과했으나 graceful 완료 통지 실패로 전체 수용·릴리스·설치를 보류한다. 최종 독립감사는 이 차단 결함으로 FAIL이다. 실행 패키지 공개·실제 예약 등록·모바일·장기 부하는 미검증이다. 서버의 종료 코드 0이나 빌드·패키지 준비 성공만으로 결함 해결을 보증하지 않는다. 실행 파일·ZIP·원시 로그·로컬 경로는 공개 소스에 추가하지 않는다.

## 현재 후보의 개발용 시험

개발용 시험 컴파일 receipt와 실행 receipt의 소스 pin, 실행 파일 SHA-256, 두 실행의 stdout·stderr 해시와 실제 로그를 직접 대조했다. 시험 실행 파일 SHA-256은 `af6a00968670a718129761a2430c1298ce4a810e03445f45b330139bb39cf7e3`이다.

| 실행 | 통과 | 실패 | 무시 | 종료 코드 |
| --- | ---: | ---: | ---: | ---: |
| daemon library 전체 | 66 | 0 | 1 | 0 |
| `captured_stdio_closes_while_child_is_alive` exact 재실행 | 1 | 0 | 0 | 0 |

개별 재실행은 전체 시험과 겹치므로 고유 시험 67개로 합산하지 않는다. 최종 실패 spawn fixture는 독립적인 표준 `Command` 초기화 뒤 기준을 잡고, helper를 미리 호출하지 않은 채 첫 호출부터 32회 각각 핸들 증가 0을 검사한다. 진단 후보의 두 실행은 각각 종료 101이었고, 독립 표준 spawn만으로 helper 호출 전에 핸들 수가 128에서 136으로 늘었다. 이후 진단의 1·2·32·64회 관측값은 136이었다. 일시 계측 코드는 최종 파일에 포함하지 않았다.

51경로 동결 목록 SHA-256은 `b7a7559aaf0243b719c17dfe5f2ae80da63247cd4ae1e14b393d881a469c70a2`다. 목록의 원본 Windows 작업파일 해시 51개와 줄바꿈을 정규화한 고정 Git blob 51개를 대조했다. 이 개발 시험은 앞선 package02 실패를 새 제품 패키지에서 다시 검증한 결과가 아니다.

## 저장된 이력의 standalone stdio 검증

앞선 package02의 별도 앱 서버 실행 파일로 실제 `turn/start` 3회와 합성 로컬 응답을 통해 3턴을 저장하고, 첫 서버를 정상 종료한 뒤 새 프로세스에서 같은 스레드·턴·전체 내용을 복원했다. 실행 결과의 harness 판정은 15개 통과·0개 실패다. 이 시험 실행 파일의 SHA-256은 `0d655230b077db31d9bb221c9fc8891b029546f9362eb4be34fb45b96aa22252`이다.

재시작 후 `thread/items/list(limit=1)`은 항목 1개를 반환했고 `thread/items/read`는 같은 항목 payload를 반환했다. 원시 합성 결과의 두 성공 응답을 직접 대조했다. 앞선 unmaterialized probe의 `not supported yet`는 해당 상태에서의 결과이며, 저장된 이력까지 일반적인 item API 미지원으로 해석할 수 없다.

이는 standalone stdio와 격리한 합성 provider의 저장·복원 경계다. Desktop GUI·중첩 launcher·PIPE EOF 수정·외부 인증과 실제 모델 응답·장기 부하는 검증하지 않았다. 원시 프로토콜 결과·실행 파일·테스트 홈과 개인 경로는 공개하지 않는다.

## 이번 공개 소스의 출처

source 9,412개·공식 대비 수정 557개를 고정 Git blob에서 수집했다. 기존 R32 기본 고지·앞선 R33 고지 46경로를 보존하고 새로 지정한 5경로만 R33 고지에 더해 총 51경로다. source delta 7경로에는 모두 개별 R33 수정 고지가 있다. 원본 Apache 고지와 LICENSE·NOTICE·프롬프트 38개 원본 및 Windows 바이트 해시는 유지한다. README는 기존 구조를 유지하면서 후보 pin과 검증 범위만 갱신하고 루트 문서 해시에 다시 결속한다. 출처 검사는 제품 컴파일·실행이나 릴리스 승인을 대신하지 않는다.

## 이전 0c588 후보 기록

아래는 이전 후보의 기록이며 새 핸들 경계 수정 후보의 검증 결과가 아니다.

# R33 개발 후보의 출처와 검증 범위

현재 고정 소스는 `0c588be6c85098f0ff139ec7a9bc119c1da71f20`, 이전 공개 기준은 `8f52cad14bf45e76ef123aeea9f7fcfa22ac3b73`이다. 전체 제품 소스 버전은 `0.166.0+native-r33.20261102`다. 같은 소스의 개발용 시험 컴파일은 종료 0이며 표적 Rust 시험은 19그룹 73개 통과·0개 실패였다. 최종 제품 실행 파일·실제 runtime·설치·릴리스·실제 사용 환경의 검증은 미완료다.

비강제 전환의 drain 중 재요청이 강제 종료로 승격되던 F2 수정 소스를 포함한다. Unix에서는 graceful-only SIGHUP을, Windows에서는 별도 ACK가 필요한 graceful-only 제어 요청을 사용한다. 구 Windows R28/R32 서버는 이를 거절하며 강제 가능 요청으로 fallback하지 않는다. 반복 요청의 drain 보존 회귀시험은 통과했지만 Unix 전용 daemon handoff 시험은 Windows에서 실행하지 않았다.

## 포함한 변경

기존 메시지 재생·상태 조회·캐시 관측 위에 F2 수정, Windows 자동 CLI 선택, Rust launcher, session cleanup 시작 경로, 암시적 로컬 연결의 전체 버전 일치를 더했다. `tools/windows-cli`의 [UPDATE.ko.md](../tools/windows-cli/UPDATE.ko.md), `update.ps1`, `update.vbs`, `test_update.ps1`과 Rust launcher·관련 시험 소스를 모두 포함한다. [캐시 관측 문서](native-cli-r33-cache-observation.ko.md)도 명시적 source prefix로 유지한다.

자동업데이트는 고정된 공개 릴리스 출처·ZIP·패키지 receipt를 검증한 뒤 새 패키지를 보관하고 선택 포인터를 원자 교체하도록 작성했다. 다음 CLI 실행이 새 패키지를 선택하며 기존 패키지·공유 daemon을 종료하거나 교체하지 않는다. cleanup 명령은 공통 session backend 선택을 사용하고 암시적 로컬 연결은 CLI·서버의 전체 버전이 맞지 않으면 내장 backend로 전환한다. 명시적 원격 연결이나 기존 공유 서버를 이 규칙으로 자동 갱신하지 않는다.

CLI 예약 토글은 업데이트 전용 `--enable-updates`·`--disable-updates`를 사용한다. 기존 전역 feature 옵션과 충돌하던 parser panic을 분리했으며 PowerShell helper의 `-Action Enable|Disable`은 유지한다. 패키징 도구는 `opendiyUpdate`의 `schema=1`, `minLauncher=1`을 생성하며, helper는 고정 launcher 1이 지원하지 않는 요구나 누락을 자동 선택 전에 거부한다. 지원 범위의 확장은 별도 수동 bootstrap 갱신이 필요하다. Junction·reparse 경계와 PowerShell 시스템 모듈 검색 경로 보강도 포함한다.

비강제 갱신 보완은 기존 updater에 종료 요청을 보내지 않고 두 패키지 root의 기존 installer 락을 확인한다. 실패 시 이번 명령이 강제 종료나 선택 교체를 새로 유발하지 않는 범위이며, 독립적인 기존 updater 작업을 영구 금지하는 보장은 아니다. 상태 조회의 `stopped`는 원자적 설치 상태나 안전한 갱신 시점을 보증하지 않는다. 실제 daemon·installer 공동 동작 검증은 별도로 남아 있다.

캐시 진단은 일반 배치 logging과 별도 파일의 `response=true`, `wire=true` 선택으로 응답·wire 관측을 함께 켠다. 환경변수의 추가 선택을 요구하지 않으며 기존 환경변수 단독 선택은 다른 tracing consumer를 위해 유지한다. 기본값·공유 logging 설정·사용자 설정 파일은 자동 변경하지 않는다. 이 소스 공개 과정에서 운영 설정이나 실행 중 프로세스를 변경하지 않았다.

## 최종 후보의 실제 근거

개발용 시험 컴파일 receipt의 소스 커밋·종료 코드 0, 로그의 `dev-small` 완료 39.79초를 직접 확인했다. 이는 기존 cache를 재사용한 개발용 시험 컴파일이며 최적화된 제품 실행 파일의 빌드나 실행 검증이 아니다. 아래 19그룹의 로그·메타데이터·6개 실제 시험 실행 파일 SHA-256을 대조했다. 합계는 기록된 실행 횟수이며 전체 작업공간의 모든 시험 통과를 뜻하지 않는다.

| 시험 경계 | 그룹 | 통과 | 실패 |
| --- | ---: | ---: | ---: |
| 상태 조회·PID 관측·비강제 설치·graceful ACK | 4 | 6 | 0 |
| 로컬 socket 종료 요청의 PID·접속 경계 | 1 | 1 | 0 |
| 반복 graceful-only drain 보존·admitted 요청 종료 | 2 | 2 | 0 |
| CLI updater helper 선택·환경·패키지 경로 | 1 | 9 | 0 |
| CLI cleanup·status·no-force·update parser | 4 | 10 | 0 |
| TUI cleanup·daemon startup·전체 버전 인식 | 3 | 12 | 0 |
| 메시지 재생·pagination·hydration·질문 알림 | 4 | 33 | 0 |
| 합계 | 19 | 73 | 0 |

각 그룹은 종료 0이고 무시된 시험은 0개다. Unix 전용 daemon handoff 시험은 Windows 실행 대상에서 제외했으며, 앞선 실행의 0개 결과를 통과에 넣지 않았다. 앞선 `159927db` 실행에서 확인한 CLI parser panic과 URL slash fixture 불일치는 현재 소스에서 수정했고 해당 필터를 최종 후보에서 다시 통과했다.

고정 소스의 46경로 동결 목록 SHA-256은 `57de9639a703d88d8c39c33c4b9dcad8ea7f8b1f04efe7567cd47129297f8da0`이다. 목록의 실제 파일 해시와 source pin을 별도 출처 검사로 대조했다. 원시 로그·목록의 로컬 경로·시험 실행 파일 자체는 공개하지 않는다.

| 추가 검증 | 직접 확인한 결과 | 한계 |
| --- | --- | --- |
| Windows updater | PowerShell 5.1 격리 13개 통과, 종료 0; receipt와 최종 원본 두 파일 SHA-256 일치 | 파일 전송 fixture·스케줄러 mock이며 실제 네트워크·사용자 프로필·예약 등록·실제 Codex 실행 제외 |
| 패키징 도구 | 최종 소스와 공개사본 각각 13개 통과, 종료 0; 원본 두 파일 SHA-256 일치 | 합성 argv 실행 파일로 패키지·메타데이터를 검사하며 제품 빌드·설치가 아님 |
| 전달 준비 검사기 | 초기 21개와 후속 보강 23개 fixture 통과; 숨김 PowerShell의 배타 잠금, 바로가기 값 수집과 시스템 PowerShell·`Get-FileHash` 선택 경계 포함 | 패키지 실행 파일과 실제 COM 바로가기를 호출하지 않으며 설치 결과가 아님 |

PowerShell updater SHA-256은 `81c26d4744130029944926c031c239ebc9ce6f2cae567c995e0e1d55ac4b22ce`, 시험 소스는 `7a73dbb041e54882e396cdbb47e0d14c3d586a936fc4c6a378babf50ff77c193`다. ZIP·원자 포인터 교체·독립 프로세스 잠금·숨김 VBS와 native 전체 버전·릴리스 태그·필수 경로·launcher 호환성 검사를 포함한다. `Status.schedule`은 실제 스케줄러 조회 결과와 마지막 업데이트 결과를 구분하지만 시험의 scheduler 명령은 mock이다.

초기 21개 fixture를 통과한 전달 준비 검사기의 SHA-256은 `0c7e4f98ea329259f8bc50f25f648505aa1b75b7917db5dbcb1d1b84acfe7a6d`다. 후속 보강본은 `e79c481ea811210560f4178ec42c700ac0e19d1d57ac93c2b4ebe8adee17d131`이며 23개 fixture를 통과했다. 후속 로그와 실제 검사기 해시를 직접 대조했다. 시스템 PowerShell·시스템 모듈을 선택하며 가짜 PATH·작업 폴더의 실행 파일을 선택하지 않는 경계도 포함한다. 이 검사기는 공개 소스 export 대상이 아니며 실제 설치는 실행하지 않았다.

## 이전 후보의 근거

| 대상 | 후보 | 직접 확인한 결과 | 한계 |
| --- | --- | --- | --- |
| 메시지 재생 | `278f41f74f26b72cd2f3bf068400d8454a00ff75` | 4개 실행, 33개 통과·0개 실패 | 당시 상세 기록은 [재생 검증 기록](r33-replay-verification.ko.md); 같은 4필터를 현재 후보에서도 재실행 |
| 캐시 설정 red | `eab2546574ad24376e3f38b6d4dc005010201305` | 종료 101, 부모 1개 통과·2개 실패·1개 무시; 격리 자식 1개 실패 | 이전 구현의 실패 재현 |
| 캐시 진단 전체 | `76c0a7ab3b971918cd3d9f30daf8e1344941359c` | 종료 0, 부모 23개 통과·0개 실패·2개 무시; 격리 자식 1개 별도 통과 | 현재 후보에서 이 필터 전량 재실행하지 않음 |
| HTTP/WS 계측 경계 | `76c0a7ab3b971918cd3d9f30daf8e1344941359c` | 종료 0, 부모 1개와 격리 조합 6개 통과 | 로컬 fixture이며 실제 서버의 캐시 적중 개선을 뜻하지 않음 |
| 보완 전 상태 조회 | `1b23c21c9d8c11f6013bc8bd47707b134e84a68d` | 상태 조회 3개·PID 관측 1개 통과 | 후속 변경 전 결과; 현재 후보에서는 별도 4개 재실행 |
| 비강제 갱신의 앞선 정적 재검토 | `970171bac167c24e70a4a547be1be7c497ca2e84` | 당시 새 차단 결함 0건 | 이후 독립 검토에서 F2 확인; 현재의 수정·시험과 구분 |

이전 캐시 진단 시험 실행 파일 SHA-256은 `f21e645420b87b1b0ae288d20f43640c259d52b130f0bd7c411c7fa1ace46745`, HTTP/WS 경계 시험 실행 파일은 `1c32736cb19d7854bdf0c8983de828c510e9fa701cf6958a9f4d10d829b81f36`이다. 각 소스 커밋·메타데이터·실제 파일·로그의 이름과 결과 수를 대조했다.

## 공개 출처 검증

최종 고정 Git blob에서 source/tools 9,411개와 공식 대비 수정 553개를 export했다. 이전 공개 소스 대비 추가·변경은 35경로이며 새 파일은 8개다. 공개 출처 검사 종료 코드는 0이고 도구 시험은 17개 통과·실제 symlink 생성 1개 건너뜀이다. 공개 패키징 도구의 별도 fixture 시험은 13개 통과·0개 건너뜀이다.

```sh
python -B scripts/source-import/verify.py
python -B scripts/source-import/test_verify.py
```

출처 검사는 inventory·공식 대비 patch·프롬프트 catalog의 해시 결속, 각 공개 파일에서 고지를 제외한 원본 SHA-256·Git blob, 로컬 경로 의존성과 literal include를 확인한다. 기존 R32 기본 고지·공식 pin과 앞선 R33 고지를 보존하고 총 46경로만 R33 고지로 기록한다. 새 PowerShell·VBS 고지는 명시한 경로에만 적용한다. 루트 LICENSE·NOTICE와 내장 프롬프트 38개의 원본·Windows 바이트 해시는 유지한다. README 두 파일은 현재 배포·개발 상태에 맞춰 별도로 갱신하고 `SOURCE_BASELINE.json`의 루트 문서 해시에 다시 결속한다. `preservedRepositoryDocuments`는 exporter가 해당 문서를 덮어쓰지 않는 기존 계약이며 이번 README 편집 전후의 바이트가 같다는 뜻은 아니다. 공개 자료에는 실행 파일·원시 로그·실제 개인 경로·비밀을 추가하지 않는다. 이 공개 검사 자체는 Rust 빌드나 제품 runtime을 실행하지 않는다.

최종 통합 CLI·daemon runtime, 실제 installer 경쟁·grace 실패·구 updater와의 공동 동작, 실제 스케줄러 등록, 실제 사용 환경의 전환·재개·부하 효과, 데스크톱 공동 시험·모바일 연결·설치·릴리스는 미검증이다. 표적 회귀시험과 합성 경계 검사의 통과를 전체 R33 제품 완료로 확대하지 않는다.
