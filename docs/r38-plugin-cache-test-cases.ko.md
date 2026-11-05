# R38 플러그인 캐시 시험 사례

구현 전 계획이다. 대상은 `codex-rs/core-plugins/src/store.rs`의 `PluginStore::install`이며, 기존 `store_tests.rs`와 Cargo 시험 체계를 사용한다. 모든 동적 시험과 red/green은 현재 `NOT_RUN`이다. 담당 root의 직렬 실행 지시 전 Cargo, 빌드, 시험, 커밋, push를 실행하지 않는다.

## 요구와 불변조건

- PC-01: 설치 payload에서 이름이 정확히 `.git`인 항목만 제외한다. 디렉터리와 worktree 포인터 파일, 하위 경로에도 적용한다. `.gitignore`, `.gitattributes`, `.gitmodules`, `.github` 등 다른 이름은 payload로 보존한다.
- PC-02: 기존 설치에 남은 `.git`가 달라도 payload가 같으면 제자리 재사용한다. migration을 위해 기존 `.git`를 강제 삭제하거나 cache 상위 디렉터리를 rename하지 않는다.
- PC-03: 같은 버전의 manifest, skill, MCP와 다른 payload의 변경·추가·삭제 및 파일 권한 차이는 기존 교체 경로로 보낸다. 비교 I/O 실패를 같음이나 성공으로 바꾸지 않는다.
- PC-04: 잠긴 Windows 설치에서 실제 payload가 달라지면 오류와 이전 설치를 보존한다. 락 해제 후 정상 교체가 가능해야 한다. payload symlink를 따라가서 같다고 판정하지 않는다.
- CS 적용: `PERF-CACHE-003`의 캐시 정합성을 PC-01~04로 구체화한다. 기존 설치 직렬 락, staging 검증, 원자 교체 및 rollback을 유지한다. 기존 8 KiB 파일 비교 버퍼를 유지하며 새 캐시·큐·재시도·시간 제한은 만들지 않는다. 성능 수치 개선은 주장하지 않는다.

## 사례

각 사례는 실제 파일시스템과 공개 설치 경계를 사용한다. Windows 락 사례는 기존 `PluginChild`와 `assert_root_is_locked`를 재사용한다. oracle은 고정 입력 bytes, 파일 존재, 실제 rename 실패와 설치 결과이며 구현 비교 함수를 기대값으로 사용하지 않는다.

| ID / 요구 | 입력·사전조건 | 절차 | 기대 결과와 pass/fail oracle | 상태 |
| --- | --- | --- | --- | --- |
| PC-T01 / PC-01 | 임시 source에 `.git/index` 디렉터리, `nested/.git` worktree 포인터 파일, `.gitignore`, `.gitattributes`, `.gitmodules`, `.github/workflow.yml`, 정상 manifest·skill·MCP를 둔다. | 공개 `install`을 호출한다. | 설치의 두 `.git`는 없고 나머지 파일 bytes는 입력과 같다. source의 `.git`는 유지된다. 하나라도 다르면 FAIL. | NOT_RUN |
| PC-T02 / PC-02 | 정상 설치 후 legacy cache `.git/index`와 `skills/.git` 파일을 직접 만들고 source에는 다른 `.git/index`를 둔다. 설치 skill의 별도 hard link를 둔다. | 같은 버전을 재설치한 뒤 별도 hard link에 새 bytes를 쓴다. | 성공, 설치 skill에서도 새 bytes가 보이고 legacy `.git` bytes는 그대로다. hard link와 설치 파일 identity가 갈라지거나 metadata를 바꾸면 FAIL. | NOT_RUN |
| PC-T03 / PC-03 | 같은 버전 설치의 source에 `.git`가 있고 manifest, skill, MCP, `.gitignore`의 내용을 각각 바꾸거나 새 파일을 추가하거나 MCP를 삭제한다. | 각 독립 fixture에서 재설치한다. | 변경 bytes·추가·삭제가 실제 설치에 반영되고 새 cache에 `.git`가 없다. 하나라도 옛 payload면 FAIL. | NOT_RUN |
| PC-T04 / PC-02,04 | Windows 설치의 legacy cache `.git/index`가 source와 다르고 실제 child cwd로 상위 rename이 OS 5/32로 실패한다. | 같은 payload 버전을 재설치한다. | 성공, child 생존, 기존 `.git/index` 보존, 활성 버전 동일. rename 전제 미충족이나 설치 오류면 FAIL. | NOT_RUN |
| PC-T05 / PC-02,04 | Windows 설치 파일을 delete 공유 없이 열고 실제 상위 rename 실패를 확인한다. source에는 다른 `.git` worktree 포인터를 둔다. | 같은 payload 버전을 재설치한다. | 성공, 열려 있는 핸들에서 원래 skill bytes를 읽을 수 있다. rename 전제 미충족이나 설치 오류면 FAIL. | NOT_RUN |
| PC-T06 / PC-03,04 | Windows child cwd가 cache를 잠근 상태에서 skill bytes와 source `.git/index`를 바꾼다. | 재설치, 기존 payload 검사, child 종료, 재설치 순으로 실행한다. | 첫 설치는 `failed to back up plugin cache entry` 오류이며 원래 skill·활성 버전 보존. 락 해제 뒤에는 새 skill 설치. 오류를 성공으로 바꾸거나 부분 갱신하면 FAIL. | NOT_RUN |
| PC-T07 / PC-03 | Unix에서 source 실행권한을 변경한다. Windows에서 설치 파일을 공유 읽기 불가로 연다. | 기존 권한 변경 및 unreadable-cache 시험을 실행한다. | Unix 권한이 갱신된다. Windows 읽기 실패는 `failed to compare installed plugin cache entry`로 반환된다. 같음/성공으로 숨기면 FAIL. | NOT_RUN |
| PC-T08 / PC-04 | Unix에서 설치 skill을 외부 동일 bytes 파일에 대한 symlink로 바꾸고 source `.git`를 둔다. | 공개 `install`을 호출한다. | 설치 skill은 일반 파일로 복구되고 외부 파일 bytes는 유지된다. symlink 유지·추적 시 FAIL. 기존 manifest/skill/executable symlink 거부 시험도 유지한다. | NOT_RUN |
| PC-T09 / PC-03,04 | 기존 same-version 동시 설치·다른 프로세스 설치 락·새 버전 설치 fixture. | 기존 시험을 그대로 실행한다. | 설치 완전성·프로세스 직렬화·구버전 child 생존과 신버전 활성화의 기존 assertion을 모두 만족한다. | NOT_RUN |

## 실행과 근거 계획

예정 명령은 승인된 별도 build 작업 사본의 `codex-rs`에서 `cargo test -p codex-core-plugins --lib store::tests:: -- --test-threads=1`이다. Windows는 실제 cwd/file-handle 락 사례를, Unix는 권한·symlink 사례를 실행한다. 다른 호스트의 미실행 항목을 통과로 세지 않는다.

red/green은 root가 허용한 격리 사본에서 같은 `store_tests.rs` bytes와 argv를 유지하고 `store.rs`의 `.git` 제외만 원래 동작으로 바꿔 PC-T01,02,04,05가 실제 assertion/설치 오류로 실패하는지 확인한다. runner·컴파일 실패는 red가 아니다. 원시 stdout/stderr, 종료 코드, source/test digest, 후보 커밋 또는 diff 식별자와 호스트를 보관한다. 현재 proof/result는 모두 `NOT_RUN`이며 최종 실행 결과는 root가 연결한다.

기존 cache `.git`는 동등 payload 재사용 시 남아 있을 수 있다. 이후 실제 payload 교체 때 새 staging에서 제외되며, 이 변경은 별도 강제 정리나 운영 cache 변경을 수행하지 않는다.

## Windows 실제 결과

위 NOT_RUN 표는 실행 전 계획을 보존한다. Windows의 같은 격리 시험 사본에서 기존 `store::tests::` 전체를 실행했다.

- 첫 후보: 38 PASS / 0 FAIL, exit0. PC-T01~06 및 PC-T07의 Windows 읽기 거절, PC-T09 설치 직렬화·새 버전 활성화를 포함한다.
- 비교 실행: 같은 시험 파일 bytes를 유지하고 제품 `store.rs`만 R37 원본으로 복원하자 32 PASS / 6 FAIL, exit101이었다. `.git` 설치 제외, legacy 내용의 파일 신원 유지, metadata 변경을 동반한 교체, 잠금 해제 뒤 교체의 assertion이 실패했다. 실제 cwd/파일 핸들 두 시험은 원래의 OS5 `failed to back up plugin cache entry`를 재현했다. 컴파일 실패를 red로 계산하지 않았다.
- 후보 복원·서식 정리 뒤 최종 시험: 38 PASS / 0 FAIL, exit0. 비교 실행 뒤 제품 후보를 복원했고 다른 운영 cache를 입력으로 쓰지 않았다.
- Unix 실행권한·symlink 사례는 이 Windows 실행의 통과 수에 포함하지 않는다. 해당 OS의 실제 실행은 NOT_RUN이다.

명령은 `cargo test --offline --locked --profile dev-small --target x86_64-pc-windows-msvc -j2 -p codex-core-plugins --lib store::tests:: -- --test-threads=1`이었다. 원시 출력·각 argv·종료 코드·시험 파일과 제품 SHA는 로컬 검증 자료에 보존하며 공개 소스에 원시 로그를 포함하지 않는다. 최종 릴리스 빌드나 사용 중인 AGS 운영 갱신 성공을 이 결과로 주장하지 않는다.
