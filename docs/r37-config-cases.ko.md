<!-- Modified for OpenDIY Codex R37 (2026-11-04); see MODIFICATIONS.md. -->
# R37 표시 설정 호환성: 구현 전 시험 사례

대상: `codex_config::ui_language`의 실제 읽기/쓰기, TUI 언어 저장, 새 CLI 프로세스 help. R36은 변경하지 않는다. 모든 입력은 TempDir의 합성 CODEX_HOME이다.

| ID | 사전조건·입력·절차 | 독립 기대값 / 실패 oracle | 수준·실행 |
|---|---|---|---|
| T13a | config.toml에 model·주석·legacy ui_language, 별도 client 설정 없음. 언어 읽기 | 기존 언어를 읽고 shared config 바이트 불변 | config unit |
| T13b | 명시 언어 저장 후 shared config와 sidecar 읽기 | 언어는 opendiy-client.toml에만 저장; model/주석·기존 config 바이트 보존; 새 shared ui_language 생성 금지 | config unit |
| T14a | legacy ko + sidecar en, sidecar를 ko로 재저장 | sidecar 우선; 기존 다른 sidecar 키·주석 보존 | config unit |
| T14b | sidecar en + 환경 ko / 잘못된 환경 kr | ko help / 명확한 invalid 오류; 두 파일 모두 불변 | tui i18n + cli integration |
| T15a | sidecar 언어가 잘못된 값/형식/읽기 실패 | legacy로 조용히 fallback하지 않고 오류 | config unit |
| T15b | 프로젝트/profile만 ko, base/sidecar 없음 | 영어; 프로젝트가 클라이언트 전역 선호를 변경하지 않음 | 기존 config unit |
| T15c | 명시 언어 변경 시 legacy ui_language 존재 | sidecar 저장 성공 뒤 legacy 키만 제거; 다른 config 값 보존. 읽기만으로 마이그레이션 금지 | core edit + TUI 연결 |

원인: R36의 언어 저장은 공식판도 사용하는 config.toml 최상위에 확장 키를 추가한다. R37은 별도 client 파일을 사용하고 legacy 읽기 호환을 유지한다. 현재 후보 NOT_RUN. `cargo test --profile dev-small --offline --locked --target x86_64-pc-windows-msvc -j2 -p codex-config --lib ui_language::tests` 및 TUI/CLI 해당 사례를 단일 Cargo로 실행한다. 원격·운영 설정·인증 파일은 읽지 않는다.
