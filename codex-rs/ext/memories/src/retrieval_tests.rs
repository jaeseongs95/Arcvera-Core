use super::*;
use codex_memories_read::publication::MemoryPublication;
use codex_memories_read::publication::PublicationStatus;
use codex_memories_read::publication::publication_path;
use codex_memories_read::publication::sha256;

fn text_input(text: &str) -> Vec<UserInput> {
    vec![UserInput::Text {
        text: text.to_string(),
        text_elements: Vec::new(),
    }]
}

fn publish_summary(root: &AbsolutePathBuf, summary: &[u8]) {
    let mut receipt = MemoryPublication::next_pending(None).unwrap();
    receipt.status = PublicationStatus::Published;
    receipt.published_generation = Some(receipt.accepted_generation);
    receipt
        .artifact_digests
        .insert("memory_summary.md".to_string(), sha256(summary));
    std::fs::write(
        publication_path(root.as_path()),
        serde_json::to_vec(&receipt).unwrap(),
    )
    .unwrap();
}

#[test]
fn mode_markers_are_small_and_contain_no_memory_payload() {
    let v1 = MemoryQueryBoundary::v1();
    assert_eq!(v1.content_kind().0, V1_QUERY_STATE_KIND);
    assert!(v1.0.len() < 256);
    let v2 = MemoryQueryBoundary::new(Some(&"a".repeat(64)), "namespace_unavailable", None);
    assert_eq!(v2.content_kind().0, QUERY_STATE_KIND);
    assert!(v2.0.len() < 1_024);
    assert!(v2.0.contains("namespace_unavailable"));
    assert!(!v2.0.contains("memory_summary.md"));
    assert!(!v2.0.contains("FORGOTTEN_DERIVED_CANARY"));
}

#[test]
fn independent_requests_and_generic_recall_do_not_inject_history() {
    for query in [
        "Translate R39 into Korean",
        "Remember to translate this sentence",
        "지난번 무엇을 했지",
        "Fix R39 memory",
        "R39 기능을 구현해 줘",
    ] {
        assert!(
            query_anchors(query).is_empty(),
            "unexpected retrieval for {query}"
        );
    }
    assert_eq!(query_anchors("이전 R39 결정은 무엇이었나"), vec!["r39"]);
    assert_eq!(
        query_anchors("Recall previous ProjectX decision"),
        vec!["projectx"]
    );
    assert!(!contains_anchor("r390 decision", "r39"));
    assert!(query_anchors(&"x".repeat(MAX_QUERY_BYTES + 1)).is_empty());
}

#[test]
fn global_preference_candidates_have_a_separate_complete_utf8_budget_and_no_duplicate_lines() {
    let summary = "v1\n## User preferences\n- 항상 한국어로 답하라. Stated default; source=user note; date=2026-11-01; R39.\n\n## General Tips\n- GENERAL_TIP_NOT_A_PREFERENCE\n\n## What's in Memory\n### ProjectA\n#### 2026-11-02\n- R39 temporary task state: PROJECT_TEMPORARY_CANARY\n";
    let ordinary = render_candidates("global", summary, &[], Some(1), true).unwrap();
    assert!(ordinary.0.contains("항상 한국어로 답하라"));
    assert!(ordinary.0.contains("2026-11-01"));
    assert!(
        ordinary
            .0
            .contains("semantics_independently_certified\":false")
    );
    assert!(!ordinary.0.contains("GENERAL_TIP_NOT_A_PREFERENCE"));
    assert!(!ordinary.0.contains("PROJECT_TEMPORARY_CANARY"));
    assert!(render_candidates("project", summary, &[], Some(1), false).is_none());
    assert!(render_candidates("legacy", summary, &[], None, true).is_none());
    let both = render_candidates("global", summary, &["r39".to_string()], Some(1), true).unwrap();
    assert_eq!(both.0.matches("항상 한국어로 답하라").count(), 1);
    assert!(both.0.contains("PROJECT_TEMPORARY_CANARY")); // Explicit prior-context candidate only.
    assert!(both.0.len() <= MAX_RETRIEVAL_BYTES);
    let payload: serde_json::Value =
        serde_json::from_str(both.0.split_once('\n').unwrap().1).unwrap();
    assert!(
        serde_json::to_vec(&payload["global_preference_candidates"])
            .unwrap()
            .len()
            <= MAX_GLOBAL_PREFERENCE_BYTES
    );
    let oversized = format!(
        "v1\n## User preferences\n- {}\n",
        "한".repeat(MAX_GLOBAL_PREFERENCE_BYTES)
    );
    assert!(render_candidates("global", &oversized, &[], Some(1), true).is_none());
    let nested = "v1\n## What's in Memory\n### User preferences\n- PROJECT_NESTED_CANARY\n";
    assert!(render_candidates("global", nested, &[], Some(1), true).is_none());
}

#[tokio::test]
async fn ordinary_query_uses_only_current_typed_global_preferences_in_the_next_input_item() {
    let fixture = tempfile::tempdir().unwrap();
    let root = AbsolutePathBuf::from_absolute_path_checked(fixture.path())
        .unwrap()
        .join("memories_v2");
    std::fs::create_dir_all(&root).unwrap();
    let summary = "v1\n## User preferences\n- 항상 한국어로 답하라. stated global default; source=synthetic user; date=2026-11-01.\n\n## What's in Memory\n### ProjectA\n- PROJECT_TEMPORARY_CANARY\n".as_bytes();
    std::fs::write(root.join("memory_summary.md"), summary).unwrap();
    let input = text_input("이 함수를 정리해 줘");
    let (_, legacy) = prepare_for_version(Some(&root), &input, true, MemoryVersion::V2, true).await;
    assert!(legacy.is_none());
    publish_summary(&root, summary);
    let (_, project) =
        prepare_for_version(Some(&root), &input, true, MemoryVersion::V2, false).await;
    assert!(
        project.is_none(),
        "typed project scope cannot be promoted by its path or section name"
    );
    let (_, candidate) =
        prepare_for_version(Some(&root), &input, true, MemoryVersion::V2, true).await;
    let item = Box::new(candidate.unwrap()).into_boxed_response_item();
    let wire = serde_json::to_string(&item).unwrap();
    assert!(wire.contains("항상 한국어로 답하라"));
    assert!(wire.contains(RETRIEVED_KIND));
    assert!(!wire.contains("PROJECT_TEMPORARY_CANARY"));
    let (large_state, large_candidate) = prepare_for_version(
        Some(&root),
        &text_input(&"x".repeat(MAX_QUERY_BYTES + 1)),
        true,
        MemoryVersion::V2,
        true,
    )
    .await;
    assert!(
        large_state
            .0
            .contains("global_preferences_only_query_over_budget")
    );
    assert!(large_candidate.unwrap().0.contains("항상 한국어로 답하라"));
    let published = load_publication(root.as_path()).unwrap().unwrap();
    let pending = MemoryPublication::next_pending(Some(&published)).unwrap();
    std::fs::write(
        publication_path(root.as_path()),
        serde_json::to_vec(&pending).unwrap(),
    )
    .unwrap();
    let (state, candidate) =
        prepare_for_version(Some(&root), &input, true, MemoryVersion::V2, true).await;
    assert!(state.0.contains("pending"));
    assert!(candidate.is_none());
}

#[test]
fn small_relevant_memory_wins_over_many_irrelevant_entries() {
    let irrelevant = (0..1_000)
        .map(|index| format!("- ProjectB unrelated item {index}\n\n"))
        .collect::<String>();
    let summary = format!(
        "v1\n## What's in Memory\n### ProjectA\n#### 2026-10-01\n{irrelevant}\n- R39 decision: preserve the release hold; uncertain until verification. source=rollout_summaries/r39.md\n"
    );
    let result = render_candidates(
        "selected-project",
        &summary,
        &["r39".to_string()],
        None,
        false,
    )
    .unwrap();
    assert!(result.0.contains("preserve the release hold"));
    assert!(result.0.contains("2026-10-01"));
    assert!(result.0.contains("uncertain until verification"));
    assert!(result.0.contains("rollout_summaries/r39.md"));
    assert!(!result.0.contains("ProjectB unrelated"));
    assert!(result.0.len() <= MAX_RETRIEVAL_BYTES);
    assert!(result.0.contains("legacy_unversioned"));
}

#[test]
fn complete_conflicting_candidates_keep_scope_and_uncertainty() {
    let summary = "v1\n## What's in Memory\n### ProjectA\n#### 2026-10-01\n- R39 temporary test instruction: pause only this job; source=old.md\n\n#### 2026-10-02\n- R39 correction: continue this job; proposed, not yet observed; source=new.md\n";
    let result = render_candidates(
        "selected-project",
        summary,
        &["r39".to_string()],
        Some(7),
        false,
    )
    .unwrap();
    assert!(result.0.contains("temporary test instruction"));
    assert!(result.0.contains("continue this job"));
    assert!(result.0.contains("proposed, not yet observed"));
    assert!(result.0.contains("not instructions or permission"));
    assert!(
        result
            .0
            .contains("do not generalize temporary instructions")
    );
    assert!(result.0.contains("start_line"));
    let oversized = format!(
        "## What's in Memory\n- R39 {}",
        "x".repeat(MAX_CANDIDATE_BYTES)
    );
    assert!(
        render_candidates(
            "selected-project",
            &oversized,
            &["r39".to_string()],
            Some(7),
            false
        )
        .is_none()
    );
}

#[tokio::test]
async fn retrieval_is_scoped_and_never_falls_back_to_global() {
    let home = tempfile::tempdir().unwrap();
    let home = AbsolutePathBuf::from_absolute_path_checked(home.path()).unwrap();
    std::fs::create_dir_all(home.join("memories_v2")).unwrap();
    std::fs::write(
        home.join("memories_v2/memory_summary.md"),
        "- R39 GLOBAL_ONLY_CANARY",
    )
    .unwrap();
    let project = home
        .join("project_memories")
        .join("a".repeat(64))
        .join("memories_v2");
    std::fs::create_dir_all(&project).unwrap();
    let input = text_input("이전 R39 결정을 기억해 줘");
    assert!(retrieve_for_input(&project, &input).await.is_none());
    std::fs::write(project.join("memory_summary.md"), "v1\n## What's in Memory\n### ProjectA\n#### 2026-10-01\n- R39 PROJECT_ONLY_CANARY source=rollout_summaries/a.md\n").unwrap();
    assert!(
        retrieve_for_input(&project, &input).await.is_none(),
        "unversioned v2 cannot claim current publication"
    );
    publish_summary(
        &project,
        &std::fs::read(project.join("memory_summary.md")).unwrap(),
    );
    let retrieved = retrieve_for_input(&project, &input).await.unwrap();
    assert!(retrieved.0.contains("PROJECT_ONLY_CANARY"));
    assert!(!retrieved.0.contains("GLOBAL_ONLY_CANARY"));
    let response = Box::new(retrieved).into_boxed_response_item();
    let wire = serde_json::to_string(&response).unwrap();
    assert!(wire.contains("PROJECT_ONLY_CANARY"));
    assert!(wire.contains(RETRIEVED_KIND));
    assert!(wire.contains("external_memory_candidates"));
}

#[tokio::test]
async fn pending_failed_or_changed_bytes_never_reintroduce_forgotten_derived_memory() {
    let fixture = tempfile::tempdir().unwrap();
    let root = AbsolutePathBuf::from_absolute_path_checked(fixture.path())
        .unwrap()
        .join("memories_v2");
    std::fs::create_dir_all(&root).unwrap();
    let old = b"v1\n## What's in Memory\n- R39 FORGOTTEN_DERIVED_CANARY\n";
    std::fs::write(root.join("memory_summary.md"), old).unwrap();
    let mut receipt = MemoryPublication::next_pending(None).unwrap();
    let input = text_input("이전 R39 결정을 기억해 줘");
    for status in [
        PublicationStatus::Pending,
        PublicationStatus::Processing,
        PublicationStatus::Failed,
        PublicationStatus::Resetting,
    ] {
        receipt.status = status;
        std::fs::write(
            publication_path(root.as_path()),
            serde_json::to_vec(&receipt).unwrap(),
        )
        .unwrap();
        assert!(retrieve_for_input(&root, &input).await.is_none());
    }
    receipt.status = PublicationStatus::Published;
    receipt.published_generation = Some(1);
    let corrected = b"v1\n## What's in Memory\n- R39 current supported decision; source=rollout_summaries/current.md\n";
    receipt
        .artifact_digests
        .insert("memory_summary.md".to_string(), sha256(corrected));
    std::fs::write(
        publication_path(root.as_path()),
        serde_json::to_vec(&receipt).unwrap(),
    )
    .unwrap();
    assert!(
        retrieve_for_input(&root, &input).await.is_none(),
        "old bytes must not inherit publication"
    );
    std::fs::write(root.join("memory_summary.md"), corrected).unwrap();
    let current = retrieve_for_input(&root, &input).await.unwrap();
    assert!(current.0.contains("current supported decision"));
    assert!(!current.0.contains("FORGOTTEN_DERIVED_CANARY"));
    assert!(current.0.contains("published_bytes"));
}

#[tokio::test]
async fn unavailable_publication_is_distinct_from_unrequested_and_empty_lookup() {
    let fixture = tempfile::tempdir().unwrap();
    let root = AbsolutePathBuf::from_absolute_path_checked(fixture.path())
        .unwrap()
        .join("memories_v2");
    std::fs::create_dir_all(&root).unwrap();
    let input = text_input("이전 R39 결정을 기억해 줘");
    let (legacy, candidate) = prepare_for_input(Some(&root), &input, true).await;
    assert!(legacy.0.contains("legacy_unverified"));
    assert!(!legacy.0.contains(&root.display().to_string()));
    assert!(candidate.is_none());
    let summary = b"v1\n## What's in Memory\n- R38 unrelated historical fact\n";
    std::fs::write(root.join("memory_summary.md"), summary).unwrap();
    publish_summary(&root, summary);
    let (unrequested, _) =
        prepare_for_input(Some(&root), &text_input("Translate this sentence"), true).await;
    assert!(
        unrequested
            .0
            .contains("no_global_preference_candidate_within_budget")
    );
    let (empty, _) = prepare_for_input(Some(&root), &input, true).await;
    assert!(empty.0.contains("no_matching_candidate_within_budget"));
    let receipt = load_publication(root.as_path()).unwrap().unwrap();
    let pending = MemoryPublication::next_pending(Some(&receipt)).unwrap();
    std::fs::write(
        publication_path(root.as_path()),
        serde_json::to_vec(&pending).unwrap(),
    )
    .unwrap();
    let (pending_state, candidate) = prepare_for_input(Some(&root), &input, true).await;
    assert!(pending_state.0.contains("pending"));
    assert!(pending_state.0.contains("\"accepted_generation\":2"));
    assert!(pending_state.0.contains("\"published_generation\":1"));
    assert!(candidate.is_none());
    std::fs::write(publication_path(root.as_path()), b"not json").unwrap();
    let (invalid, _) = prepare_for_input(Some(&root), &input, true).await;
    assert!(invalid.0.contains("invalid_receipt"));
    assert!(!invalid.0.contains("not json"));
}

#[tokio::test]
async fn dedicated_v2_tools_cannot_bypass_publication_via_old_rollout_inputs() {
    use crate::backend::ListMemoriesRequest;
    use crate::backend::MemoriesBackend;
    use crate::backend::MemoriesBackendError;
    use crate::backend::ReadMemoryRequest;
    use crate::backend::SearchMatchMode;
    use crate::backend::SearchMemoriesRequest;
    use crate::local::LocalMemoriesBackend;

    let fixture = tempfile::tempdir().unwrap();
    let root = AbsolutePathBuf::from_absolute_path_checked(fixture.path())
        .unwrap()
        .join("memories_v2");
    std::fs::create_dir_all(root.join("rollout_summaries")).unwrap();
    std::fs::write(
        root.join("rollout_summaries/old.md"),
        "FORGOTTEN_DERIVED_CANARY",
    )
    .unwrap();
    let summary =
        b"v1\n## What's in Memory\n- R39 current correction; earlier claim was withdrawn.\n";
    std::fs::write(root.join("memory_summary.md"), summary).unwrap();
    let backend = LocalMemoriesBackend::from_memory_root_and_version(
        root.to_path_buf(),
        codex_protocol::MemoryVersion::V2,
    );
    let read = || ReadMemoryRequest {
        path: "memory_summary.md".to_string(),
        line_offset: 1,
        max_lines: None,
        max_tokens: 1_000,
    };
    assert!(matches!(
        backend.read(read()).await,
        Err(MemoriesBackendError::MemoryUnavailable { .. })
    ));
    publish_summary(&root, summary);
    let result = backend.read(read()).await.unwrap();
    assert!(result.content.contains("current correction"));
    let mut old = read();
    old.path = "rollout_summaries/old.md".to_string();
    assert!(matches!(
        backend.read(old).await,
        Err(MemoriesBackendError::MemoryUnavailable { .. })
    ));
    let matches = backend
        .search(SearchMemoriesRequest {
            queries: vec!["FORGOTTEN_DERIVED_CANARY".to_string()],
            match_mode: SearchMatchMode::Any,
            path: None,
            cursor: None,
            context_lines: 0,
            case_sensitive: true,
            normalized: false,
            max_results: 10,
        })
        .await
        .unwrap();
    assert!(matches.matches.is_empty());
    let listed = backend
        .list(ListMemoriesRequest {
            path: None,
            cursor: None,
            max_results: 10,
        })
        .await
        .unwrap();
    assert_eq!(listed.entries.len(), 1);
    assert_eq!(listed.entries[0].path, "memory_summary.md");
    let published = load_publication(root.as_path()).unwrap().unwrap();
    let pending = MemoryPublication::next_pending(Some(&published)).unwrap();
    std::fs::write(
        publication_path(root.as_path()),
        serde_json::to_vec(&pending).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        backend.read(read()).await,
        Err(MemoriesBackendError::MemoryUnavailable { .. })
    ));
    // V1 remains a separately selected legacy interface.
    let v1 = LocalMemoriesBackend::from_memory_root_and_version(
        root.to_path_buf(),
        codex_protocol::MemoryVersion::V1,
    );
    assert!(
        matches!(
            v1.read(read()).await,
            Err(MemoriesBackendError::MemoryUnavailable { .. })
        ),
        "versioned v1 must not bypass a pending correction"
    );
}

#[tokio::test]
async fn legacy_v1_and_versioned_v1_have_distinct_publication_contracts() {
    use crate::backend::{
        ListMemoriesRequest, MemoriesBackend, MemoriesBackendError, ReadMemoryRequest,
        SearchMatchMode, SearchMemoriesRequest,
    };
    use crate::local::LocalMemoriesBackend;
    use codex_memories_read::publication::{
        collect_published_artifacts, published_artifact_digests,
    };
    let fixture = tempfile::tempdir().unwrap();
    let root = AbsolutePathBuf::from_absolute_path_checked(fixture.path())
        .unwrap()
        .join("memories");
    std::fs::create_dir_all(root.join("skills/project-helper")).unwrap();
    std::fs::create_dir_all(root.join("rollout_summaries")).unwrap();
    let summary = b"v1\n## What's in Memory\n### ProjectA\n#### 2026-10-02\n- R39 current useful decision; source=repo/decision.md\n";
    std::fs::write(root.join("memory_summary.md"), summary).unwrap();
    std::fs::write(root.join("MEMORY.md"), "current detailed decision\n").unwrap();
    std::fs::write(
        root.join("skills/project-helper/SKILL.md"),
        "CURRENT_SKILL_EVIDENCE\n",
    )
    .unwrap();
    std::fs::write(root.join("rollout_summaries/old.md"), "OLD_RAW_CANARY\n").unwrap();
    let backend = LocalMemoriesBackend::from_memory_root_and_version(
        root.to_path_buf(),
        codex_protocol::MemoryVersion::V1,
    );
    let read = |path: &str| ReadMemoryRequest {
        path: path.to_string(),
        line_offset: 1,
        max_lines: None,
        max_tokens: 1000,
    };
    assert!(
        backend
            .read(read("rollout_summaries/old.md"))
            .await
            .unwrap()
            .content
            .contains("OLD_RAW_CANARY")
    );
    let input = text_input("previous R39 decision");
    let (legacy, candidate) =
        prepare_for_version(Some(&root), &input, true, MemoryVersion::V1, true).await;
    assert_eq!(legacy.content_kind().0, V1_QUERY_STATE_KIND);
    assert!(candidate.is_none());
    let mut publication = MemoryPublication::next_pending(None).unwrap();
    std::fs::write(
        publication_path(root.as_path()),
        serde_json::to_vec(&publication).unwrap(),
    )
    .unwrap();
    let (pending, candidate) =
        prepare_for_version(Some(&root), &input, true, MemoryVersion::V1, true).await;
    assert_eq!(pending.content_kind().0, VERSIONED_V1_QUERY_STATE_KIND);
    assert!(candidate.is_none());
    assert!(matches!(
        backend.read(read("MEMORY.md")).await,
        Err(MemoriesBackendError::MemoryUnavailable { .. })
    ));
    let artifacts = collect_published_artifacts(root.as_path(), MemoryVersion::V1).unwrap();
    assert_eq!(artifacts.len(), 3);
    assert!(!artifacts.contains_key("rollout_summaries/old.md"));
    publication.status = PublicationStatus::Published;
    publication.published_generation = Some(publication.accepted_generation);
    publication.artifact_digests = published_artifact_digests(&artifacts);
    std::fs::write(
        publication_path(root.as_path()),
        serde_json::to_vec(&publication).unwrap(),
    )
    .unwrap();
    assert!(
        backend
            .read(read("skills/project-helper/SKILL.md"))
            .await
            .unwrap()
            .content
            .contains("CURRENT_SKILL_EVIDENCE")
    );
    assert!(matches!(
        backend.read(read("rollout_summaries/old.md")).await,
        Err(MemoriesBackendError::MemoryUnavailable { .. })
    ));
    let list = backend
        .list(ListMemoriesRequest {
            path: None,
            cursor: None,
            max_results: 10,
        })
        .await
        .unwrap();
    assert_eq!(list.entries.len(), 3);
    let found = backend
        .search(SearchMemoriesRequest {
            queries: vec!["CURRENT_SKILL_EVIDENCE".to_string()],
            match_mode: SearchMatchMode::Any,
            path: None,
            cursor: None,
            context_lines: 0,
            case_sensitive: true,
            normalized: false,
            max_results: 10,
        })
        .await
        .unwrap();
    assert_eq!(found.matches.len(), 1);
    assert_eq!(found.matches[0].path, "skills/project-helper/SKILL.md");
    let (current, candidate) =
        prepare_for_version(Some(&root), &input, true, MemoryVersion::V1, true).await;
    assert_eq!(current.content_kind().0, VERSIONED_V1_QUERY_STATE_KIND);
    assert!(candidate.unwrap().0.contains("current useful decision"));
    std::fs::write(
        root.join("skills/project-helper/SKILL.md"),
        "unexpected later modification",
    )
    .unwrap();
    assert!(
        matches!(
            backend.read(read("MEMORY.md")).await,
            Err(MemoriesBackendError::MemoryUnavailable { .. })
        ),
        "the complete artifact set must stay bound to the generation"
    );
}
