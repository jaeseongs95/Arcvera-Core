# R37 cold 종료 항목 정리: 사전 사례와 계약 공백

대상: R37 작업 후보의 `core/src/thread_manager/cleanup.rs`, app-server `request_processors/thread_delete.rs`, thread-store `local/{unloaded_cleanup,delete_thread}.rs`, rollout `writer_lock.rs`를 읽기 전용으로 대조한다. R36·운영 daemon·운영 데이터·인증·다른 세션 원시 로그는 접근/수정하지 않는다. 사용자 실제 삭제는 중단 상태이므로 수행하지 않는다. Cargo는 root 직렬 실행만 허용한다.

## 사전 engineering-test-plan

기존 Rust runner와 TempDir fixture를 재사용한다. 새 runner·운영 허용 플래그·보호 검사 해제는 추가하지 않는다.

| 사례 | 공개 경계·합성 입력·절차 | 독립 oracle | 상태 |
| --- | --- | --- | --- |
| C01/T19 | graph에 연결된 같은 부모의 loaded completed 자식을 unload한 뒤 preview/기존 selection confirm | 둘 다 `reference_participants_unverified`, snapshot 없음, 원본 보존; `running`으로 보고하지 않음 | 기존 `unloaded_cleanup_defaults_to_preservation_when_participants_are_unverified` 재사용, R37 NOT_RUN |
| C02/T19 | 같은 부모의 completed/failed/cancelled cold 항목 | 참여자 증명 부재 시 모두 보존; terminal 완료 판정과 삭제 권한을 구분 | 제품 지원은 미완, 실행 NOT_RUN |
| C03/T20 | 별도 같은-home store가 PreparedFork 보유 중 원본 정리 시도 | 참조 게시 shared lease가 삭제 exclusive lease를 막고 dangling source 없음 | 기존 `metadata_cleanup_cannot_publish_dangling_cross_store_prepared_fork` 재사용, NOT_RUN |
| C04/T20 | 다른 writer가 UUID writer File 보유 또는 삭제 뒤 stale PreparedFork 게시 | writer 소유 시 보존, stale source 게시 거부 | 기존 unloaded/metadata cleanup 회귀 재사용, NOT_RUN |
| C05/T21 | preview 뒤 새 turn/queue/mail/goal/자손 변경 | 정확한 selection만 처리; 변경·대기 작업은 보존 | 기존 cleanup suite 재사용, NOT_RUN |
| C06/T22 | 정상 종료/재시작 뒤 이전 프로세스가 준비했던 미게시 fork가 남을 수 있는 legacy 참여자 | 현재 writer lock이 비거나 신규 participant 목록이 비어도 안전 증명으로 승격하지 않음 | 정적 계약 반례; 제품 cold 활성화 불가 |
| C07/T19~22 | 검증된 모든 게시자가 최소 참조 프로토콜을 준수하고 legacy 재진입이 차단된 storage namespace | 같은 namespace의 준비/게시/삭제 전 기간 참여, epoch/generation 일치, 오류/취소에도 lease 유지 | 현재 공개 계약·전환 경계 없음, CASE_EXECUTION_NOT_READY |

## 확인한 현재 동작

- 제품 cold 보존은 ThreadStore의 metadata parser 실패가 아니다. Core `CleanupAdmission::unloaded_reference_block_reason()`이 production에서 항상 `Some("reference_participants_unverified")`를 반환하여 preview/confirm이 metadata reservation 전에 멈춘다. 예외는 private `cfg(test)` isolated TempDir fixture에만 있다.
- preview outcome은 `preserved`이고 reason은 위 코드다. `preserved`를 `running`으로 바꾸는 변환은 이 실제 cleanup 경로에서 확인하지 못했다. app-server cleanup 핸들러는 `subagent_processor.rs`가 아니라 `thread_delete.rs`다. subagent processor의 managed status는 cleanup 결과와 별개이며 `Unknown`을 `Unknown`으로 보존한다.
- rollout WriterLockCoordinator의 UUID lock은 파일 잠금만 보유하며 participant capability/version/epoch를 기록하지 않는다. `.references.lock` shared/exclusive는 현재 구현의 PreparedFork·지연 recorder 게시·revert·archive/unarchive·compression·delete에 적용된다. 이전 바이너리는 이 신규 참조 잠금에 참여하지 않는다.
- 그러므로 R37이 새 participant 파일/최소 버전 숫자만 만들거나 PID 목록/현재 잠금 공백을 검사해도 legacy process의 메모리 속 미게시 참조와 미래 재진입을 배제할 수 없다. 신규 writer끼리의 race PASS를 혼합 legacy 환경의 cold 안전성으로 일반화하지 않는다.

## 최소 완전 지원에 필요한 경계

1. 현재 store namespace의 모든 과거 writer가 정상 배출됐다는 전환 경계와, 기존 바이너리가 이후 같은 namespace에 다시 쓰지 못하는 경계가 함께 필요하다. 기존 `.references.lock`만으로는 후자를 만들 수 없다.
2. 안전한 후보는 새 버전 전용 storage namespace와 그 namespace를 이해하는 writer만의 참조 프로토콜 계약이다. 구버전이 경로 규칙을 무시하지 못하도록 기존 경로를 계속 공유하지 않아야 한다. legacy data를 이전하려면 기존 writer 배출, source/reference 완전 이전과 재검증, 읽기/쓰기 호환 정책이 필요하다. 현재 작업 범위에서 임의 migration이나 운영 전환을 하지 않는다.
3. 별도 broker를 통한 쓰기 단일화와 legacy 경로 접근 차단도 가능하지만, 현재 서비스·인증·권한·프로세스 수명 계약을 바꾸는 더 큰 선택이다. R37 cold 결함의 최소 패치로 새 broker를 추가하지 않는다.
4. 어떤 선택이든 protocol minimum은 기능상 `PreparedFork 준비부터 durable header 게시까지 shared lease`, `모든 canonical 참조 게시·이동 경계 shared lease`, `삭제 scan부터 파일/DB/host cleanup 완료까지 exclusive lease`, `취소 이후 실제 owner까지 lease 유지`, `같은 namespace와 epoch`다. 버전 숫자는 그 계약을 구현한 고정 후보와 전환 증거에 결속해야 하며 지금 임의로 지원 최소 버전으로 선언하지 않는다.

## 판정

원인과 기존 보호 경계는 확인했다. 현재 범위에서 검사 해제 없이 production cold 지원을 켤 수 있는 완전한 변경은 없다. Core/protocol/namespace 호환 정책에 영향을 주는 선택이 필요하므로 root에 구체안과 영향을 전달한다. 이 조사로 제품 파일은 변경하지 않았고 실제 삭제/Cargo 실행은 0회다. 새 기능이 없는 상태에서 기존 PASS를 복제하는 테스트만 추가하지 않는다. 기존 public preservation/race 시험은 root 후보 검증 대상으로 인계한다.
