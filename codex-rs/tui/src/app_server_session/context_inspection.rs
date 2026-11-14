// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
//! Bounded, body-free inspection of the current thread's metadata pages.
use super::*;
use crate::app_event::AppEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ContextInspectionKey {
    backend_id: crate::backend_id::BackendId,
    connection_id: uuid::Uuid,
    request_generation: uuid::Uuid,
    pub(crate) thread_id: ThreadId,
    view_generation: crate::chatwidget::ConnectorScopeGeneration,
}

impl ContextInspectionKey {
    pub(crate) fn matches_view(
        &self,
        thread: Option<ThreadId>,
        view: crate::chatwidget::ConnectorScopeGeneration,
    ) -> bool {
        thread == Some(self.thread_id) && view == self.view_generation
    }
}

#[derive(Debug)]
pub(crate) enum ContextInspectionFailure {
    Unsupported,
    Stale,
    PageTooLarge,
    SingleFragmentTooLarge,
    Failed,
}

#[derive(Debug)]
pub(crate) enum ContextInspectionOutcome {
    Completed(
        std::result::Result<
            codex_app_server_protocol::ThreadContextReadResponse,
            ContextInspectionFailure,
        >,
    ),
    TimedOut,
    Drained,
}

pub(super) struct PendingContextInspection {
    key: ContextInspectionKey,
    task: tokio::task::JoinHandle<()>,
    cancelled: bool,
    page_limit: usize,
}

pub(super) struct ContextInspectionCursor {
    thread_id: ThreadId,
    view_generation: crate::chatwidget::ConnectorScopeGeneration,
    snapshot_id: String,
    cursor: usize,
    page_limit: usize,
}

impl Drop for PendingContextInspection {
    fn drop(&mut self) {
        self.task.abort();
    }
}

const CONTEXT_INSPECTION_UI_DEADLINE: std::time::Duration = std::time::Duration::from_secs(5);

async fn run_context_inspection_request(
    request: impl std::future::Future<
        Output = std::result::Result<
            codex_app_server_protocol::ThreadContextReadResponse,
            ContextInspectionFailure,
        >,
    >,
    key: ContextInspectionKey,
    events: AppEventSender,
    deadline: std::time::Duration,
) {
    tokio::pin!(request);
    match tokio::time::timeout(deadline, &mut request).await {
        Ok(result) => events.send(AppEvent::ContextCompositionLoaded {
            key,
            outcome: ContextInspectionOutcome::Completed(result),
        }),
        Err(_) => {
            events.send(AppEvent::ContextCompositionLoaded {
                key,
                outcome: ContextInspectionOutcome::TimedOut,
            });
            // Dropping the waiter cannot guarantee cancellation of a sent wire request.
            // Keep the single slot until it finishes or the owning connection closes.
            let _ = request.await;
            events.send(AppEvent::ContextCompositionLoaded {
                key,
                outcome: ContextInspectionOutcome::Drained,
            });
        }
    }
}

impl AppServerSession {
    pub(crate) fn has_context_inspection_next(
        &self,
        thread_id: ThreadId,
        view: crate::chatwidget::ConnectorScopeGeneration,
    ) -> bool {
        self.context_inspection_cursor
            .as_ref()
            .is_some_and(|cursor| cursor.thread_id == thread_id && cursor.view_generation == view)
    }

    pub(crate) fn start_context_inspection(
        &mut self,
        thread_id: ThreadId,
        view: crate::chatwidget::ConnectorScopeGeneration,
        events: AppEventSender,
    ) -> bool {
        self.start_context_page(thread_id, view, events, false, 25)
    }

    pub(crate) fn start_context_page(
        &mut self,
        thread_id: ThreadId,
        view_generation: crate::chatwidget::ConnectorScopeGeneration,
        events: AppEventSender,
        next: bool,
        initial_limit: usize,
    ) -> bool {
        if !next && !matches!(initial_limit, 1 | 25) {
            return false;
        }
        if self
            .pending_context_inspection
            .as_ref()
            .is_some_and(|pending| !pending.task.is_finished())
        {
            return false;
        }
        let (snapshot_id, cursor, page_limit) = if next {
            let Some(cursor) = self.context_inspection_cursor.as_ref().filter(|cursor| {
                cursor.thread_id == thread_id && cursor.view_generation == view_generation
            }) else {
                return false;
            };
            (
                Some(cursor.snapshot_id.clone()),
                cursor.cursor,
                cursor.page_limit,
            )
        } else {
            self.context_inspection_cursor = None;
            (None, 0, initial_limit)
        };
        let key = ContextInspectionKey {
            backend_id: self.backend_id,
            connection_id: self.connection_id,
            request_generation: uuid::Uuid::new_v4(),
            thread_id,
            view_generation,
        };
        let request_id = self.next_request_id();
        let handle = self.request_handle();
        let task = tokio::spawn(async move {
            let request = async move {
                handle
                    .request_typed(ClientRequest::ThreadContextRead {
                        request_id,
                        params: codex_app_server_protocol::ThreadContextReadParams {
                            thread_id: thread_id.to_string(),
                            snapshot_id: snapshot_id.clone(),
                            cursor,
                            limit: Some(page_limit),
                        },
                    })
                    .await
                    .map_err(|error| match error {
                        TypedRequestError::Server { source, .. }
                            if source.code == JSONRPC_METHOD_NOT_FOUND =>
                        {
                            ContextInspectionFailure::Unsupported
                        }
                        TypedRequestError::Server { source, .. }
                            if source.message.contains("context page exceeds 65536 bytes") =>
                        {
                            if page_limit == 1 {
                                ContextInspectionFailure::SingleFragmentTooLarge
                            } else {
                                ContextInspectionFailure::PageTooLarge
                            }
                        }
                        TypedRequestError::Server { source, .. }
                            if source
                                .data
                                .as_ref()
                                .and_then(|data| data.get("reason"))
                                .and_then(serde_json::Value::as_str)
                                == Some("staleContextSnapshot") =>
                        {
                            ContextInspectionFailure::Stale
                        }
                        _ => ContextInspectionFailure::Failed,
                    })
                    .and_then(
                        |response: codex_app_server_protocol::ThreadContextReadResponse| {
                            if response.thread_id != thread_id.to_string()
                                || snapshot_id
                                    .as_ref()
                                    .is_some_and(|id| id != &response.snapshot_id)
                            {
                                Err(ContextInspectionFailure::Stale)
                            } else {
                                Ok(response)
                            }
                        },
                    )
            };
            run_context_inspection_request(request, key, events, CONTEXT_INSPECTION_UI_DEADLINE)
                .await;
        });
        self.pending_context_inspection = Some(PendingContextInspection {
            key,
            task,
            cancelled: false,
            page_limit,
        });
        true
    }

    /// Cancel presentation, not the already-sent request; retain the slot until drain.
    pub(crate) fn cancel_context_inspection(
        &mut self,
        thread: ThreadId,
        view: crate::chatwidget::ConnectorScopeGeneration,
    ) -> bool {
        let Some(pending) = self.pending_context_inspection.as_mut() else {
            return false;
        };
        if !pending.key.matches_view(Some(thread), view) || pending.cancelled {
            return false;
        }
        pending.cancelled = true;
        true
    }

    pub(crate) fn accept_context_inspection(
        &mut self,
        key: ContextInspectionKey,
        outcome: &ContextInspectionOutcome,
    ) -> bool {
        if key.backend_id != self.backend_id || key.connection_id != self.connection_id {
            return false;
        }
        let Some(pending) = self
            .pending_context_inspection
            .as_ref()
            .filter(|pending| pending.key == key)
        else {
            return false;
        };
        let visible = !pending.cancelled;
        let page_limit = pending.page_limit;
        if visible && let ContextInspectionOutcome::Completed(Ok(response)) = outcome {
            self.context_inspection_cursor =
                response.next_cursor.map(|cursor| ContextInspectionCursor {
                    thread_id: key.thread_id,
                    view_generation: key.view_generation,
                    snapshot_id: response.snapshot_id.clone(),
                    cursor,
                    page_limit,
                });
        }
        if visible
            && matches!(
                outcome,
                ContextInspectionOutcome::Completed(Err(ContextInspectionFailure::Stale))
            )
        {
            self.context_inspection_cursor = None;
        }
        if !matches!(outcome, ContextInspectionOutcome::TimedOut) {
            self.pending_context_inspection = None;
        }
        visible
    }
}

#[cfg(test)]
#[path = "context_inspection_tests.rs"]
mod tests;
