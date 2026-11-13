# Core 2.0 / Desktop 3.0 P0 공유 계약 초안

상태: DRAFT. 구조 협의와 구현 입력이며, HTTP 엔드포인트 구현이나 기능 지원을 뜻하지 않는다. `openapi.json`, `schemas.json`, `events.schema.json`이 전송 형태의 정본이다. 이 문서는 스키마만으로 판정할 수 없는 의미를 정의한다. 내부 협업 문서이며 공개 배포 문서는 별도 영문·한글판을 작성한다.

## 소유권과 첫 통합 경로

Core는 공유 wire/auth 계약, 요청 중복 방지, 작업·효과 수명, 영속 저장과 이벤트를 소유한다. Desktop은 클라이언트, GUI, 자체 저장소, PTY를 소유한다. 첫 통합은 workspace 선택 → 인증된 handshake → conversation 생성 → TaskRun 접수·조회 → SSE 표시·승인 → 취소 → 재접속 snapshot/operation 조회다. Core의 사용자 지정 JDBC DB와 Desktop의 자체 파일 H2를 공유하지 않는다.

## 식별자·리비전

- `operation.operationId`는 내부 requestId와 같은 요청 중복 키다. OpenAPI 문서의 엔드포인트 이름인 `operationId`와 구분한다. 같은 주체·연산·ID는 같은 요청 본문과 생성시각을 재전송한다.
- `taskRunId`는 서버가 생성하는 사용자 작업 ID다. 모델·도구·제어 명령마다 별도 `executionTurnId`가 있다.
- 승인 객체의 `requestId`는 승인 요청의 상관 ID다. 승인 답변 전송 자체의 중복 키는 `operation.operationId`다.
- `expectedRevision`은 해당 변경 대상 집합체의 CAS 버전이다. TaskRun 접수는 Conversation, 취소는 TaskRun, 승인 답변은 Approval, lease 변경은 InputLease 버전을 검사한다. DB 내부 `rowVersion`과 원격 실행 소유자의 `statusRevision`을 혼동하지 않는다.
- `instanceId`는 Core 프로세스 세대, `storeEpoch`는 영속 저장소 정체성, `streamEpoch`는 대화 저널 세대다. 정상 프로세스 재시작만으로 후자의 두 값을 바꾸지 않는다.
- 모든 카운터는 0부터 9223372036854775807까지의 선행 0 없는 십진 문자열이다. 시간은 UTC 고정 밀리초 `uuuu-MM-dd'T'HH:mm:ss.SSS'Z'`다. JavaScript number로 카운터를 변환하지 않는다.

## 인증·연결 파일

초기 구현은 숫자 loopback 주소에 바인딩한 Core2.0 전용 bearer 인증만 제공한다. 기존 CLI/vendor 토큰·설정 파일을 재사용하지 않는다. 원격/mTLS는 구현·검증 전 capability로 광고하지 않는다.

Core가 전용 연결 descriptor와 자격을 발급·회전·회수한다. Desktop의 신뢰 프로세스는 읽기 전 파일 owner와 ACL, 허용 경로를 검증하고 descriptor의 endpoint·instanceId·자격의 결속, 만료·회수 상태를 확인한다. 프로세스 재시작·자격 회전 때 다시 검증한다. `handshake`도 인증 대상이다. 일반 DNS 호스트명·리다이렉트·원격 endpoint로 자격을 전달하지 않는다.

자격을 renderer, 로그, DB, 오류 본문, 명령줄에 넣지 않는다. 연결 실패·권한 거절을 다른 무인증 endpoint 재시도로 숨기지 않는다. credential 발급·회전·ACL 실제 구현과 시험은 아직 남아 있다.

## 접수·중복·취소

인증·스코프를 먼저 검사한다. 유효한 최초 요청은 요청 receipt·작업·활성 작업 참조·최초 저널을 한 트랜잭션으로 커밋한 뒤 202를 반환한다. 202는 실행 완료가 아니다. 주체와 연산, 정규화 본문 SHA-256, 불변 생성시각에 요청 ID를 결속한다. 동일 요청 재전송은 원래 receipt를 반환한다. 본문·연산 변경은 `OPERATION_CONFLICT`다. timeout 후 새 ID로 다시 실행하지 않고 기존 operation을 조회한다.

handshake의 최대 요청 나이·미래 시계 오차·재전달 유예와 보존기간을 따른다. 보존기간은 수용기간과 유예보다 짧을 수 없다. 기록이 삭제됐어도 만료 ID를 새 요청으로 받지 않는다. 정규화 알고리즘과 receipt 정리의 구체적 구현은 아직 동결하지 않았다.

`CANCEL_REQUESTED`는 취소 요청만 확정한다. 실행 소유 세션·자손·비동기 효과가 실제 종료·정리된 뒤 `CANCELLED`다. 완료와 취소는 같은 저장소 CAS 순서로 결정한다. 연결 상실·EOF·lease 만료는 작업 완료나 효과 종료의 증거가 아니다. 최종 모델 응답, 모든 명령 응답, 모든 효과 정리가 확인되어야 `COMPLETED`다.

## 입력 소유권·승인

conversation InputLease 정본은 Core다. Desktop은 mirror와 입력 gate를 유지한다. 같은 conversation의 GUI와 CLI 입력에는 같은 lease 규칙을 적용한다. Desktop 소유 독립 shell PTY에는 conversation lease를 강제하지 않는다.

서버는 인증된 주체·clientInstanceId·leaseId·revision·만료·대상 conversation을 검사한다. clientInstanceId 자체는 인증 수단이 아니다. 입력 모드 전환과 lease 획득·갱신·해제는 CAS 연산이다. 불확실한 해제·회전 결과는 정본 조회 전 입력을 막는다. lease 만료가 실행 효과의 비만료 resource claim을 해제하지 않는다.

승인 답변은 승인 ID·요청 상관 ID·명령 ID·예상 승인 버전·허용 결정·유효기간·인증 스코프에 결속한다. 같은 세션의 승인/일반 입력도 서버에서 직렬화하고 취소·새 권한·미확인 명령과의 경합을 검사한다. lease만으로 승인·도구 실행 권한을 부여하지 않는다. `secret=true` 답변은 일반 transcript·SSE·로그·DB의 평문 표시 데이터에 복제하지 않는다.

## 이벤트·스냅샷

durable SSE의 `id:`는 `streamEpoch:sequence`, `event:`는 `durable`이며 data는 `events.schema.json`이다. UUID `eventId`는 이벤트 정체성이고 SSE 커서와 다르다. transient는 `event: transient`, control은 `event: control`; 둘에는 SSE `id:`를 넣지 않는다. heartbeat 주석에도 커서가 없다.

SSE 줄 끝은 [WHATWG의 CRLF·CR·LF 규칙](https://html.spec.whatwg.org/multipage/server-sent-events.html#parsing-an-event-stream)을 따른다. `limits.eventBytes`는 마지막 빈 줄 종결자 직전까지 한 블록의 원시 UTF-8 바이트 수다. 필드명·콜론·공백·주석·무시되는 필드 및 비어 있지 않은 각 줄의 종결자도 포함한다. 마지막 빈 줄 종결자만 이 계수에서 제외하고 별도로 최대 2바이트를 허용한다. 스트림 첫머리의 선택적 BOM은 한 번만 최대 3바이트를 별도 허용한다. 그 밖의 BOM은 일반 내용으로 계수한다. 주석이나 알 수 없는 필드를 만났다는 이유로 블록 계수를 초기화하지 않는다.

| 원시 입력 표기 | eventBytes 계수 | 마지막 구분자 |
|---|---:|---:|
| `data: x\n\n` | 8 | 1바이트 |
| `data: x\r\r` | 8 | 1바이트 |
| `data: x\r\n\r\n` | 9 | 2바이트 |
| `data: 가\n\n` | 10 | 1바이트 |

빈 줄의 CR을 받으면 다음 바이트를 기다리지 않고 블록을 끝낼 수 있다. 바로 뒤 LF는 이미 끝난 블록 구분자의 두 번째 바이트로 흡수하고 다음 블록 내용으로 세지 않는다. 수신 청크 경계와 무관하게 같은 바이트열은 같은 계수·초과 판정을 낸다. 입력 상한은 역직렬화 전에 적용하고, 부분 필드·블록은 상한 안에서만 보관한다. 빈 줄 없이 EOF가 오면 불완전 블록을 전달하지 않는다. 빈 블록·heartbeat에도 무제한 큐를 만들지 않는다. 이 계수 정의와 예시는 계약이며 실제 Core SSE 송신·Desktop 소비의 통합 시험을 대신하지 않는다.

재접속은 `Last-Event-ID` 또는 `(streamEpoch, after)` 쿼리 한 가지를 쓴다. 두 방식 동시 사용·부분 쿼리·형식 오류는 거부한다. 인증된 같은 대화·epoch에 대해 `sequence > after`를 재생한다. 중복은 epoch와 순번으로 한 번만 반영한다. 누락을 건너뛰지 않는다.

snapshot의 `appliedThroughSequence`와 `resumeAfter`는 같은 N이며 상태도 정확히 N 시점이다. `aggregateRevision`은 포함된 Conversation 버전과 같다. 서버는 이를 한 일관된 읽기로 만든다. Desktop은 상태와 N을 함께 저장하고 N+1부터 재생한다. UI 저장 ACK는 실행 효과 완료 ACK가 아니다.

GAP·epoch 변경·지원하지 않는 major·느린 소비자는 증분 반영을 멈추고 입력 보호를 유지한다. control 또는 HTTP Problem을 받고 snapshot/handshake를 다시 검증한다. 전송 snapshot은 원격 실행 revision/hash 계보 검증을 대체하지 않는다. 원격 명령·효과의 hash 계보가 미확인이면 높은 revision이나 인증된 연결만으로 자원 점유를 해제하지 않는다.

`textDelta`는 임시 표시다. 완전 `messageCompleted`가 같은 messageId의 표시를 교체한다. 델타 유실이 도구 실행이나 영구 이력 손실로 이어지지 않는다. raw continuation, vendor 서명·비밀, raw 비공개 reasoning은 wire에 내보내지 않는다. `reasoningPreview`는 표시 허용된 요약만 담는다.

## 오류·표시 설정

오류는 `application/problem+json`이다. 일반 상태 매핑은 400 INVALID_REQUEST, 401 UNAUTHENTICATED, 403 FORBIDDEN, 404 NOT_FOUND, 409 REVISION_CONFLICT/OPERATION_CONFLICT/LEASE_CONFLICT/APPROVAL_STALE/OUTCOME_UNRESOLVED, 410 REQUEST_EXPIRED/CURSOR_GAP, 422 CAPABILITY_UNSUPPORTED, 423 LEASE_REQUIRED, 429 BUSY, 426 SCHEMA_MISMATCH, 500 INTERNAL_ERROR다. `correlationId`는 비밀·내부 스택을 대신한다. 재시도 안내는 허가된 멱등 연산에만 적용한다.

표시 언어는 `Accept-Language`, 표시 시간대는 `X-Arcvera-Time-Zone`으로 요청한다. wire ID·enum·UTC 시각은 번역하지 않는다. 서버는 capability의 실제 지원 상태와 이유를 반환한다. 미구현 기능에 ENABLED를 반환하지 않는다.

## 검증과 남은 범위

fixture 검사는 JSON Schema 형태·타입·허용 필드·enum·카운터 한계를 확인한다. DB 원자성, 인증/ACL, 실제 요청 중복·취소 경합, SSE 복구, payload 비밀 차단은 실제 구현 통합 시험으로 별도 검증한다. 스키마 통과를 이들 동작의 통과로 보고하지 않는다. Desktop의 fixture 검토 후 계약 revision을 함께 동결한다. 기존 native 1.x 사용자 데이터·wire는 이 초안으로 자동 변환하지 않으며 별도 명시한 어댑터·이전 검증 전 보존한다.

## 0.2 초안 보완: 유한한 복구와 엄격한 variant

TOOL 턴은 원본 `callId`를 반드시 가지며, ACCEPTED receipt는 resourceId와 revision이 null일 수 없다. `messageCompleted`는 COMPLETE 또는 FAILED 메시지만 담고 PARTIAL을 완료로 승격하지 않는다. 전송 중 GAP은 SNAPSHOT_REQUIRED/CURSOR_GAP, 스트림을 열기 전 GAP은 HTTP 410/CURSOR_GAP이다.

스냅샷 첫 페이지가 정한 snapshotId·epoch·N·revision·pageCount·totalBytes를 모든 후속 페이지가 유지한다. pageIndex는 0부터 연속하고 마지막 nextPageToken만 null이다. 페이지 토큰은 인증 주체·대화·snapshotId·epoch·N·만료에 결속한 서버의 불투명 값이며 인증 자격이 아니다. 중복된 객체 ID, 페이지 누락, 버전 불일치, 만료는 전체 임시 페이지를 버리고 보호 상태로 다시 조회한다. 일부 페이지만 적용하거나 커서를 앞당기지 않는다.

스냅샷은 활성/보류 작업의 현재 투영과 최근 메시지 창을 담는다. 오래된 완결 이력은 같은 버전에 결속한 history 페이지로 읽는다. 서버는 일관 읽기 또는 불변 저장된 snapshot에서 페이징하며 각 요청 시점의 최신 행을 섞지 않는다. 대화·작업·효과 메타데이터를 충실히 전달할 수 없는 크기면 숨기거나 잘라 성공하지 않고 413/SNAPSHOT_LIMIT_EXCEEDED를 반환한다. 작업 접수/체크포인트 예산은 이러한 활성 상태 한도도 초과하지 않도록 검사한다.

하드 상한은 snapshot 페이지 1MiB, 최대 32페이지, 전체 snapshot 16MiB, history 페이지 1MiB, 일반 응답 16MiB다. 협상값은 이보다 낮출 수 있다. 수신자는 JSON 전체 역직렬화 전에 바이트 상한을 적용하고 페이지를 모으는 임시 저장에도 전체 상한을 적용한다. 배열 상한은 일반 128개, TaskRun turns 1024개/effects 256개, snapshot 작업 32개다. 이 값은 무제한 작업 수명 보장이 아니라 명시적 자원 계약이다. 큰 출력은 detail 참조와 별도 한도 있는 읽기로 제공한다.

닫힌 enum과 payload variant에 새 값을 추가하면 schemaMajor를 올린다. 미지 durable/control/Problem/DisplayPart나 major를 받은 소비자는 해당 대화의 증분 적용·입력을 멈추고 UNSUPPORTED 표시 및 호환 handshake를 요구한다. 모르는 이벤트의 커서를 저장하거나 raw payload를 UI·로그로 보내지 않는다. 기존 major를 유지한 임의 확장을 성공으로 해석하지 않는다.

P0 통합 범위는 text/default-profile 경로다. 모델·effort 선택, 첨부, runtime 전환과 확장 detail은 전체 Core2.0/Desktop3.0 목표에 남아 있으며 후속 계약/구현/검증 전 지원 capability로 광고하지 않는다.

## 0.2 소비자 조립 및 critical 복구 보완안

각 snapshot 페이지의 `state.conversation`과 `state.lease`는 모든 페이지에 의미상 완전히 같은 JSON 값으로 반복한다. `tasks`, `pendingApprovals`, `messages`는 각 객체를 정확히 한 페이지에만 넣는 연속 분할이며 빈 배열을 허용한다. 각 배열은 pageIndex 증가 순으로 이어 붙인다. 객체 자체는 페이지 사이에서 나누지 않는다. 원본 순서를 보존하며 수신 순서를 바꾸거나 중복을 제거해 오류를 숨기지 않는다.

수신자는 각 페이지를 한 번씩 0부터 pageCount-1까지, 요청에 사용한 바로 앞 응답의 nextPageToken으로 받는다. 모든 페이지의 schemaMajor·conversationId·streamEpoch·snapshotId·aggregateRevision·appliedThroughSequence·resumeAfter·pageCount·totalBytes는 동일하고 N=appliedThroughSequence=resumeAfter다. 마지막 토큰만 null이며 앞선 토큰들은 비어 있지 않고 중복되지 않는다. conversation의 ID·epoch·revision 및 lease의 conversationId는 봉투와 일치한다. 전체 합산 제한은 tasks 32, pendingApprovals 128, messages 128이며 task 각각의 turns 1024·effects 256이다. taskRunId, approvalId, messageId는 각 배열 전체에서 유일하고 executionTurnId·effectId는 각각 snapshot 전체에서 유일하다. 각 task의 conversationId, effect의 taskRunId와 originExecutionTurnId, approval의 conversationId·taskRunId·executionTurnId를 대조한다. activeTaskRunId가 있으면 해당 task가 있어야 한다. 참조 실패·불완전 페이지·불일치·중복·한도 위반은 전체 임시 조립을 폐기하고 cursor를 저장하지 않는다.

`totalBytes`는 모든 페이지에서 **압축 해제 후 각 HTTP 응답 entity의 실제 UTF-8 바이트 길이 합**이다. JSON 공백과 각 봉투 자체도 포함하며 HTTP 헤더·프레임·압축 바이트는 제외한다. 서버는 정확한 최종 직렬화 길이를 선언하고 클라이언트는 선언값과 실제 합계를 비교한다. 전송 압축 여부와 무관하게 streaming decoder가 압축 해제 중 페이지별 1MiB 및 전체 16MiB 이하를 집행하고, 최대 32페이지와 협상된 더 작은 제한도 적용한다. 선언값만 신뢰하거나 전체 할당 뒤 검사하지 않는다.

snapshot이 413이거나 Desktop이 로컬 ID를 모르면 입력 보호를 유지하고 `GET /api/v1/critical-recovery`를 인증해 pageToken 없이 호출한다. 첫 요청은 인증 주체가 접근할 수 있는 모든 대화의 미완료 작업(QUEUED/RUNNING/SUSPENDED/CANCEL_REQUESTED)과 모든 pending 승인에 대한 일관된 고정 시점의 불변 인덱스를 만든다. 각 항목은 conversationId와 복구·취소에 필요한 식별자·리비전을 포함한다. 서버는 고정된 항목을 페이지별 작업 최대 32개, 승인 최대 128개, 비압축 UTF-8 entity 최대 1MiB로 나누며 전역 작업 수에 임의 상한을 두지 않는다. 허용된 페이지 생성·보존 용량이 없으면 최초 요청을 BUSY로 거절하고 일부 목록을 완전한 성공으로 위장하지 않는다. 큰 본문은 잘라내지 않고 detail 참조로 보존한다.

페이지는 0부터 연속된 pageIndex·동일 recoveryId·expiresAt을 갖는다. 마지막에만 nextPageToken이 null이고, 중간 토큰은 유일하며 인증 주체·handshake의 storeEpoch·해당 인덱스·다음 순번·만료에 결속된 불투명 값이다. 다음 요청은 앞 응답의 토큰을 사용한다. 클라이언트는 전체 페이지의 중복 task/approval과 승인→작업·대화 참조를 검증하고 마지막 토큰까지 받은 뒤에만 그 고정 시점의 목록을 완전하다고 표시한다. 누락·중복·순서 오류·토큰 만료·권한 변경·storeEpoch 변경은 수집을 폐기하고 입력 보호 상태로 0부터 새로 시작한다. 페이지 내용은 커밋된 인덱스에서 오며 요청할 때마다 최신 행을 섞지 않는다. 클라이언트는 페이지를 제한된 저장소에 단계별로 보관하고 할당 제한 초과나 저장 실패 시 명시적으로 중단한다. 무한 버퍼·부분 적용·커서 전진을 하지 않는다.

이 인덱스는 고정 시점 이후 생성된 작업을 포함한다고 보장하지 않고 SSE 재개 커서도 아니다. 각 항목을 최신 작업/승인 정본과 대조하고 변한 리비전은 재조회한다. 필요하면 인덱스를 다시 읽는다. 새 작업이 계속 생길 수 있으므로 전역 정지·실시간 완전성은 별도 장벽/구독이 구현되기 전 주장하지 않는다. 해당 대화의 snapshot이 413이면 전체 대화 재동기화는 미완료로 남지만 인덱스의 작업 ID·revision으로 취소를 시도하고 충돌 시 정본을 재조회할 수 있다. 작업/승인 생성은 해당 항목의 식별자·최소 복구 메타데이터와 detail 참조를 읽을 수 있는 상태로 함께 커밋해야 한다. 승인 생성·수정은 decision 수와 길이, 개별 인덱스 항목의 페이지 바이트 한도를 초과하면 접수 전 거절한다. 기존 실행의 취소·정리와 복구 조회는 유지한다.

critical 승인별 결정 목록은 중복 없는 최대 16개이며 각 값은 1~64자다. 큰 prompt는 같은 conversation·승인 리비전에 결속된 불변 detail을 가리킨다. 상세 응답 한 페이지는 비압축 UTF-8 1MiB를 넘지 않고 `nextPageToken`을 이용해 순서대로 이어 읽는다. 하나의 큰 텍스트는 UTF-8 문자 경계를 지켜 조각으로 전달하고 전체 조각의 순서대로 이어 붙이면 원문과 같아야 한다. 페이지 누락·만료·접근 거절·보존 범위 이탈은 명시적 오류이며 부분 본문을 완전한 답변 근거로 취급하지 않는다. 기존 데이터가 이 한도를 충족하는지 확인하기 전 신규 계약을 활성화하지 않는다.
