<!-- Modified for OpenDIY Codex R37 (2026-11-04); see MODIFICATIONS.md. -->
# R37 플러그인 캐시 사전 시험 계획

대상: `codex-rs/core-plugins/src/store.rs`, 기존 `store_tests.rs`.
기준: `1b441423`, Windows, R37 격리 사본. 운영 캐시·설정·인증·daemon·다른 대화는 읽거나 변경하지 않는다.
실행 담당은 root이며 이 문서 작성자는 Cargo를 실행하지 않는다. 아래 사례 상태는 모두 **NOT_RUN**이다.

## 원인과 계약

동일 버전 강제 refresh는 검증한 staging을 만든 뒤 플러그인의 모든 버전이 들어 있는 root를 backup으로 rename한다.
Windows에서 하위 프로세스의 cwd나 삭제 공유를 허용하지 않은 파일 핸들이 root 안에 있으면 이 rename은 접근 거부/공유 위반으로 실패할 수 있다.
manifest 버전이 같다는 사실은 내용 불변을 뜻하지 않는다. manifest만 비교하거나 PermissionDenied를 성공으로 바꾸면 실제 갱신을 누락한다.

CS 제약 PC-1: staging의 기존 manifest 검증과 migration을 거친 전체 트리만 비교한다. 이름·유형·내용·파일 권한·추가/누락 항목이 일치할 때만 교체를 생략한다.
PC-2: 변경된 동일 버전은 기존 원자적 backup/activation/rollback 경로를 유지한다. 잠금 해제 전 실패는 기존 캐시를 보존하고 호출자에게 오류를 반환한다.
PC-3: 해당 바이너리의 install/uninstall은 root 밖 단일 파일락으로 직렬화한다. 비교와 확정 사이 자체 writer 경합을 막는다. 중첩 락은 없다.
PC-4: 새 버전 활성선택(`local` 우선, 그 외 semver/문자열 비교), old version 정리 규칙을 바꾸지 않는다.

원본 marketplace는 사용자/다른 바이너리가 직접 쓸 수 있다. staging은 manifest 변경을 검사하지만 source 전체의 외부 변경 snapshot까지 보장하지 않는다.
advisory lock을 모르는 이전 바이너리·직접 파일 수정과의 동시 실행도 보호 범위 밖이다.

## 사례와 독립 oracle

| ID | 경계/조건 | 기대 결과와 탐지할 결함 |
| --- | --- | --- |
| PC01 | install 동일 버전/전체 동일 | 캐시 파일 hardlink identity 유지. 무조건 교체 탐지 |
| PC02 | install 동일 버전/skill 동일길이 변경 | 새 bytes 반영. manifest/size만 비교하는 잘못된 no-op 탐지 |
| PC03 | install 동일 버전/캐시 추가 파일 | 추가 파일 제거. source 부분집합 비교 탐지 |
| PC04 | Windows 실제 no-delete 파일 핸들 | root rename 실패 전제 확인 후 동일 내용 install 성공, 핸들 유효 |
| PC05 | Windows 살아 있는 자식 cwd | ready 신호 확인, root rename 실패 전제 확인 후 동일내용 install 성공, 자식 종료 전 살아 있음 |
| PC06 | Windows 동일버전 변경+잠금 | backup 실패 context/OS 원인 보존, 기존 bytes 보존; 잠금 해제 후 갱신 성공 |
| PC07 | Windows 읽기 공유도 금지한 핸들 | 비교 IO 실패를 성공으로 숨기지 않고 기존 파일 보존 |
| PC08 | Windows 새 버전+기존 버전 cwd | 새 버전 성공/활성 2.0.0, 하위 child 생존. 낮은 잠긴 버전 잔존 가능 |
| PC09 | 두 독립 프로세스/스레드 동시 동일 refresh | 모두 완결, 캐시 완전성 유지, timeout/lock 해제 확인. 스레드만으로 프로세스 검증을 대신하지 않음 |
| PC10 | Unix 실행 권한만 변경 | 변경된 권한 반영. 내용만 같으면 생략하는 결함 탐지 |
| PC11 | 기존 manifest 불일치·fallback·remote metadata 수명 | 기존 테스트 회귀 보존 |

기존 Rust `#[test]`/`tempdir` 체계를 쓴다. 파일시스템과 Windows child/cwd/공유 핸들은 실제 OS 자원이며 mock하지 않는다.
자식은 자기 fixture에서만 `cmd.exe`를 실행하고 readiness 이후 stdin 종료 또는 kill/wait로 회수한다. 입력·기대값은 합성 내용이며 인증·사용자 데이터를 쓰지 않는다.
rollback activation 실패의 결정적 OS fault injection은 현재 runner에 없다. 기존 rollback 코드를 그대로 유지하고 정적 대조하며, 실행 증거로 포장하지 않는다.

## 실행과 증거 계획

root 직렬 실행 명령: `cargo test -p codex-core-plugins store::tests --lib` (R37의 승인된 profile/target/TEMP 옵션은 root가 적용).
같은 회귀 테스트 파일로 수정 전/후를 격리 비교한다. 원본 제품 파일을 사용자의 작업 트리에서 reset하지 않는다.
결과에는 후보 hash, 테스트 파일 SHA256, argv, 종료 코드와 원시 stdout/stderr를 연결한다.
사전 문서의 NOT_RUN은 실제 root 실행 결과가 확인될 때만 갱신한다. 설치/배포 검증이나 운영 문제 재현으로 일반화하지 않는다.

## 실제 OS 전제 확인 (Cargo 전, 2026-11-04)

`artifacts/plugin-lock-probe-01`의 자기 합성 fixture에서 .NET `Directory.Move`를 실행했다. `cmd.exe /D /Q /C "echo ready & set /p finish="`의 cwd를 캐시 버전 하위로 두고 ready/생존을 확인한 경우와, 하위 파일을 `FileShare.ReadWrite`(Delete 제외)로 연 경우 모두 ancestor rename이 실패했다.
두 결과는 `Access to the path ... is denied`, HRESULT `-2147024891` (`0x80070005`, OS error 5)였다. 자기 자식은 finally에서 kill/wait, 핸들은 Dispose했다. 기존 운영 파일·프로세스는 사용하지 않았다.
이것은 Windows 잠금 전제의 실제 재현이며 Rust 설치 경계의 red/green 결과는 아직 **NOT_RUN**이다.

같은 전제를 별도 fixture `artifacts/plugin-lock-probe-02`에서 다시 실행해 스크립트와 원시 JSON을 보존했다.
명령: `& './artifacts/plugin-lock-probe-02/probe.ps1'`, 종료 코드 **0**.
원시 근거: `artifacts/plugin-lock-probe-02/result.json`; 정확한 실행 코드: 같은 폴더 `probe.ps1`.
두 경우 모두 HRESULT를 `-2147024891`로 assert하며, 예상 밖 rename 성공은 실패한다.
