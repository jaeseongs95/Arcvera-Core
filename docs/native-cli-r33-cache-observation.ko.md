<!-- Modified for OpenDIY Codex R33 (2026-11-02); see MODIFICATIONS.md. -->
# R33 프롬프트 캐시 관측 설정

R32는 별도 캐시 진단 설정과 `APP_CACHE_WIRE_DIAGNOSTICS=1`을 함께 요구했다. R33에서는 일반 배치 logging을 켠 상태에서 `$CODEX_HOME/cli-cache-diagnostics.toml`에 다음을 명시하면 응답과 wire 관측을 함께 저장한다. 환경변수는 추가로 필요하지 않다.

```toml
response = true
wire = true
```

기본값은 계속 모두 false다. 일반 CLI logging만 켜고 이 확장 파일을 빠뜨린 초기 설정에서는 캐시 관측이 수집되지 않는다. 확장 파일을 자동 생성하거나 기존 사용자의 설정을 자동 변경하지 않는다. 공유 `cli-logging.toml`에 캐시 설정 키를 추가하지 않는다.

`wire=true`만으로는 켜지지 않으며 배치 logging과 `response=true`도 필요하다. 이 조건이 꺼져 있고 기존 환경변수 선택도 없으면 wire producer의 직렬화·해시·ordinal 연산을 시작하지 않는다. 기존 환경변수 단독 선택은 다른 tracing consumer를 위해 유지하되 배치 저장의 logging·response·wire 조건을 우회하지 않는다. 환경변수 값 0은 별도 파일의 명시적 선택을 취소하지 않는다.

관측 모드는 프로세스 시작 때 고정된다. 설정을 적용한 새 실행의 `diagnostic_start`와 `batch_summary`에서 `instance_id`, `run.cache.response_enabled`, `wire_enabled`, `wire_producer_enabled`를 확인하고 캐시 전용 이벤트의 토큰 상태를 비교한다. 관측이 없거나 `cached_tokens`가 누락되면 적중률 0으로 계산하지 않는다. 기록은 기존 안전 필드 allowlist와 완결 배치 계약을 사용한다.

이 변경은 관측의 중복 선택 조건만 줄인다. 모델, 프롬프트 캐시 키, TTL, 컨텍스트 구성과 서버 캐시 정책은 바꾸지 않는다. 기존 R32 설계 문서는 당시 동작의 기록으로 유지한다.
