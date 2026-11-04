<!-- Modified for OpenDIY Codex R37 (2026-11-04); see MODIFICATIONS.md. -->
# R37 메모리 통합 공개 경계: 구현 전 사례

입력은 격리된 HOME/SQLite/합성 note뿐이다. 운영 메모리와 다른 대화 로그는 읽지 않는다.

| 사례 | 실제 진입점·입력 | 기대값·거부할 잘못된 결과 |
|---|---|---|
| M01 | session_control memory_consolidate, retry 생략 | thread/memory/consolidate를 원래 thread에 1회 전송, retry=false; accepted를 완료라고 하지 않음 |
| M02 | retry=true로 명시 요청 | 기존 lease 보존, 알려진 backoff만 재시도; 다른 thread/root 입력 거부 |
| M03 | memory_view의 phase2 job pending/running/done/error | 선택 scope의 상태/시간/오류/watermark 반환; 특정 note 반영 완료 추정 금지 |
| M04 | feature/generation off·state 없음·프로젝트 root 미확인 | blocked 또는 실제 unavailable 오류, 설정 자동활성화·global fallback 금지 |
| M05 | 서버 RPC 미지원/timeout | 명확한 오류/결과 미확인, 자동 재전송 없음 |
| M06 | 레거시 memory_read 응답 summary만 존재 | 이전 optional decode 호환; 신규 상태필드 없음을 통합완료로 해석하지 않음 |

test-engineering: 기존 session_control 구조화 parser/retained reply fixture, state 실제 독립 DB pool, memories phase2 fixture를 재사용한다. RPC와 JSON schema의 compile/생성·실제 app-server 호출을 각각 확인한다. 지금 상태 NOT_RUN.
