// Modified for OpenDIY Codex R35 (2026-11-03); see MODIFICATIONS.md.
//! Goal summary for the bare `/goal` command.

use super::*;
use crate::goal_display::format_goal_elapsed_seconds;
use crate::goal_files;
use crate::status::format_tokens_compact;

impl ChatWidget {
    pub(crate) fn show_goal_summary(&mut self, goal: AppThreadGoal) {
        self.add_plain_history_lines(goal_summary_lines(&goal));
    }

    pub(crate) fn show_goal_edit_prompt(&mut self, thread_id: ThreadId, goal: AppThreadGoal) {
        let tx = self.app_event_tx.clone();
        let status = edited_goal_status(goal.status);
        let token_budget = goal.token_budget;
        let view = CustomPromptView::new(
            crate::i18n::static_message("controls-goal-edit").to_string(),
            crate::i18n::static_message("controls-goal-edit-help").to_string(),
            goal.objective,
            /*context_label*/ None,
            Box::new(move |objective: String| {
                tx.send(AppEvent::SetThreadGoalDraft {
                    thread_id,
                    draft: goal_files::GoalDraft {
                        objective,
                        ..Default::default()
                    },
                    mode: crate::app_event::ThreadGoalSetMode::UpdateExisting {
                        status,
                        token_budget,
                    },
                });
            }),
        );
        self.bottom_pane.show_text_prompt(view);
    }

    pub(crate) fn show_resume_paused_goal_prompt(
        &mut self,
        thread_id: ThreadId,
        objective: String,
    ) {
        let resume_actions: Vec<SelectionAction> = vec![Box::new(move |tx| {
            tx.send(AppEvent::SetThreadGoalStatus {
                thread_id,
                status: AppThreadGoalStatus::Active,
            });
        })];
        self.show_selection_view(SelectionViewParams {
            title: Some(crate::i18n::static_message("controls-goal-resume-title").to_string()),
            subtitle: Some(crate::i18n::message(
                "controls-goal-value",
                &[("value", objective.as_str())],
            )),
            footer_hint: Some(standard_popup_hint_line()),
            initial_selected_idx: Some(0),
            items: vec![
                SelectionItem {
                    name: crate::i18n::static_message("controls-goal-resume").to_string(),
                    description: Some(
                        crate::i18n::static_message("controls-goal-resume-help").to_string(),
                    ),
                    actions: resume_actions,
                    dismiss_on_select: true,
                    ..Default::default()
                },
                SelectionItem {
                    name: crate::i18n::static_message("controls-goal-leave-paused").to_string(),
                    description: Some(
                        crate::i18n::static_message("controls-goal-leave-paused-help").to_string(),
                    ),
                    dismiss_on_select: true,
                    ..Default::default()
                },
            ],
            ..SelectionViewParams::confirmation()
        });
    }

    pub(crate) fn on_thread_goal_cleared(&mut self, thread_id: &str) {
        if self
            .thread_id
            .is_some_and(|active_thread_id| active_thread_id.to_string() == thread_id)
        {
            self.current_goal_status = None;
            self.update_collaboration_mode_indicator();
        }
    }
}

fn goal_summary_lines(goal: &AppThreadGoal) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(crate::i18n::static_message("controls-goal").bold()),
        Line::from(vec![
            crate::i18n::static_message("controls-goal-status").dim(),
            goal_status_label(goal.status).to_string().into(),
        ]),
        Line::from(vec![
            crate::i18n::static_message("controls-goal-objective").dim(),
            goal.objective.clone().into(),
        ]),
        Line::from(vec![
            crate::i18n::static_message("controls-goal-time").dim(),
            format_goal_elapsed_seconds(goal.time_used_seconds).into(),
        ]),
        Line::from(vec![
            crate::i18n::static_message("controls-goal-tokens").dim(),
            format_tokens_compact(goal.tokens_used).into(),
        ]),
    ];
    if let Some(token_budget) = goal.token_budget {
        lines.push(Line::from(vec![
            crate::i18n::static_message("controls-goal-budget").dim(),
            format_tokens_compact(token_budget).into(),
        ]));
    }
    let command_hint = match goal.status {
        AppThreadGoalStatus::Active => crate::i18n::static_message("controls-goal-active-commands"),
        AppThreadGoalStatus::Paused
        | AppThreadGoalStatus::Blocked
        | AppThreadGoalStatus::UsageLimited => {
            crate::i18n::static_message("controls-goal-paused-commands")
        }
        AppThreadGoalStatus::BudgetLimited | AppThreadGoalStatus::Complete => {
            crate::i18n::static_message("controls-goal-finished-commands")
        }
    };
    lines.push(Line::default());
    lines.push(Line::from(command_hint.dim()));
    lines
}

fn goal_status_label(status: AppThreadGoalStatus) -> &'static str {
    match status {
        AppThreadGoalStatus::Active => crate::i18n::static_message("controls-goal-active"),
        AppThreadGoalStatus::Paused => crate::i18n::static_message("controls-goal-paused"),
        AppThreadGoalStatus::Blocked => crate::i18n::static_message("controls-goal-stalled"),
        AppThreadGoalStatus::UsageLimited => {
            crate::i18n::static_message("controls-goal-usage-limited")
        }
        AppThreadGoalStatus::BudgetLimited => {
            crate::i18n::static_message("controls-goal-budget-limited")
        }
        AppThreadGoalStatus::Complete => crate::i18n::static_message("controls-goal-complete"),
    }
}

fn edited_goal_status(status: AppThreadGoalStatus) -> AppThreadGoalStatus {
    match status {
        AppThreadGoalStatus::Active => AppThreadGoalStatus::Active,
        AppThreadGoalStatus::Paused
        | AppThreadGoalStatus::Blocked
        | AppThreadGoalStatus::UsageLimited => status,
        AppThreadGoalStatus::BudgetLimited | AppThreadGoalStatus::Complete => {
            AppThreadGoalStatus::Active
        }
    }
}

#[cfg(test)]
mod localization_tests {
    use super::*;

    #[test]
    fn goal_summary_localizes_labels_and_preserves_objective_and_commands() {
        let objective = "English objective /goal pause { $value } 한국어";
        let goal = AppThreadGoal {
            thread_id: ThreadId::new().to_string(),
            objective: objective.to_string(),
            status: AppThreadGoalStatus::Paused,
            token_budget: Some(1000),
            tokens_used: 123,
            time_used_seconds: 90,
            created_at: 0,
            updated_at: 0,
        };
        for (locale, title, status, label) in [
            (
                crate::i18n::Locale::English,
                "Goal",
                "Status: paused",
                "Objective: ",
            ),
            (
                crate::i18n::Locale::Korean,
                "목표",
                "상태: 일시 중지",
                "목표 내용: ",
            ),
        ] {
            crate::i18n::with_locale(locale, || {
                let lines = goal_summary_lines(&goal);
                let rendered: Vec<String> = lines.iter().map(ToString::to_string).collect();
                assert_eq!(rendered[0], title);
                assert_eq!(rendered[1], status);
                assert_eq!(rendered[2], format!("{label}{objective}"));
                assert!(
                    rendered
                        .last()
                        .unwrap()
                        .contains("/goal edit, /goal resume, /goal clear")
                );
                assert_eq!(lines[1].spans[0].style, Style::default().dim());
            });
        }
        assert_eq!(goal.objective, objective);
        assert_eq!(goal.status, AppThreadGoalStatus::Paused);
    }
}
