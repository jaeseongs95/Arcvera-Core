use super::*;
use codex_models_manager::model_info::model_info_from_slug;
use tempfile::tempdir;

#[test]
fn build_stage_one_input_message_truncates_rollout_using_model_context_window() {
    let input = format!("{}{}{}", "a".repeat(700_000), "middle", "z".repeat(700_000));
    let mut model_info = model_info_from_slug("gpt-5.3-codex");
    model_info.context_window = Some(123_000);
    let expected_rollout_token_limit = usize::try_from(
        ((123_000_i64 * model_info.effective_context_window_percent) / 100)
            * crate::stage_one::CONTEXT_WINDOW_PERCENT
            / 100,
    )
    .unwrap();
    let expected_truncated = truncate_text(
        &input,
        TruncationPolicy::Tokens(expected_rollout_token_limit),
    );
    let message = build_stage_one_input_message(
        &model_info,
        Path::new("/tmp/rollout.jsonl"),
        Path::new("/tmp"),
        &input,
    )
    .unwrap();

    assert!(expected_truncated.contains("tokens truncated"));
    assert!(expected_truncated.starts_with('a'));
    assert!(expected_truncated.ends_with('z'));
    assert!(message.contains(&expected_truncated));
}

#[test]
fn build_stage_one_input_message_uses_default_limit_when_model_context_window_missing() {
    let input = format!("{}{}{}", "a".repeat(700_000), "middle", "z".repeat(700_000));
    let mut model_info = model_info_from_slug("gpt-5.3-codex");
    model_info.context_window = None;
    model_info.max_context_window = None;
    let expected_truncated = truncate_text(
        &input,
        TruncationPolicy::Tokens(crate::stage_one::DEFAULT_ROLLOUT_TOKEN_LIMIT),
    );
    let message = build_stage_one_input_message(
        &model_info,
        Path::new("/tmp/rollout.jsonl"),
        Path::new("/tmp"),
        &input,
    )
    .unwrap();

    assert!(message.contains(&expected_truncated));
}

#[test]
fn build_consolidation_prompt_points_to_workspace_diff_and_extension_tree() {
    let temp = tempdir().unwrap();
    let memory_root = temp.path().join("memories");
    let memory_extensions_root = memory_root.join("extensions");
    std::fs::create_dir_all(&memory_extensions_root).unwrap();

    let prompt = build_consolidation_prompt(&memory_root);

    assert!(prompt.contains("Memory workspace diff:"));
    assert!(prompt.contains("phase2_workspace_diff.md"));
    assert!(prompt.contains(&format!(
        "Memory extensions (under {}/):",
        memory_extensions_root.display()
    )));
    assert!(prompt.contains("workspace diff shows deleted extension resource files"));
}

#[test]
fn host_scope_contract_does_not_promote_project_history_or_temporary_permission() {
    use codex_config::types::MemoryScope;
    let global = memory_scope_instructions(MemoryScope::Global);
    let project = memory_scope_instructions(MemoryScope::Project);
    assert!(global.contains("Host memory scope: global"));
    assert!(global.contains("Do not copy a project's activity history into global memory"));
    assert!(project.contains("Never read or update another memory namespace"));
    let extraction = include_str!("../templates/memories/stage_one_system_v2.md");
    let consolidation = include_str!("../templates/memories/consolidation_v2.md");
    assert!(extraction.contains("Return empty strings when nothing meets"));
    assert!(extraction.contains("never authority"));
    assert!(consolidation.contains("Do not restore") || consolidation.contains("do not restore"));
    // This is a prompt-contract regression, not evidence that a model follows
    // the contract on the semantic usefulness and forgetting cases.
}
