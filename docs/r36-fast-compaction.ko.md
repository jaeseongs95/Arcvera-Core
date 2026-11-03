<!-- Modified for OpenDIY Codex R36 (2026-11-03); see MODIFICATIONS.md. -->
# R36 Fast 범위와 압축 지연 관측

이 문서는 `feat/r36-observability-20261103`의 소스 추적과 추가한 관측 경계를 설명한다. 사용자 실로그, 다른 세션의 대화·로그·인증·DB는 열지 않았다. 현재 문서의 압축 지연 원인은 **실측 미확인**이다. 실행 시간이나 개선율을 꾸며내지 않았으며, 계측 추가를 성능 개선 완료로 판정하지 않는다.

## Fast가 적용되는 범위

| 구분 | 소스에서 확인한 동작 |
| --- | --- |
| Fast 요청 값 | `ServiceTier::Fast.request_value()`는 `priority`다. Fast와 모델·추론 수준은 서로 다른 설정이다. |
| Ultra Fast | `ultrafast`라는 별도 tier다. Fast와 같은 값이 아니며 `Feature::UltrafastMode`의 독립 정책 검사를 받는다. |
| 사용 가능 여부 | 기능 정책과 **요청에 고정된 모델의 catalog**가 함께 허용해야 한다. 설정·UI에 표시된 문자열만으로 서버에 적용됐다고 단정할 수 없다. 지원하지 않는 tier는 최종 요청에서 생략한다. |
| 현재 루트와 하위 트리 | 루트 설정 변경은 그 트리의 `LocalAgentControl.runtime.root_service_tier`에 게시된다. 새 하위 에이전트는 spawn 시 루트 값을 받는다. 기존 하위 에이전트는 매 step의 context를 잡을 때 최신 루트 값을 읽는다. role 파일의 tier가 루트 선택을 이기지 않는다. |
| 실행 중인 하위 작업 | 이미 전송한 모델 요청의 tier는 바뀌지 않는다. 변경 뒤 새로 잡는 step과 다음 턴·압축 요청에 반영된다. 하위 세션 자체의 저장 설정을 전부 덮어쓰는 구현은 아니다. |
| 전체 이력 fork | 계승된 트리의 root routing을 사용한다. 빈 이력 하위 작업에도 같은 root routing이 적용된다. |
| 비활성 후 재적재 | 기존 `evicted_role_subagent_uses_root_service_tier_after_reload` 시험은 재적재 때 현재 루트 선택을 유지하는 경계를 검사한다. |
| 다른 루트 대화 | root tier는 각 `LocalAgentControl` runtime에 속한다. 한 루트의 선택을 앱 전체·다른 루트로 방송하는 설정이 아니다. |
| 기본값·정책 | UI는 catalog 기본값과 opt-out을 해석한다. core는 명시된 tier를 기능 정책과 catalog로 거른다. `default`는 wire 요청에서 생략된다. 예외인 `flex`는 일반 OpenAI 경로에서 catalog가 없어도 가능하지만 Bedrock은 catalog 표기가 필요하다. |
| 효과의 한계 | tier는 모델 요청의 routing 선택이다. 로컬 이력 준비, 도구 실행, 저장 flush, 화면 렌더가 빨라진다는 보장은 아니다. 이 저장소만으로 실제 계정의 서버 tier 적용·과금·지연 개선을 입증할 수 없다. |

근거 경로:

- `codex-rs/protocol/src/config_types.rs`: `ServiceTier`와 wire 값.
- `codex-rs/features/src/lib.rs`: `service_tier_enabled`의 Fast/Ultra Fast 독립 검사.
- `codex-rs/tui/src/service_tier_resolution.rs`: catalog·서버 정책·기본값 해석과 구 서버의 shared Fast gate 호환.
- `codex-rs/core/src/agent/control/service_tier.rs`, `runtime.rs`, `spawn.rs`, `api.rs`: 트리 단위 root preference 게시·계승.
- `codex-rs/core/src/session/mod.rs`: 루트 설정 commit의 게시와 `capture_step_context`의 하위 작업 step별 재해석.
- `codex-rs/core/src/session/step_settings.rs`: immutable snapshot; 이미 capture된 step에는 소급하지 않음.
- `codex-rs/core/src/client.rs`, `codex-rs/protocol/src/openai_models.rs`: 실제 request tier의 최종 catalog 필터.
- `codex-rs/core/src/compact_remote_v2.rs`: 전경·배경 압축 모두 해당 step의 `service_tier` 사용.

## 압축이 느릴 수 있는 실제 대기 경계

기존 `CompactionAnalyticsAttempt`는 총 `duration_ms`를 기록한다. `session.flush_rollout` trace span과 retry 경고·retry 지연 기록도 이미 있다. 하지만 총 시간만으로 로컬 준비, 기존 배경 압축 채택 대기, 네트워크/서버, retry, 이력 설치, 완료 통지를 구별할 수 없다.

소스에서 확인한 대기는 다음과 같다. 이것들은 실행 가능한 경로이며, 현재 사용자에게 실제로 오래 걸린 원인이라는 뜻은 아니다.

- 수동 압축 task는 turn 시작 통지, step context 준비와 world state 생성을 먼저 기다린다.
- V2는 pre-compact hook 실행 후 호환되는 배경 압축이 있으면 `wait_for_pending`에서 기다린다. 이 함수는 session lock을 놓은 뒤 결과를 기다리며, 완료한 요청의 과금 기록도 보존한다. 기다림을 없애고 중복 요청을 보내는 변경은 하지 않았다.
- 전경 V2는 stream retry를 전송 방식별 최대 2회로 제한한다. 공통 retry 처리기는 서버의 retry deadline, backoff, transport fallback을 적용한다. HTTP 내부 재시도·인증 갱신·연결 준비는 이 바깥 계측의 request 시간에 포함될 수 있다.
- `response.completed` 수신 뒤에도 관측 이벤트/usage 기록, 압축 이력 설치와 토큰 재계산, 완료 통지가 남는다.
- 수동 task 본문이 끝나도 task runner의 저장 flush와 `on_task_finished`가 남는다. 후자는 종료 hook·이벤트 전달·terminal flush·idle 전환·후속 작업 admission 등을 포함한다.

이번 변경은 위 동작·timeout·재시도 정책을 유지하며, 다음 INFO 관측을 더한다. 새 설정이나 별도 수집기를 만들거나 관측 설정을 켜지 않는다.

| `phase` | 측정 경계 |
| --- | --- |
| `prepare_turn` | 수동 압축의 turn 시작 통지부터 step/world-state 준비 완료까지 |
| `pre_hooks`, `post_hooks` | V2 압축 전·후 hook 호출 |
| `capture_background` | 배경 압축 입력 정규화·도구 attribution 포착; 앞선 session lock 획득 시간은 제외 |
| `prepare_background_images` | 배경 입력의 미해결 이미지 처리 |
| `pending_background` | 호환 배경 결과의 조회·대기; 후보 없음이면 짧게 끝날 수 있음 |
| `prepare_request` | foreground 이력 clone/trim/image 준비, prompt와 요청 metadata·선택적 trace 준비; background는 이미 포착한 입력의 나머지 준비 |
| `foreground_request`, `background_request` | 한 번의 바깥 모델 요청 시작부터 stream 결과 수집까지. 순수 서버 연산 시간이 아니며 HTTP 내부 재시도·연결·usage 통지도 포함 가능 |
| `publish_response_completed` | 전경의 `response.completed`를 받은 뒤 관측 완료 이벤트/usage 통지 처리. request phase 안에 중첩됨 |
| `retry_wait` | 전경 바깥 stream 실패 뒤 retry 판정·통지·backoff/deadline·fallback 처리; 실제 retry 없이 terminal error가 반환되는 호출도 포함 |
| `install_history` | 모델 결과 후 usage 정산·retained 이력 구성·reviewer hash·initial context·압축 이력 교체 |
| `publish_compaction` | 최종 토큰 재계산과 compaction item 완료 통지; 앞선 선택적 trace checkpoint 기록 시간은 제외 |
| `history_flush` | 수동 압축 task runner의 본문 종료 뒤 저장 barrier |
| `finish_turn` | 수동 압축의 `on_task_finished` 전체; 순수 UI 통지 대기 시간은 아님 |

각 종료 이벤트는 `codex.compaction.phase` span의 conversation/turn 식별자·정적인 phase, `elapsed_ms`, `outcome`만 추가한다. prompt·history·사용자 문구·error 문자열·경로·인증값은 추가하지 않는다. `Instant`로 측정하고 밀리초보다 짧은 구간은 0으로 반올림된다. `returned`는 계측 끝점 도달이며 성공을 의미하지 않는다. `dropped`는 명시 끝점 전 scope 종료로, 조기 오류·취소 등을 포함한다. 강제 프로세스 종료나 INFO 필터링 때문에 이벤트가 없을 수 있으므로 누락을 0ms·정상 완료로 해석하지 않는다.

중첩 phase와 동시에 실행된 배경 요청 시간을 모두 더하면 중복 계산한다. 같은 conversation/turn의 순서와 기존 attempt/retry·결과 metadata를 대조한다. `foreground_request`에서 중첩된 `publish_response_completed`를 구분하면 모델 응답 완료 후의 로컬 통지 비용을 분리할 수 있다. 그러나 남은 request 시간을 순수 서버 시간으로 부르지 않는다. 완전한 서버/전송/HTTP 내부 retry 분리는 기존 하위 transport telemetry가 함께 필요하다.

## 불변조건과 검증 계획

`cs-engineering`의 PERF-MEASURE-001에 따라 실제 지연 개선 판정은 동일 부하·동일 모델/tier·동일 hook·같은 이미지/도구 이력·같은 cold/warm 조건에서 baseline/candidate를 반복 측정한 뒤 한다. 현재는 그 제품 실측을 수행하지 않았으며 성능 의무는 UNVERIFIED다. `ponytail`에 따라 기존 tracing과 test runner를 사용하고 별도 queue·캐시·timeout 정책을 추가하지 않았다.

| 의무 | 실패 판별 및 검증 |
| --- | --- |
| R36-TIER-1: 루트 선택은 기존/신규 child와 compact의 이후 요청에 적용 | 기존 Wiremock integration 시험을 3개 전환에서 8개로 확장. Fast/Ultra Fast on/off/상호 전환과 미지원 Ultra Fast catalog를 실제 request body로 확인. 기존 요청·child 자체 설정과 루트 값의 구분도 유지 |
| R36-TIER-2: catalog 미지원 tier를 보내지 않음 | Ultra Fast가 없는 합성 catalog에서 child의 계속·압축·후속·신규 요청에 service_tier가 없음을 검사 |
| R36-OBS-1: 계측은 결과·취소·저장 순서를 바꾸지 않음 | 기존 compact/remote retry/cold-resume 시험을 유지하고 root의 일괄 Cargo 실행 대상으로 전달. timeout·retry 횟수·flush 순서의 source diff 대조 |
| R36-OBS-2: 완료와 미완료 계측을 구별 | 기존 unit runner에서 timer의 명시 종료·scope drop 2개 case를 작성. 각각 한 이벤트, phase·경과 시간·outcome·conversation/turn 식별자를 검사 |
| R36-PERF-1: 실제 병목은 실측으로 판정 | 승인된 합성 대화만 사용해 빈/긴/이미지/도구 이력, 배경 결과 유무, retry 유무를 비교. 각 구간 p50/p95와 오류율·메모리·총 시간을 보존. 실제 사용자 재현은 별도 승인 범위의 개인정보 없는 계측으로 확인 |

추가 test source는 `codex-rs/core/tests/suite/subagent_service_tier.rs`와 `codex-rs/core/src/compact_tests.rs`다. 새로운 runner나 외부 유료 API를 쓰지 않는다. Wiremock은 실제 서버 tier 정책·계정 entitlement·과금·WAN 성능을 대신하지 않는다. 새 case의 제품 검증과 timer unit 실행은 root 담당 일괄 Cargo 전까지 **NOT_RUN**이다. 회귀 감도 red/green·표적 mutation도 이 담당 범위에서는 **NOT_RUN**이다.

정적 검증은 지정된 rustfmt 1.99.1의 `--edition 2024 --config skip_children=true`와 소유 파일만의 `git diff --check`로 수행했다. rustfmt가 stable에서 `imports_granularity = Item`을 지원하지 않는다는 경고는 남지만 명령 종료 코드는 0이었다. 전체 컴파일·제품 시험 통과를 뜻하지 않는다.
