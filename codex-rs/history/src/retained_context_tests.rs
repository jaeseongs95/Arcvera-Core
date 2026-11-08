use super::*;
use crate::CodexHarnessMetadata;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use pretty_assertions::assert_eq;

fn adoption_message(id: &str, turn: &str, text: &str) -> RetainedUserMessage {
    RetainedUserMessage {
        turn_id: turn.to_owned(),
        message_id: Some(id.to_owned()),
        text: text.to_owned(),
        complete: true,
        origin: crate::UserInputOrigin::User,
        phase: None,
    }
}

fn user_boundary(id: &str, turn: &str) -> RetainedSourceId {
    RetainedSourceId {
        message_id: id.to_owned(),
        turn_id: turn.to_owned(),
        role: RetainedSourceRole::User,
    }
}

fn refork_legacy_worker(nonempty: bool) -> (RetainedContext, Option<RetainedSource>) {
    let mut parent = RetainedContext::default();
    parent.adopt_user_messages(false);
    let mut old = RetainedContext::for_worker_fork(Some(&parent), true);
    let source = nonempty.then(|| {
        old.record_user_message(
            adoption_message("old-local", "old-turn", "Never publish."),
            RetainedInputSource::Local(Some(0)),
        )
        .unwrap()
    });
    let mut wire = serde_json::to_value(old).unwrap();
    for field in ["adoption_user_messages", "standalone_root_provenance"] {
        wire.as_object_mut().unwrap().remove(field);
        assert!(wire.get(field).is_none());
    }
    assert_eq!(wire["user_messages_incomplete"], false);
    assert_eq!(wire["incomplete"], false);
    let checkpoint = serde_json::from_value(wire).unwrap();
    let mut worker = RetainedContext::default();
    worker.restore(Some(&checkpoint), &[]);
    assert!(!worker.has_missing_user_messages());
    assert!(worker.user_messages_complete());
    (worker, source)
}

fn assert_legacy_refork(nonempty: bool) {
    for forks in [1, 2] {
        let (mut worker, source) = refork_legacy_worker(nonempty);
        for _ in 0..forks {
            worker = RetainedContext::for_worker_fork(Some(&worker), true);
            assert!(worker.user_messages_complete());
            assert_eq!(worker.ordered_entries().count(), 0);
            let checkpoint = serde_json::from_slice(&serde_json::to_vec(&worker).unwrap()).unwrap();
            worker.restore(Some(&checkpoint), &[]);
        }
        let checkpoint = serde_json::from_slice(&serde_json::to_vec(&worker).unwrap()).unwrap();
        let mut root = RetainedContext::default();
        root.restore(Some(&checkpoint), &[]);
        root.adopt_user_messages(true);
        assert!(
            root.has_missing_user_messages(),
            "nonempty={nonempty}, forks={forks}"
        );
        assert!(!root.user_messages_complete());
        assert!(!root.verified_answers_complete());
        assert_eq!(root.verified_answers().count(), 0);
        assert_eq!(root.user_messages.len(), usize::from(nonempty));
        if let Some(source) = source {
            let inherited = &root.user_messages[0];
            assert_eq!(inherited.value.message_id.as_deref(), Some("old-local"));
            assert_eq!(inherited.value.turn_id, "old-turn");
            assert_eq!(inherited.value.text, "Never publish.");
            assert_eq!(inherited.revision.as_ref(), Some(&source.revision));
            assert_eq!(inherited.key(), RetainedContextOrder::Inherited(0));
            assert!(!inherited.value.complete);
            root.record_user_message(
                adoption_message("new-local", "new-turn", "Inspect only."),
                RetainedInputSource::Local(Some(0)),
            );
            assert_eq!(root.user_messages[1].key(), RetainedContextOrder::Local(0));
            assert!(root.user_messages[1].value.complete);
        }
        let expected = root.clone();
        for _ in 0..2 {
            let checkpoint = serde_json::from_slice(&serde_json::to_vec(&root).unwrap()).unwrap();
            root.restore(Some(&checkpoint), &[]);
            root.adopt_user_messages(true);
            assert_eq!(root, expected);
        }
    }
}

#[test]
fn adoption_refork_legacy_empty_retains_missing_prefix() {
    assert_legacy_refork(false);
}

#[test]
fn adoption_refork_legacy_nonempty_retains_missing_prefix() {
    assert_legacy_refork(true);
}

#[test]
fn adoption_refork_restored_suffix_does_not_certify_legacy_prefix() {
    let (mut worker, _) = refork_legacy_worker(false);
    let message = adoption_message("suffix", "suffix-turn", "Ask me first.");
    let mut original = RetainedContext::default();
    let source = original
        .record_user_message(message.clone(), RetainedInputSource::Local(Some(0)))
        .unwrap();
    worker.record_user_message_for_adoption(message, Some(&source));
    assert!(worker.user_messages_complete());
    let checkpoint = serde_json::from_slice(&serde_json::to_vec(&worker).unwrap()).unwrap();
    let mut root = RetainedContext::default();
    root.restore(Some(&checkpoint), &[]);
    root.adopt_user_messages(true);
    assert!(root.has_missing_user_messages());
    assert!(!root.user_messages_complete());
    assert_eq!(root.user_messages.len(), 1);
    assert_eq!(root.user_messages[0].value.text, "Ask me first.");
    assert_eq!(
        root.user_messages[0].revision.as_ref(),
        Some(&source.revision)
    );
    assert!(!root.user_messages[0].value.complete);
    assert!(!root.verified_answers_complete());
    assert_eq!(root.verified_answers().count(), 0);
}

#[test]
fn adoption_refork_modern_empty_lineages_stay_complete() {
    let mut root = RetainedContext::default();
    root.adopt_user_messages(false);
    let modern_worker = RetainedContext::for_worker_fork(Some(&root), true);
    for mut parent in [root, modern_worker] {
        for _ in 0..2 {
            let checkpoint = serde_json::from_slice(&serde_json::to_vec(&parent).unwrap()).unwrap();
            parent.restore(Some(&checkpoint), &[]);
            parent = RetainedContext::for_worker_fork(Some(&parent), true);
        }
        let checkpoint = serde_json::from_slice(&serde_json::to_vec(&parent).unwrap()).unwrap();
        let mut root = RetainedContext::default();
        root.restore(Some(&checkpoint), &[]);
        root.adopt_user_messages(true);
        assert!(!root.has_missing_user_messages());
        assert!(root.user_messages_complete());
        assert_eq!(root.user_messages.len(), 0);
    }
}

#[test]
fn adoption_rollback_keeps_same_id_in_other_turn() {
    for mode in ["backup", "adopted", "local"] {
        for boundary_id in ["same-id", "unknown-id"] {
            let mut context = RetainedContext::default();
            for (turn, text) in [("turn-A", "Never publish."), ("turn-B", "Inspect only.")] {
                let message = adoption_message("same-id", turn, text);
                if mode == "local" {
                    context.record_user_message(message, RetainedInputSource::Local(None));
                } else {
                    context.record_user_message_for_adoption(message, None);
                }
            }
            if mode == "adopted" {
                context.adopt_user_messages(true);
            }
            let checkpoint =
                serde_json::from_slice(&serde_json::to_vec(&context).unwrap()).unwrap();
            let mut restored = RetainedContext::default();
            restored.restore(Some(&checkpoint), &[]);
            restored.rollback(
                &["turn-B"],
                Some(&user_boundary(boundary_id, "turn-B")),
                if mode == "local" {
                    RetainedInputSource::Local(None)
                } else {
                    RetainedInputSource::Inherited
                },
            );
            if mode == "backup" {
                let checkpoint =
                    serde_json::from_slice(&serde_json::to_vec(&restored).unwrap()).unwrap();
                restored.restore(Some(&checkpoint), &[]);
                restored.adopt_user_messages(true);
            }
            assert_eq!(
                restored
                    .ordered_entries()
                    .filter_map(|(_, entry)| match entry {
                        RetainedContextEntry::UserMessage(message) => Some((
                            message.message_id.as_deref(),
                            message.turn_id.as_str(),
                            message.text.as_str()
                        )),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
                vec![(Some("same-id"), "turn-A", "Never publish.")],
                "mode={mode}, boundary={boundary_id}",
            );
        }
    }
}

#[test]
fn adoption_checkpoint_preserves_sources_without_worker_local_authorization() {
    let mut parent = RetainedContext::default();
    parent.record(&publish_answer());
    let source = parent
        .record_user_message(
            adoption_message("parent", "parent-turn", "Never publish publicly."),
            RetainedInputSource::Local(Some(7)),
        )
        .unwrap();
    let mut worker = RetainedContext::for_worker_fork(Some(&parent), true);
    assert_eq!(worker.ordered_entries().count(), 0);
    assert!(worker.user_messages_complete());
    assert!(worker.verified_answers_complete());
    worker.record_user_message(
        adoption_message("local", "worker-turn", "Ask me first."),
        RetainedInputSource::Local(Some(0)),
    );
    let wire = serde_json::to_vec(&worker).unwrap();
    let checkpoint: RetainedContext = serde_json::from_slice(&wire).unwrap();
    let mut root = RetainedContext::default();
    root.restore(Some(&checkpoint), &[]);
    root.adopt_user_messages(true);
    assert_eq!(
        root.user_messages
            .iter()
            .map(|entry| (
                entry.value.message_id.as_deref(),
                entry.value.text.as_str(),
                entry.value.complete,
                entry.key()
            ))
            .collect::<Vec<_>>(),
        vec![
            (
                Some("parent"),
                "Never publish publicly.",
                false,
                RetainedContextOrder::Inherited(0)
            ),
            (
                Some("local"),
                "Ask me first.",
                true,
                RetainedContextOrder::Local(0)
            ),
        ]
    );
    assert_eq!(
        root.user_messages[0].revision.as_ref(),
        Some(&source.revision)
    );
    assert!(root.verified_answers().next().is_none());
    assert!(!root.verified_answers_complete());
    assert!(!root.user_messages_complete());
    let expected = root.clone();
    root.adopt_user_messages(true);
    root.restore(Some(&expected), &[]);
    root.adopt_user_messages(true);
    assert_eq!(root, expected);
}

#[test]
fn adoption_merges_partial_sources_without_downgrading_or_changing_turn_identity() {
    let mut parent = RetainedContext::default();
    parent.record_user_message(
        adoption_message("same-id", "first-turn", "Never publish."),
        RetainedInputSource::Local(Some(0)),
    );
    parent.record_user_message(
        adoption_message("other-id", "second-turn", "Inspect only."),
        RetainedInputSource::Local(Some(1)),
    );
    let mut root = RetainedContext::for_worker_fork(Some(&parent), true);
    let original = root
        .record_user_message(
            adoption_message("same-id", "first-turn", "Never publish."),
            RetainedInputSource::Inherited,
        )
        .unwrap();
    root.record_user_message(
        adoption_message("same-id", "local-turn", "Local restriction."),
        RetainedInputSource::Local(Some(0)),
    );
    root.adopt_user_messages(true);
    assert_eq!(root.user_messages.len(), 3);
    assert_eq!(root.user_messages[0].value.text, "Never publish.");
    assert!(root.user_messages[0].value.complete);
    assert_eq!(
        root.user_messages[0].revision.as_ref(),
        Some(&original.revision)
    );
    assert_eq!(root.user_messages[1].value.text, "Inspect only.");
    assert_eq!(root.user_messages[2].value.turn_id, "local-turn");
}

#[test]
fn adoption_bound_omissions_and_rollback_survive_checkpoint() {
    let mut worker = RetainedContext::default();
    for i in 0..10 {
        worker.record_user_message_for_adoption(
            adoption_message(
                &format!("source-{i}"),
                &format!("turn-{i}"),
                &format!("Restriction {i}"),
            ),
            None,
        );
    }
    assert_eq!(worker.ordered_entries().count(), 0);
    assert!(worker.user_messages_complete());
    assert_eq!(
        worker
            .adoption_user_messages
            .as_ref()
            .unwrap()
            .messages
            .len(),
        8
    );
    worker.record_user_message(
        adoption_message("local", "local-turn", "Do not deploy."),
        RetainedInputSource::Local(Some(0)),
    );
    worker.rollback(
        &["local-turn"],
        Some(&user_boundary("local", "local-turn")),
        RetainedInputSource::Local(Some(0)),
    );
    assert_eq!(
        worker
            .adoption_user_messages
            .as_ref()
            .unwrap()
            .messages
            .len(),
        8
    );
    worker.rollback(
        &["turn-7", "turn-8", "turn-9"],
        Some(&user_boundary("source-7", "turn-7")),
        RetainedInputSource::Inherited,
    );
    let wire = serde_json::to_vec(&worker).unwrap();
    let checkpoint = serde_json::from_slice(&wire).unwrap();
    let mut root = RetainedContext::default();
    root.restore(Some(&checkpoint), &[]);
    root.adopt_user_messages(true);
    assert!(root.has_missing_user_messages());
    assert_eq!(
        root.user_messages
            .iter()
            .map(|entry| entry.value.message_id.as_deref().unwrap())
            .collect::<Vec<_>>(),
        vec!["source-2", "source-3", "source-4", "source-5", "source-6"]
    );
    assert!(!root.verified_answers_complete());
    assert!(root.user_messages.iter().all(|entry| !entry.value.complete));
}

#[test]
fn adoption_old_missing_backup_with_local_messages_remains_incomplete() {
    let mut local = RetainedContext::default();
    local.record_user_message(
        adoption_message("local-id", "local-turn", "Inspect only."),
        RetainedInputSource::Local(Some(0)),
    );
    assert!(local.user_messages_complete());
    assert!(local.verified_answers_complete());
    let mut wire = serde_json::to_value(&local).unwrap();
    wire.as_object_mut()
        .unwrap()
        .remove("adoption_user_messages");
    wire.as_object_mut()
        .unwrap()
        .remove("standalone_root_provenance");
    let expected_source = wire["user_messages"].clone();
    for _ in 0..3 {
        let checkpoint = serde_json::from_value(wire).unwrap();
        let mut root = RetainedContext::default();
        root.restore(Some(&checkpoint), &[]);
        root.adopt_user_messages(true);
        assert!(
            root.has_missing_user_messages(),
            "legacy local source cannot prove the missing inherited prefix"
        );
        assert!(!root.user_messages_complete());
        assert!(
            !root.verified_answers_complete(),
            "legacy local source cannot prove absent parent answers"
        );
        assert_eq!(root.verified_answers().count(), 0);
        wire = serde_json::to_value(&root).unwrap();
        assert_eq!(wire["user_messages"], expected_source);
    }
}

#[test]
fn adoption_preserves_known_root_completeness_and_legacy_root_facts() {
    let mut root = RetainedContext::default();
    root.adopt_user_messages(false);
    root.record_user_message(
        adoption_message("root-local", "root-turn", "Never publish."),
        RetainedInputSource::Local(Some(0)),
    );
    root.record(&publish_answer());
    let expected = root.clone();
    let checkpoint = serde_json::from_value(serde_json::to_value(&root).unwrap()).unwrap();
    root.restore(Some(&checkpoint), &[]);
    root.adopt_user_messages(true);
    assert_eq!(root, expected);
    assert!(root.user_messages_complete());
    assert!(root.verified_answers_complete());

    // Older checkpoints cannot prove whether they were roots or workers. Keep
    // every local fact, but conservatively disclose that provenance gap.
    let mut wire = serde_json::to_value(&root).unwrap();
    wire.as_object_mut()
        .unwrap()
        .remove("standalone_root_provenance");
    let expected_messages = wire["user_messages"].clone();
    let expected_answers = wire["verified_answers"].clone();
    let checkpoint = serde_json::from_value(wire).unwrap();
    root.restore(Some(&checkpoint), &[]);
    root.adopt_user_messages(true);
    assert!(root.has_missing_user_messages());
    assert!(!root.verified_answers_complete());
    let wire = serde_json::to_value(root).unwrap();
    assert_eq!(wire["user_messages"], expected_messages);
    assert_eq!(wire["verified_answers"], expected_answers);
}

#[test]
fn adoption_old_missing_backup_and_oversized_sources_remain_incomplete() {
    let mut old = RetainedContext::default();
    old.adopt_user_messages(true);
    assert!(old.has_missing_user_messages());
    assert!(!old.verified_answers_complete());
    let mut worker = RetainedContext::default();
    worker.record_user_message_for_adoption(
        adoption_message("large", "turn", "x".repeat(MAX_RECORD_BYTES + 1).as_str()),
        None,
    );
    assert_eq!(worker.ordered_entries().count(), 0);
    worker.adopt_user_messages(true);
    assert_eq!(worker.user_messages.len(), 1);
    assert!(worker.user_messages[0].value.text.is_empty());
    assert!(!worker.user_messages_complete());
    assert!(!worker.verified_answers_complete());
}

#[test]
fn adoption_rejects_mismatched_host_source_identity() {
    let mut parent = RetainedContext::default();
    let source = parent
        .record_user_message(
            adoption_message("original", "turn", "Never publish."),
            RetainedInputSource::Local(Some(0)),
        )
        .unwrap();
    for message in [
        adoption_message("forged", "turn", "Publish now."),
        adoption_message("original", "other-turn", "Publish now."),
    ] {
        let mut worker = RetainedContext::default();
        worker.record_user_message_for_adoption(message, Some(&source));
        assert!(worker.user_messages_complete());
        assert_eq!(worker.ordered_entries().count(), 0);
        worker.adopt_user_messages(true);
        assert_eq!(worker.ordered_entries().count(), 0);
        assert!(worker.has_missing_user_messages());
        assert!(!worker.verified_answers_complete());
    }
}

fn publish_answer() -> RetainedContextEvent {
    RetainedContextEvent::VerifiedAnswer {
        answer: VerifiedAnswer {
            turn_id: "turn-1".to_owned(),
            call_id: "ask-1".to_owned(),
            questions: vec![VerifiedQuestionAnswer {
                question: "Publish?".to_owned(),
                answer: "Yes, but never publicly.".to_owned(),
            }],
        },
        acceptance_order: None,
    }
}

fn delivered_message(id: &str, text: &str, acceptance_order: u64) -> RetainedContextEvent {
    RetainedContextEvent::DeliveredAssistantMessage {
        message: RetainedUserMessage {
            origin: crate::UserInputOrigin::User,
            turn_id: "turn-1".to_owned(),
            message_id: Some(id.to_owned()),
            text: text.to_owned(),
            complete: true,
            phase: None,
        },
        acceptance_order,
    }
}

#[test]
fn retained_evidence_preserves_order_through_recovery_checkpoint_and_rollback() {
    let mut context = RetainedContext::default();
    let first = publish_answer();
    assert!(context.record(&first));
    let before_restriction = context.clone();
    context.record_user_message(
        RetainedUserMessage {
            phase: None,
            origin: crate::UserInputOrigin::User,
            turn_id: "revocation-turn".to_owned(),
            message_id: Some("revocation".to_owned()),
            text: String::new(),
            complete: false,
        },
        RetainedInputSource::Local(None),
    );
    let mut expected = context.clone();
    expected.user_messages[0].value.text = "Do not publish after all.".to_owned();
    context.recover_user_message_excerpts(|id| {
        assert_eq!(id, "revocation");
        Some("Do not publish after all.".to_owned())
    });
    assert_eq!(context, expected);
    context.recover_user_message_excerpts(|_| panic!("existing text must not be replaced"));
    let snapshot = context.clone();
    assert!(!context.record(&first));
    assert_eq!(context, snapshot);
    let checkpoint =
        serde_json::from_str(&serde_json::to_string(&context).expect("retained answer fixture"))
            .expect("retained answer fixture");
    let mut restored = RetainedContext::default();
    restored.restore(Some(&checkpoint), &[]);
    assert_eq!(restored, snapshot);
    assert_eq!(
        restored
            .ordered_entries()
            .map(|(_, entry)| match entry {
                RetainedContextEntry::VerifiedAnswer(answer) => answer.questions[0].answer.as_str(),
                RetainedContextEntry::UserMessage(message)
                | RetainedContextEntry::AssistantMessage(message) => message.text.as_str(),
            })
            .collect::<Vec<_>>(),
        vec!["Yes, but never publicly.", "Do not publish after all."]
    );
    restored.rollback(
        &["revocation-turn"],
        Some(&user_boundary("revocation", "revocation-turn")),
        RetainedInputSource::Local(None),
    );
    assert_eq!(
        restored,
        RetainedContext {
            next_order: snapshot.next_order,
            ..before_restriction
        }
    );
}

#[test]
fn retained_families_enforce_storage_limits_without_changing_snapshots() {
    let mut context = RetainedContext::default();
    let first = publish_answer();
    context.record(&first);
    let snapshot = context.clone();

    for index in 2..=10 {
        context.record(&RetainedContextEvent::VerifiedAnswer {
            answer: VerifiedAnswer {
                turn_id: "turn-2".to_owned(),
                call_id: format!("ask-{index}"),
                questions: vec![VerifiedQuestionAnswer {
                    question: "Continue?".to_owned(),
                    answer: "Yes".to_owned(),
                }],
            },
            acceptance_order: None,
        });
    }
    assert!(!context.verified_answers_complete());
    assert_eq!(context.verified_answers().count(), MAX_FAMILY_RECORDS);
    context.rollback(
        &["turn-2"],
        /*first_removed_message_id*/ None,
        RetainedInputSource::Local(None),
    );
    assert_eq!(context.verified_answers().count(), 0);
    assert!(!context.verified_answers_complete());

    let mut restored = snapshot.clone();
    let mut oversized = first.clone();
    let RetainedContextEvent::VerifiedAnswer { answer, .. } = &mut oversized else {
        panic!("verified answer fixture");
    };
    answer.questions[0].answer = "a".repeat(MAX_RECORD_BYTES);
    restored.record(&oversized);
    assert!(!restored.verified_answers_complete());
    assert!(
        restored
            .verified_answers()
            .next()
            .expect("retained answer fixture")
            .questions
            .is_empty()
    );
    assert_eq!(
        snapshot
            .verified_answers()
            .next()
            .expect("retained answer fixture")
            .questions[0]
            .answer,
        "Yes, but never publicly."
    );
    for index in 0..=MAX_FAMILY_RECORDS {
        restored.record_user_message(
            RetainedUserMessage {
                phase: None,
                origin: crate::UserInputOrigin::User,
                turn_id: "later-turn".to_owned(),
                message_id: Some(format!("message-{index}")),
                text: "Keep the repository private.".to_owned(),
                complete: true,
            },
            RetainedInputSource::Local(None),
        );
    }
    assert!(!restored.user_messages_complete());
    assert_eq!(
        restored
            .ordered_entries()
            .filter(|(_, entry)| matches!(entry, RetainedContextEntry::UserMessage(_)))
            .count(),
        MAX_FAMILY_RECORDS
    );
    let recent = restored.clone();
    restored.record_user_message(
        RetainedUserMessage {
            phase: None,
            origin: crate::UserInputOrigin::User,
            turn_id: "earlier-turn".to_owned(),
            message_id: Some("delayed-message".to_owned()),
            text: "An older queued instruction.".to_owned(),
            complete: true,
        },
        RetainedInputSource::Local(Some(0)),
    );
    assert_eq!(
        restored, recent,
        "delayed old input must not evict newer evidence"
    );
    restored.record_user_message(
        RetainedUserMessage {
            phase: None,
            origin: crate::UserInputOrigin::User,
            turn_id: "oversized-turn".to_owned(),
            message_id: Some("oversized-message".to_owned()),
            text: "restriction ".repeat(MAX_RECORD_BYTES),
            complete: true,
        },
        RetainedInputSource::Local(None),
    );
    let Some((_, RetainedContextEntry::UserMessage(message))) =
        restored.ordered_entries().next_back()
    else {
        panic!("latest user evidence");
    };
    assert_eq!((&message.text, message.complete), (&String::new(), false));
    let restrictions = restored.user_messages.clone();
    let authorization = restored.verified_answers.clone();
    for index in 0..MAX_FAMILY_RECORDS {
        let order = restored.reserve_order();
        restored.record_assistant_message(
            RetainedUserMessage {
                message_id: Some(format!("assistant-{index}")),
                text: "Run smoke tests?".to_owned(),
                complete: true,
                ..restrictions.back().unwrap().value.clone()
            },
            RetainedInputSource::Local(Some(order)),
        );
    }
    assert!(!restored.has_omitted_assistant_messages());
    let evicted = delivered_message("evicted", "Already replaced?", /*acceptance_order*/ 0);
    assert!(restored.record(&evicted));
    // The old-order delivery is immediately evicted, so the bounded buffer
    // cannot prove a subsequent recording with this ID is a duplicate.
    assert!(restored.record(&evicted));
    let order = restored.reserve_order();
    let oversized = delivered_message("oversized", &"x".repeat(MAX_RECORD_BYTES), order);
    assert!(restored.record(&oversized));
    assert!(!restored.record(&oversized));
    assert_eq!(restored.user_messages, restrictions);
    assert_eq!(restored.verified_answers, authorization);
    assert_eq!(restored.assistant_messages.len(), MAX_FAMILY_RECORDS);
    assert!(restored.has_omitted_assistant_messages());
    assert_eq!(
        restored.assistant_messages.back().unwrap().value,
        RetainedUserMessage {
            origin: crate::UserInputOrigin::User,
            turn_id: "turn-1".to_owned(),
            message_id: Some("oversized".to_owned()),
            text: String::new(),
            complete: false,
            phase: None,
        }
    );
    // Once the omission marker is set, another confirmed delivery still needs
    // its own rollout record even when the bounded window stays incomplete.
    let later = delivered_message("later", "A later question?", order + 1);
    assert!(restored.record(&later));
    assert!(!restored.record(&later));
}

#[test]
fn recovered_excerpts_obey_record_and_family_limits() {
    let mut context = RetainedContext::default();
    for index in 0..MAX_FAMILY_RECORDS {
        context.record_user_message(
            RetainedUserMessage {
                phase: None,
                origin: crate::UserInputOrigin::User,
                turn_id: "turn-1".to_owned(),
                message_id: Some(format!("message-{index}")),
                text: String::new(),
                complete: false,
            },
            RetainedInputSource::Local(None),
        );
    }
    let unchanged = context.clone();
    context.recover_user_message_excerpts(|_| Some("x".repeat(MAX_RECORD_BYTES)));
    assert_eq!(context, unchanged);
    context.recover_user_message_excerpts(|_| Some("x".repeat(MAX_RECORD_BYTES - 1_024)));
    assert!(context.user_messages_incomplete);
    assert!(context.user_messages.len() < MAX_FAMILY_RECORDS);
    assert!(serde_json::to_vec(&context.user_messages).unwrap().len() <= MAX_FAMILY_BYTES);
}

#[test]
fn legacy_checkpoints_mark_user_messages_incomplete() {
    let RetainedContextEvent::VerifiedAnswer { answer, .. } = publish_answer() else {
        panic!("verified answer fixture");
    };
    let mut wire = serde_json::json!({
        "verified_answers": [answer], "incomplete": false
    });
    let legacy: RetainedContext =
        serde_json::from_value(wire.clone()).expect("legacy retained-answer checkpoint");
    assert!(legacy.verified_answers_complete());
    assert!(!legacy.user_messages_complete());

    // Retain the original wire key even though the internal field names its family.
    wire["verified_answers"][0]["order"] = serde_json::json!(0);
    wire["user_messages"] = serde_json::json!([]);
    wire["user_messages_incomplete"] = serde_json::json!(true);
    wire["next_order"] = serde_json::json!(0);
    wire["assistant_messages"] = serde_json::json!([]);
    wire["assistant_messages_incomplete"] = serde_json::json!(false);
    assert_eq!(serde_json::to_value(&legacy).unwrap(), wire);

    let mut restored = RetainedContext::default();
    restored.restore(/*checkpoint*/ None, &[]);
    assert!(!restored.user_messages_complete());
}

#[test]
fn restored_input_order_accounts_for_surviving_local_sources() {
    let surviving_items = [(Some(7), false), (Some(100), true), (None, false)].map(
        |(user_input_order, inherited_user_message)| ResponseItemEnvelope {
            item: ResponseItem::Message {
                status: None,
                encrypted_content: None,
                id: None,
                role: "user".to_owned(),
                content: vec![ContentItem::InputText {
                    text: "Inspect the deployment.".to_owned(),
                }],
                phase: None,
                internal_chat_message_metadata_passthrough: None,
            },
            metadata: Some(CodexHarnessMetadata {
                user_input_order,
                inherited_user_message,
                ..Default::default()
            }),
        },
    );
    let checkpoint = RetainedContext {
        next_order: 20,
        ..Default::default()
    };
    for (checkpoint, expected_order) in [(None, 8), (Some(&checkpoint), 20)] {
        let mut restored = RetainedContext::default();
        restored.restore(checkpoint, &surviving_items);
        assert_eq!(
            (
                restored.reserve_order(),
                restored.ordered_entries().count(),
                restored.user_messages_complete(),
            ),
            (expected_order, 0, checkpoint.is_some()),
        );
    }
}

#[test]
fn accepted_order_survives_delayed_recording_and_checkpoint_replay() {
    let mut context = RetainedContext::default();
    // Rejected/canceled input consumes a sequence but never becomes evidence.
    context.reserve_order();
    let steer_order = context.reserve_order();
    let answer_order = context.reserve_order();
    let RetainedContextEvent::VerifiedAnswer { answer, .. } = publish_answer() else {
        panic!("verified answer fixture");
    };
    let event = RetainedContextEvent::VerifiedAnswer {
        answer,
        acceptance_order: Some(answer_order),
    };
    context.record(&event);
    let instruction = RetainedUserMessage {
        phase: None,
        origin: crate::UserInputOrigin::User,
        turn_id: "turn-1".to_owned(),
        message_id: Some("steer".to_owned()),
        text: "Keep the repository private.".to_owned(),
        complete: true,
    };
    // Assistant delivery after accepted steering must not move the steering past it.
    let assistant_order = context.reserve_order();
    let delivered = delivered_message("assistant", "Publish publicly?", assistant_order);
    assert!(context.record(&delivered));
    let checkpoint = serde_json::from_value(serde_json::to_value(&context).unwrap()).unwrap();
    context.record_user_message(
        instruction.clone(),
        RetainedInputSource::Local(Some(steer_order)),
    );
    let mut resumed = RetainedContext::default();
    resumed.restore(Some(&checkpoint), &[]);
    assert!(!resumed.record(&delivered));
    resumed.record_user_message(instruction, RetainedInputSource::Local(Some(steer_order)));
    // This suffix has no captured version metadata, so replay creates a fresh revision.
    // Its original contents and acceptance order still reconstruct identically.
    let mut expected = context.clone();
    assert_ne!(
        resumed.user_messages[0].revision,
        expected.user_messages[0].revision
    );
    expected.user_messages[0].revision = resumed.user_messages[0].revision.clone();
    assert_eq!(resumed, expected);
    let source = context
        .source(RetainedContextEntry::UserMessage(
            &context.user_messages[0].value,
        ))
        .unwrap();
    assert!(resumed.restore_source_revision(&source));
    assert_eq!(resumed, context);
    assert_eq!(
        resumed
            .ordered_entries()
            .map(|(order, entry)| match entry {
                RetainedContextEntry::UserMessage(message)
                | RetainedContextEntry::AssistantMessage(message) => (order, message.text.as_str()),
                RetainedContextEntry::VerifiedAnswer(answer) => {
                    (order, answer.questions[0].answer.as_str())
                }
            })
            .collect::<Vec<_>>(),
        vec![
            (
                RetainedContextOrder::Local(steer_order),
                "Keep the repository private."
            ),
            (
                RetainedContextOrder::Local(answer_order),
                "Yes, but never publicly."
            ),
            (
                RetainedContextOrder::Local(assistant_order),
                "Publish publicly?"
            ),
        ],
    );
    // This checkpoint predates model delivery of the queued instruction. Rollback
    // still removes its later-accepted answer using the persisted boundary order.
    let mut before_delivery = checkpoint;
    before_delivery.rollback(
        &["turn-1"],
        Some(&user_boundary("steer", "turn-1")),
        RetainedInputSource::Local(Some(steer_order)),
    );
    resumed.rollback(
        &["turn-1"],
        Some(&user_boundary("steer", "turn-1")),
        RetainedInputSource::Local(Some(steer_order)),
    );
    assert_eq!(before_delivery, resumed);
    assert_eq!(
        resumed,
        RetainedContext {
            next_order: 4,
            ..Default::default()
        }
    );
}

#[test]
fn adopted_instructions_preserve_local_order_and_rollback_scope() {
    let mut context = RetainedContext::default();
    context.record(&publish_answer());
    for index in 0..2 {
        let message = RetainedUserMessage {
            phase: None,
            origin: crate::UserInputOrigin::User,
            turn_id: "parent-turn".to_owned(),
            message_id: Some(format!("parent-{index}")),
            text: format!("Parent instruction {index}"),
            complete: true,
        };
        context.record_user_message(message.clone(), RetainedInputSource::Inherited);
        context.record_user_message(message.clone(), RetainedInputSource::Inherited);
        context.record_assistant_message(message, RetainedInputSource::Inherited);
    }
    assert_eq!(context.reserve_order(), 1);
    assert!(!context.verified_answers_complete());
    assert_eq!(
        context
            .ordered_entries()
            .map(|(order, _)| order)
            .collect::<Vec<_>>(),
        vec![
            RetainedContextOrder::Inherited(0),
            RetainedContextOrder::Inherited(1),
            RetainedContextOrder::Inherited(2),
            RetainedContextOrder::Inherited(3),
            RetainedContextOrder::Local(0)
        ],
    );
    let checkpoint = serde_json::from_slice(&serde_json::to_vec(&context).unwrap()).unwrap();
    let mut restored = RetainedContext::default();
    restored.restore(Some(&checkpoint), &[]);
    assert_eq!(restored, context);
    // Older roots adopted complete instructions without recording the missing parent answers.
    let mut legacy_checkpoint = serde_json::to_value(&checkpoint).unwrap();
    legacy_checkpoint["incomplete"] = serde_json::json!(false);
    let legacy_checkpoint = serde_json::from_value(legacy_checkpoint).unwrap();
    restored.restore(Some(&legacy_checkpoint), &[]);
    assert_eq!(restored, context);
    restored.rollback(
        &["turn-1"],
        /*first_removed_message_id*/ None,
        RetainedInputSource::Local(Some(0)),
    );
    let mut expected = context;
    expected.verified_answers.clear();
    assert_eq!(restored, expected);
    restored.rollback(
        &["parent-turn"],
        Some(&user_boundary("parent-1", "parent-turn")),
        RetainedInputSource::Inherited,
    );
    expected.user_messages.pop_back();
    expected.assistant_messages.pop_back();
    assert_eq!(restored, expected);
}
