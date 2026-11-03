// Modified for OpenDIY Codex R36 (2026-11-03); see MODIFICATIONS.md.
use super::*;
use codex_app_server_protocol::CompletedChildCleanupResult;

#[test]
fn cleanup_summary_counts_preservation_reasons_and_accepts_older_responses() {
    let preview = ThreadCleanupCompletedResponse {
        results: [
            "writer_owned",
            "writer_owned",
            "terminal_turn_unverified",
            "",
        ]
        .into_iter()
        .map(|reason| CompletedChildCleanupResult {
            thread_id: "child".into(),
            outcome: "preserved".into(),
            reason: reason.into(),
            snapshot: None,
        })
        .collect(),
    };
    for locale in [Locale::English, Locale::Korean] {
        let mut output = Vec::new();
        write_cleanup_reasons(&preview, locale, &mut output).unwrap();
        let output = String::from_utf8(output).unwrap();
        assert_eq!(output.lines().count(), 3);
        assert!(
            output
                .lines()
                .any(|line| line.contains("writer_owned") && line.contains('2'))
        );
        assert!(
            output
                .lines()
                .any(|line| line.contains("terminal_turn_unverified") && line.contains('1'))
        );
        assert!(
            output
                .lines()
                .any(|line| line.contains("preserved / preserved") && line.contains('1'))
        );
    }
    let older: CompletedChildCleanupResult =
        serde_json::from_str(r#"{"threadId":"child","outcome":"preserved","snapshot":null}"#)
            .unwrap();
    assert!(older.reason.is_empty());
}

#[test]
fn cleanup_confirmation_uses_selected_language_and_requires_the_displayed_token() {
    for (locale, token, other_token, expected_prompt) in [
        (
            Locale::English,
            "cleanup",
            "정리",
            "permanently delete the 2 conversations",
        ),
        (Locale::Korean, "정리", "cleanup", "2개 대화를 영구 삭제"),
    ] {
        for (answer, accepted) in [
            (format!(" {token}\r\n"), true),
            (format!("{other_token}\n"), false),
            (String::new(), false),
            ("yes\n".into(), false),
        ] {
            let mut output = Vec::new();
            assert_eq!(
                confirm_cleanup(true, &mut answer.as_bytes(), &mut output, 2, locale).unwrap(),
                accepted,
            );
            let prompt = String::from_utf8(output).unwrap();
            assert!(prompt.contains(expected_prompt), "{prompt}");
            assert!(prompt.contains(&format!("'{token}'")), "{prompt}");
            assert!(!prompt.contains(other_token), "{prompt}");
        }
        let mut output = Vec::new();
        let error = confirm_cleanup(false, &mut token.as_bytes(), &mut output, 2, locale)
            .unwrap_err()
            .to_string();
        assert!(error.contains("--apply --confirm"));
        assert!(error.contains(match locale {
            Locale::English => "requires terminal confirmation",
            Locale::Korean => "터미널 확인",
        }));
        assert!(output.is_empty());
    }
}

#[test]
fn cleanup_selection_and_partial_failure_preserve_guards_in_both_languages() {
    let snapshot = CompletedChildSnapshot {
        thread_id: "finished-child".into(),
        runtime_id: "runtime".into(),
        activity_revision: 7,
        last_turn_id: "last-turn".into(),
        terminal_status: "completed".into(),
    };
    let deleted = CompletedChildCleanupResult {
        thread_id: snapshot.thread_id.clone(),
        outcome: "deleted".into(),
        reason: "deleted".into(),
        snapshot: Some(snapshot.clone()),
    };
    let preserved = CompletedChildCleanupResult {
        thread_id: "parent".into(),
        outcome: "preserved".into(),
        reason: "ownership_unverified".into(),
        snapshot: None,
    };
    let preview = ThreadCleanupCompletedResponse {
        results: vec![
            CompletedChildCleanupResult {
                outcome: "eligible".into(),
                ..deleted.clone()
            },
            preserved.clone(),
            CompletedChildCleanupResult {
                thread_id: "current".into(),
                ..preserved.clone()
            },
        ],
    };
    for locale in [Locale::English, Locale::Korean] {
        // Only the exact eligible snapshot is forwarded; protected rows have no snapshot.
        assert_eq!(
            select_cleanup_snapshots(preview.clone(), &[], locale).unwrap(),
            vec![snapshot.clone()]
        );
        for id in ["parent", "current", "unknown", "unfinished-child"] {
            let error = select_cleanup_snapshots(preview.clone(), &[id.into()], locale)
                .unwrap_err()
                .to_string();
            assert!(error.ends_with(id));
            assert!(error.contains(match locale {
                Locale::English => "terminal state could not be confirmed",
                Locale::Korean => "종료 상태를 확정할 수 없어 보존",
            }));
        }
        assert!(
            check_cleanup_result(
                &ThreadCleanupCompletedResponse {
                    results: vec![deleted.clone()]
                },
                locale,
            )
            .is_ok()
        );
        let partial = ThreadCleanupCompletedResponse {
            results: vec![deleted.clone(), preserved.clone()],
        };
        let error = check_cleanup_result(&partial, locale)
            .unwrap_err()
            .to_string();
        assert!(error.contains(match locale {
            Locale::English => "Some items were preserved",
            Locale::Korean => "일부 항목을 보존",
        }));
        assert!(
            check_cleanup_result(
                &ThreadCleanupCompletedResponse {
                    results: vec![preserved.clone()]
                },
                locale,
            )
            .is_err()
        );
    }
}
