# R37 캐시 관측 상태와 검증 공백

2026-11-04 이 담당 CLI의 `session_control cache_diagnostics_status` 및 `logging_status` 직접 조회 결과:

- 현재 시작 모드: logging_enabled=true, response_requested=false, wire_requested=false, response_enabled=false, wire_enabled=false, wire_disabled_reason=response_disabled.
- 저장 설정도 logging=true, cache response=false/wire=false. 저장 설정과 현재 모드 사이 restart_required=false.
- 일반 로그 목적지는 G:/내 드라이브/앱로그/codex-cli, flush60s/sample5s/max_batch_bytes16777216. 이는 실제 모든 배치 생성·Drive 전송 성공의 증거가 아니다.

따라서 현재 실행의 응답·wire 캐시 분석용 진단이 켜졌다고 보고할 수 없다. 관측 누락을 cached_tokens=0이나 cache hit 0%로 집계하지 않는다. 이 조회에서는 설정을 변경하지 않았으며 다른 세션 원시 로그를 열지 않았다. 캐시 개선율·실사용 병목 원인 해결은 NOT_VERIFIED다. 허용된 해당 세션 usage 카운터와 현재 수집기 구현부터 대조하고, 실제 cache key/prefix 원인이 확인된 경우에만 제품 변경을 추가한다.

추가 소스 확인: `tui/src/chatwidget.rs::token_usage_info_from_app_server`는 app-server가 보내는 total/last의 `cached_input_tokens`를 그대로 매핑하며, 이 매핑은 응답·wire 진단 설정으로 막지 않는다. 따라서 진단 off만으로 일반 usage/analytics 자료가 없다고 결론 내릴 수 없다. 실제 해당 자료의 수집·보존 여부는 여기서 조회하지 않았으며 미확인이다.

## 일반 turn usage 수집 결함: 사전 시험 계획

2026-11-04 소스에서 확인한 결함은 `core/src/tasks/mod.rs`의 turn span이 `field::Empty`로 선언한 뒤 `Span::record`로 기록하는 숫자 필드 7개를 `diagnostics/src/batch_log.rs::numeric_field`가 허용하지 않는다는 것이다. `on_record`의 일반 visitor에서 값이 제외되므로 `span_close.fields`에 보존되지 않는다. 일반 로그가 켜져 있어도 응답·wire 진단의 활성 여부와 별개로 발생하는 수집 누락이다. 기존 배치에서 이 값을 복원하거나 실제 캐시율이 개선되었다는 뜻은 아니다.

허용 변경은 다음 정확한 숫자 필드명 7개를 기존 일반 숫자 allowlist에 추가하는 것뿐이다.

```text
codex.turn.token_usage.input_tokens
codex.turn.token_usage.cached_input_tokens
codex.turn.token_usage.cache_write_input_tokens
codex.turn.token_usage.non_cached_input_tokens
codex.turn.token_usage.output_tokens
codex.turn.token_usage.reasoning_output_tokens
codex.turn.token_usage.total_tokens
```

`engineering-test-plan`: 기존 `codex-diagnostics` lib 시험과 `tracing_subscriber::registry`/동기 채널 fixture를 사용한다. entryPoint는 실제 `tracing::info_span!` 생성 → `Span::record` → span drop이며, 실제 metadata filter/visitor/layer를 거친 `Command::Event` JSON row를 검사한다. 디스크 worker·Drive 전송·운영 자료·서버의 캐시 정책은 이 시험 범위 밖이다. runner·의존성·로그 API·설정은 추가하거나 바꾸지 않는다.

| case | 요구와 시나리오 | 독립 oracle |
| --- | --- | --- |
| CACHE-GENERAL-01 | 응답·wire 모두 false인 상태에서 같은 producer 필드명을 Empty로 선언한 3개 turn에 각각 미기록/0/양수 42를 기록 | open row에는 usage 키가 없고 close row에는 미기록 키 부재, 명시적 숫자 0, 숫자 42가 각각 구분된다. 7개 정확한 키를 모두 검사한다. |
| CACHE-GENERAL-02 | contextual parent와 thread/turn ID를 둔 span에서 기록하고 일반 이벤트 발생 | parent ID, open/close/event의 span ID, thread/turn ID, 동일 run instance ID가 유지된다. 기존 unprefixed input/cached/output/total 수치도 유지된다. |
| CACHE-GENERAL-03 | dotted prompt/path/secret 문자열과 미허용 dotted 숫자 필드를 span 생성·갱신 시 전달하고, 허용된 usage 이름에도 문자열을 전달 | serialized row 전체에 비밀 fixture 문자열이 없고 미허용 필드도 없다. 문자열은 이미 보존된 숫자를 덮어쓰지 않으며 미기록 값을 생성하지 않는다. 임의 prefix 허용과 numeric-name 문자열 허용을 거부한다. |
| CACHE-GENERAL-04 | response/wire DEBUG 이벤트도 같은 subscriber에 전달 | 해당 target row는 생성되지 않고 일반 usage/span/event row는 남는다. 진단 off를 자동 변경하지 않는다. |

고정 selector: `batch_log::tests::turn_usage_span_records_reach_general_batches_with_cache_diagnostics_disabled`.

부모가 승인된 직렬 환경에서 실행할 명령:

```text
cargo test --profile dev-small --offline --locked --target x86_64-pc-windows-msvc -j2 -p codex-diagnostics --lib batch_log::tests::turn_usage_span_records_reach_general_batches_with_cache_diagnostics_disabled -- --exact --test-threads=1 --nocapture
cargo test --profile dev-small --offline --locked --target x86_64-pc-windows-msvc -j2 -p codex-diagnostics --lib batch_log::tests:: -- --test-threads=1 --nocapture
```

`engineering-test-proof`: 변경 전 allowlist에서는 CACHE-GENERAL-01의 0/42 close row 기대값과 일치할 수 없다는 소스상 결함을 확인했다. pre-fix red와 수정 후 시험은 이 담당의 실행 금지에 따라 모두 NOT_RUN이다. 기존 allowlist/privacy 시험과 assertion은 유지한다. `engineering-test-result`: 구현·서식/diff 확인과 실제 시험 PASS는 분리하여 기록하며, 실제 운영 로그 존재·캐시율·개선 효과는 계속 NOT_VERIFIED다.

구현 후 정적 확인: 일반 숫자 allowlist에 위 7개 정확한 이름만 추가했고, 사전 사례 4개를 위 selector의 실제 tracing 계층 시험 1개로 작성했다. 기존 시험과 assertion은 수정하지 않았다. 소유 Rust 파일만 rustfmt를 적용하고 두 소유 경로의 `git diff --check`를 확인했다. 시험 실행·red/green·운영 관측은 하지 않았다.
