//! Structured model entry into the existing client settings and thread controls.
use codex_app_server_protocol::DynamicToolFunctionSpec;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;

pub(crate) const NAME: &str = "session_control";
const PREFIX: &str = "session_control:";

#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Command {
    Status,
    MemoryView,
    MemoryConsolidate {
        #[serde(default)]
        retry: bool,
    },
    MemoryEnable {
        enabled: bool,
    },
    MemoryNote {
        note: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<Scope>,
    },
    MemoryForget {
        note: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<Scope>,
    },
    MemoryRead {
        enabled: bool,
    },
    MemoryGenerate {
        enabled: bool,
    },
    MemoryScope {
        scope: Scope,
    },
    ContextGet,
    ContextSet {
        tokens: i64,
    },
    ContextReset,
    Compact,
    UiLanguage {
        language: String,
    },
    ServiceTier {
        tier: String,
    },
    ReloadPlugins,
    CleanupAgentsPreview,
    LoggingStatus,
    LoggingSet {
        enabled: bool,
    },
    CacheDiagnosticsStatus,
    CacheDiagnosticsSet {
        response: bool,
        wire: bool,
    },
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Scope {
    Project,
    Global,
}

pub(crate) fn parse(value: Value) -> Result<Command, String> {
    let fields = value
        .as_object()
        .ok_or("로컬 제어 요청은 객체여야 합니다.")?;
    let command: Command =
        serde_json::from_value(value.clone()).map_err(|error| error.to_string())?;
    // Internally tagged unit variants can ignore extra fields despite deny_unknown_fields.
    // Derive the accepted fields from this exact action instead of maintaining a second list.
    let accepted = serde_json::to_value(&command).map_err(|error| error.to_string())?;
    if let Some(field) = fields.keys().find(|field| accepted.get(*field).is_none()) {
        return Err(format!(
            "이 로컬 제어 동작에서 허용하지 않는 필드입니다: {field}"
        ));
    }
    match &command {
        Command::MemoryNote { note, .. } | Command::MemoryForget { note, .. }
            if note.trim().is_empty() || note.len() > 16 * 1024 =>
        {
            return Err(
                "메모리 정정은 비어 있지 않은 16 KiB 이하의 사용자 요청이어야 합니다.".into(),
            );
        }
        Command::CacheDiagnosticsSet {
            response: false,
            wire: true,
        } => {
            return Err(
                "Wire cache observation requires response observation; select both explicitly."
                    .into(),
            );
        }
        Command::ContextSet { tokens } if *tokens <= 0 => {
            return Err("문맥 크기는 양의 정수 토큰 수여야 합니다.".into());
        }
        Command::UiLanguage { language } => {
            codex_config::ui_language::UiLanguage::parse(language)
                .map_err(|error| error.to_string())?;
        }
        Command::ServiceTier { tier } if tier.trim().is_empty() => {
            return Err("status에서 확인한 지원 속도 식별자가 필요합니다.".into());
        }
        _ => {}
    }
    Ok(command)
}

pub(crate) fn command(value: Value) -> Result<String, String> {
    serde_json::to_string(&parse(value)?)
        .map(|json| format!("{PREFIX}{json}"))
        .map_err(|error| error.to_string())
}

pub(crate) fn from_command(command: &str) -> Result<Command, String> {
    let json = command
        .strip_prefix(PREFIX)
        .ok_or("로컬 제어 요청 형식이 잘못되었습니다.")?;
    parse(serde_json::from_str(json).map_err(|error| error.to_string())?)
}

pub(crate) fn is_read_only_command(command: &str) -> bool {
    matches!(
        from_command(command),
        Ok(Command::Status
            | Command::MemoryView
            | Command::ContextGet
            | Command::CleanupAgentsPreview
            | Command::LoggingStatus
            | Command::CacheDiagnosticsStatus)
    )
}

pub(crate) fn spec() -> DynamicToolFunctionSpec {
    DynamicToolFunctionSpec {
        name: NAME.into(),
        description: "사용자가 이 앱의 기능을 자연어로 요청하면 실행하는 로컬 제어 도구입니다. 메모리 보여줘/기억해/이 기억 정정해/잊어줘는 memory_view/memory_note/memory_forget, 메모리 통합 실행·재시도를 명시 요청하면 memory_consolidate(retry=false, 명시 재시도만 true)를 사용하고 memory_view로 선택 범위의 실제 작업 상태를 확인하세요. accepted는 완료가 아니고 작업 완료도 특정 노트 반영을 보장하지 않습니다. 사용자가 프로젝트 메모리에 저장하라고 명시하면 memory_note/forget에 scope=project, 전역 저장을 명시하면 scope=global을 함께 보내세요. 생략하면 현재 대화의 고정 범위에 저장됩니다. memory_scope는 새 대화 설정일 뿐 현재 노트의 저장 대상을 바꾸지 않습니다. 응답의 실제 scope/root와 pendingConsolidation을 확인하며 저장 대상 변경은 현재 읽기·통합 범위를 바꾸지 않습니다. 프로젝트 identity·신뢰·local 환경을 확인할 수 없으면 전역으로 대신 저장하지 마세요. 저장할 때 전역 공통 원칙과 프로젝트 상태를 구분하고 프로젝트 상태를 전역 지침으로 승격하지 마세요. 메모리 전체 활성화·비활성화 명시 요청은 memory_enable(enabled), 읽기·생성만 켜기/끄기는 memory_read/memory_generate(enabled), 프로젝트/전역 메모리는 memory_scope(scope), 이 대화 문맥 크기 조회·변경·초기화는 context_get/context_set(tokens)/context_reset, 대화 압축 요청은 compact, 화면 언어 한국어/영어로 바꿔줘는 ui_language(ko/en), 빠르게/ultrafast로 바꿔줘는 status로 현재 지원 tier를 확인한 뒤 service_tier(tier), 플러그인 다시 읽어줘는 reload_plugins를 사용하세요. slash 명령 입력을 사용자에게 떠넘기거나 셸로 설정·메모리 파일을 직접 고치지 마세요. 메모리 정정·망각은 후속 통합용 기록이며 요약·기존 대화가 즉시 바뀌거나 영구 삭제된 것이 아닙니다. memory_note/forget은 꺼진 기능을 자동으로 켜지 않습니다. memory_enable은 기능과 읽기·생성을 함께 바꾸고, 부분 설정은 다른 설정의 명시 off를 보존합니다. 읽기 변경은 이후 문맥, 범위 설정은 새 대화, 생성 변경은 새 처리에 적용되며 실행 중인 처리는 끝날 수 있습니다. compact는 현재 호출 턴 완료 뒤 기존 idle admission을 한 번 시도하며 pending 응답은 시작·완료가 아닙니다. busy나 대기 메시지가 있으면 실행하지 않고 하위 작업을 취소하지 않습니다. 문맥 변경은 현재 호출 대화의 다음 턴부터이며 모델 한도를 늘리지 않습니다. 화면 언어는 클라이언트 전역에 저장되어 다음 실행에 적용되고 환경변수가 우선합니다. 속도는 현재 모델이 지원하는 식별자만 사용하며 현재 대화에만 적용합니다. 플러그인 응답은 다시 연결 요청의 확인이며 서버 준비 완료가 아닙니다. 종료된 하위 작업 정리는 cleanup_agents_preview로 현재 대화 아래 후보만 조회하세요. 이 동작은 삭제하지 않으며 삭제 완료라고 말하지 마세요. 실제 영구 삭제는 미리보기의 정확한 대상과 기존 cleanup-agents 명시 확인 절차를 유지하세요. 이 도구에는 confirmed/force/임의 threadId/경로 입력이 없습니다. 로그 상태는 logging_status, 명시적인 로그 켜기/끄기는 logging_set(enabled), 캐시 관측 상태는 cache_diagnostics_status, 명시적인 응답·wire 관측 변경은 cache_diagnostics_set(response,wire)을 사용하세요. 일반 로그 on과 캐시 관측 on을 혼동하지 마세요. 두 설정은 로컬 클라이언트 전역에 저장되어 다음 실행부터 적용되며 현재 실행 중 수집이나 Drive 업로드 성공을 뜻하지 않습니다. 캐시 성능을 물었다고 관측을 자동 활성화하지 마세요. 설정 결과 유실 뒤 변경을 자동 재전송하지 말고 상태를 조회하세요. 예약은 별도 cron 도구를 사용하세요. 사용자의 명시 요청·정정을 보존하고 모호한 변경은 먼저 질문하세요. 실제 성공 응답 전 완료를 말하지 말고 시간초과·유실 뒤 변경을 자동 재전송하지 마세요.".into(),
        input_schema: json!({
            "type":"object", "additionalProperties":false,
            "properties": {
                "action":{"type":"string","enum":["status","memory_view","memory_consolidate","memory_enable","memory_note","memory_forget","memory_read","memory_generate","memory_scope","context_get","context_set","context_reset","compact","ui_language","service_tier","reload_plugins","cleanup_agents_preview","logging_status","logging_set","cache_diagnostics_status","cache_diagnostics_set"]},
                "note":{"type":"string","minLength":1,"description":"사용자가 명시한 추가·정정·망각 내용. 최대 16 KiB."},
                "enabled":{"type":"boolean"},
                "retry":{"type":"boolean","default":false,"description":"명시적인 메모리 통합 재시도 요청에만 true"},
                "response":{"type":"boolean"},
                "wire":{"type":"boolean"},
                "scope":{"type":"string","enum":["project","global"],"description":"memory_note/forget에서는 사용자가 명시한 저장 대상만 지정. 생략하면 현재 대화의 고정 범위. memory_scope는 이후 새 대화의 기본 범위 설정."},
                "tokens":{"type":"integer","minimum":1},
                "language":{"type":"string","enum":["ko","en"]},
                "tier":{"type":"string","minLength":1,"description":"status에서 확인한 현재 모델의 지원 속도 식별자 또는 default"}
            }, "required":["action"]
        }),
        defer_loading: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_note_scope_is_an_explicit_target_without_changing_the_scope_action() {
        assert_eq!(
            parse(json!({"action":"memory_note","note":"legacy"})).unwrap(),
            Command::MemoryNote {
                note: "legacy".into(),
                scope: None
            }
        );
        for (action, note) in [
            ("memory_note", "README 최신화"),
            ("memory_forget", "이전 프로젝트 상태"),
        ] {
            let value = json!({"action":action,"note":note,"scope":"project"});
            assert_eq!(
                from_command(&command(value.clone()).unwrap()).unwrap(),
                parse(value).unwrap()
            );
        }
        assert!(matches!(
            parse(json!({"action":"memory_scope","scope":"project"})).unwrap(),
            Command::MemoryScope {
                scope: Scope::Project
            }
        ));
        for value in [
            json!({"action":"memory_note","note":"correction","scope":"other"}),
            json!({"action":"memory_note","note":"correction","scope":"project","path":"other"}),
            json!({"action":"memory_note","note":"correction","scope":"project","threadId":"other"}),
            json!({"action":"memory_view","scope":"project"}),
        ] {
            assert!(parse(value).is_err());
        }
        assert!(
            spec()
                .description
                .contains("memory_scope는 새 대화 설정일 뿐")
        );
        assert!(spec().description.contains("scope=project"));
    }

    #[test]
    fn structured_controls_reject_foreign_targets_and_destructive_confirmation() {
        for value in [
            json!({"action":"memory_note","note":" "}),
            json!({"action":"context_set","tokens":0}),
            json!({"action":"context_set","tokens":1,"threadId":"other"}),
            json!({"action":"memory_scope","scope":"external"}),
            json!({"action":"ui_language","language":"invalid"}),
            json!({"action":"service_tier","tier":" "}),
            json!({"action":"cleanup_agents_preview","confirmed":true}),
            json!({"action":"cleanup_agents_preview","force":true}),
            json!({"action":"memory_reset"}),
            json!({"action":"memory_consolidate","threadId":"other"}),
            json!({"action":"memory_consolidate","retry":"true"}),
            json!({"action":"reload_plugins","path":"other"}),
            json!({"action":"status","threadId":"other"}),
            json!({"action":"memory_view","path":"other"}),
            json!({"action":"context_get","tokens":1}),
            json!({"action":"context_reset","tokens":1}),
            json!({"action":"compact","force":true}),
            json!({"action":"memory_enable"}),
            json!({"action":"logging_set"}),
            json!({"action":"logging_status","path":"other"}),
            json!({"action":"cache_diagnostics_set","response":false,"wire":true}),
            json!({"action":"cache_diagnostics_set","response":true}),
        ] {
            assert!(command(value.clone()).is_err(), "{value}");
        }
        for value in [
            json!({"action":"memory_consolidate"}),
            json!({"action":"memory_consolidate","retry":true}),
            json!({"action":"memory_forget","note":"이전 선호"}),
            json!({"action":"memory_generate","enabled":false}),
            json!({"action":"context_set","tokens":120000}),
            json!({"action":"ui_language","language":"ko"}),
            json!({"action":"cleanup_agents_preview"}),
            json!({"action":"compact"}),
            json!({"action":"memory_enable","enabled":true}),
            json!({"action":"logging_status"}),
            json!({"action":"logging_set","enabled":false}),
            json!({"action":"cache_diagnostics_status"}),
            json!({"action":"cache_diagnostics_set","response":true,"wire":true}),
        ] {
            let expected = parse(value.clone()).unwrap();
            assert_eq!(from_command(&command(value).unwrap()).unwrap(), expected);
        }
        assert!(is_read_only_command(
            &command(json!({"action":"memory_view"})).unwrap()
        ));
        assert!(!is_read_only_command(
            &command(json!({"action":"memory_note","note":"정정"})).unwrap()
        ));
    }
}
