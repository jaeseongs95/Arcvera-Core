# 로컬 bootstrap descriptor 정책 초안

Core가 발급·회전·회수하고 Desktop 신뢰 프로세스가 소비한다. `bootstrap-descriptor.schema.json`은 형태만 검사하며 OS·인증서·시각·주체 검증을 대신하지 않는다. 실제 발급·보호·TLS 구현과 운영 확인은 아직 미완료다.

## 경로와 OS 보호

정본 경로는 명시적으로 선택한 `ARCVERA_HOME/runtime/core2/<profileId>/connection.json`이다. profileId는 UUID이며 요청 URI나 자유 형식 문자열을 파일 경로에 붙이지 않는다. Core/CLI/Desktop은 동일한 명시적 home 선택을 사용하고 CODEX_HOME이나 기존 vendor 자격을 읽지 않는다. 시험은 반드시 작업 폴더 안에 신규로 만든 전용 home을 지정하고, 기동 전에 경로가 그 안에 있는지 단언한다.

Windows의 파일·보호 디렉터리 owner SID는 현재 OS 사용자 SID와 같아야 한다. 파일 DACL은 현재 사용자와 SYSTEM(S-1-5-18)의 ALLOW만 허용하며 그룹·다른 사용자 ACE와 상속된 넓은 읽기/쓰기 권한은 거부한다. 읽기 허용만으로 안전하다고 보지 않고 수정·삭제·DACL/owner 변경 권한도 검사한다. runtime/core2 이하 각 조상은 다른 일반 사용자가 자식 이름을 교체하거나 수정할 수 없어야 한다. 관리자·SYSTEM 권한으로 호스트를 장악한 공격자는 이 로컬 보안 경계 밖이다. 발급자가 기존 사용자 home의 권한을 임의 변경하지 않고 전용 하위 디렉터리만 보호한다.

reparse point·junction·symbolic link를 따라가지 않는다. path 정규화 결과, 실제 열린 파일 identity, owner/DACL을 같은 읽기에 결속한다. 검사 후 다른 파일로 교체되는 TOCTOU를 막도록 파일 핸들을 열고 해당 핸들 기준 속성을 확인한다. 읽기 중 바뀌거나 검사 API가 필요한 속성을 확인할 수 없으면 연결하지 않는다. 파일은 64KiB 이하 UTF-8, 단일 JSON 객체이며 중복 키·NUL·미지 속성을 거부한다. 자격 원문을 오류·로그·renderer·일반 DB에 복제하지 않는다.

발급자는 보호된 임시 파일을 같은 디렉터리에 만들고 모든 필드를 완성한 뒤 원자 교체한다. 부분 파일을 공개하지 않는다. 다른 프로세스가 소유한 살아 있는 descriptor를 덮어쓰지 않고 충돌을 반환한다. 프로세스 종료 시 자신이 발급한 descriptorId/instanceId인 파일만 회수하며 다른 세대 파일을 삭제하지 않는다. 원래 파일이 사라지거나 파싱 실패하면 이전 자격으로 새 endpoint를 추측하지 않는다.

## TLS와 자격의 결속

endpoint는 https 및 숫자 loopback만 허용한다. port 0, DNS 이름, userinfo, path/query로 전달하는 토큰, 리다이렉트, 시스템 프록시 경유를 금지한다. port 재사용 공격을 막기 위해 bearer를 보내기 전에 TLS 서버 인증을 완료한다.

초기 구현의 trustAnchorDer는 Core가 이 인스턴스용으로 발급한 단일 self-signed X.509 leaf의 DER이다. 이 공개 인증서를 해당 프로필·세대의 전용 표준 truststore/Boot SSL bundle에 넣는다. 서버가 제시한 인증서의 표준 경로·유효기간·serverAuth EKU·key usage와 숫자 주소 SAN을 검사하고, SubjectPublicKeyInfo DER의 SHA-256이 serverSpkiSha256과 일치하는지 추가로 검사한다. descriptor의 anchor 공개키도 같은 pin과 일치해야 한다. trustAll, hostname 검증 끄기, 전역 truststore 변경은 허용하지 않는다. TLS 1.2 이상을 사용하고 암호·검증 정책은 표준 보안 provider 기본값을 따른다.

TLS 검증이 끝난 동일 endpoint 요청에만 `Authorization: Bearer`를 붙인다. credential은 32바이트 SecureRandom의 padding 없는 base64url이다. credentialId·descriptorId는 식별자일 뿐 인증 수단이 아니다. 서버는 credential 해시를 일정 시간 비교하고 현재 instance·OS principal·scope·generation·유효기간·폐기 여부를 검증한다. handshake의 instanceId/storeEpoch/principal/profile/protocol과 descriptor가 맞지 않으면 연결을 닫고 입력 보호를 유지한다. 독립 로컬 TLS 인증은 원격 접속 지원을 뜻하지 않는다.

## 유효기간·회전·취소

issuedAt/expiresAt은 UTC 고정 밀리초다. issuedAt의 미래 오차 허용은 30초, expiresAt은 issuedAt보다 뒤이며 최대 15분 이내다. 클라이언트는 로컬 Clock과 인증된 serverTime을 비교하되 임의 시계 보정으로 만료 자격을 연장하지 않는다. 서버는 만료 2분 전에 회전 가능하며 새 descriptor는 새 credentialId/descriptorId와 증가한 generation을 가진다.

같은 instance의 generation 역행·같은 generation의 다른 내용·과거 descriptor 재등장은 거부한다. 새 instance는 TLS 검증과 storeEpoch·진행 작업 대조를 다시 거친다. 정상 회전의 이전 자격 유예는 최대 30초이며, 명시적 revoke/프로세스 종료/침해 대응은 유예 없이 신규 요청과 SSE 연결을 거부·종료한다. HTTP는 401, 열린 SSE는 REAUTHENTICATE/CREDENTIAL_REVOKED 후 닫는다. Desktop은 descriptor를 다시 읽어 검증하며 무한 재시도하지 않는다. 이전 client·SSL bundle·listener는 닫고 교체하여 연결 자원이 누적되지 않게 한다.

Desktop 신뢰 프로세스는 프로필별 H2에 비밀 없는 high-water `(instanceId, storeEpoch, generation, descriptorDigest)`를 원자적으로 영속하고 재시작 후에도 읽는다. descriptorDigest는 보호된 핸들에서 읽고 검증한 **원본 UTF-8 파일 바이트 전체**의 SHA-256이다. 자격 원문·descriptor 원문을 DB나 로그에 저장하지 않는다. 형식·보호·TLS anchor/SPKI·bearer 인증 및 인증된 handshake의 profile/principal/instance/store 결속을 모두 확인하기 전에는 어떤 generation도 저장하거나 기준을 높이지 않는다. 같은 instance/store에서 generation이 낮으면 거절하고 같으면 digest가 일치할 때만 허용한다. 높으면 인증 후 비교와 저장을 한 트랜잭션으로 하며 동시 갱신에 진 값은 새 기준을 다시 읽어 비교한다. 다른 instance 또는 storeEpoch는 숫자를 비교해 자동 수락하지 않고 인증 후 이전 작업·저널과 새 저장소 정본을 대조할 때까지 입력을 보호한다. 복구 확인 뒤 새 기준을 저장한다. 이 저장 자체는 TLS·서버 인증의 대체물이 아니다.

owner SID와 SYSTEM으로 제한한 DACL은 다른 일반 OS 사용자의 접근을 막기 위한 경계다. 같은 SID로 실행되는 다른 프로세스에 대한 격리를 제공하지 않는다.

## 필수 검증 사례

정상 protected fixture·TLS·handshake; 다른 owner·그룹 읽기/쓰기·부모 교체 권한; 링크/읽는 중 교체; oversize·중복 키; DNS·http·port0; 잘못된/만료 인증서·SAN·SPKI; stale port에서 TLS 실패 시 bearer 미전송; 만료/미래 자격; generation 역행/동일세대 변조; 회전·revoke·SSE 종료; profile/principal/storeEpoch 불일치; 반복 재접속 뒤 context/connection 수 회수다. schema fixture만으로 이 OS/TLS/런타임 사례가 통과했다고 보고하지 않는다.
