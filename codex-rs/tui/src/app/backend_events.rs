// Modified for OpenDIY Codex R38 (2026-11-06); see MODIFICATIONS.md.
//! Checks incoming event ownership before consulting native-thread caches or UI state.
//! Connection-wide lifecycle and account events may only update the displayed conversation's UI.

use super::App;
use super::app_server_event_targets::ServerNotificationThreadTarget;
use super::app_server_event_targets::server_notification_thread_target;
use crate::app_server_session::AppServerSession;
use crate::app_server_session::AppServerSessionEvent;
use crate::backend_id::BackendId;
use codex_app_server_client::AppServerEvent;

impl App {
    pub(super) fn handle_backend_stream_closed(&mut self, app_server: &AppServerSession) {
        app_server
            .cron_tool_connection()
            .live
            .store(false, std::sync::atomic::Ordering::Release);
        self.cron.invalidate_backend(
            app_server.backend_id(),
            crate::i18n::text("Backend event stream ended: cron execution stopped"),
        );
    }

    pub(super) async fn handle_polled_backend_event(
        &mut self,
        app_server: &AppServerSession,
        displayed_owner: BackendId,
        event: AppServerSessionEvent,
    ) {
        if event.backend_id != app_server.backend_id() {
            tracing::warn!("ignoring server event from a different backend");
            return;
        }
        if matches!(
            &event.event,
            AppServerEvent::Disconnected { .. } | AppServerEvent::Lagged { .. }
        ) {
            app_server
                .cron_tool_connection()
                .live
                .store(false, std::sync::atomic::Ordering::Release);
            self.cron.invalidate_backend(
                event.backend_id,
                crate::i18n::text("Backend connection or events lost: cron execution stopped"),
            );
        }
        if displayed_owner != event.backend_id {
            // A parked voice session still needs its own notifications.
            if event.backend_id == app_server.backend_id()
                && let AppServerEvent::ServerNotification(notification) = &event.event
                && let ServerNotificationThreadTarget::Thread(thread_id) =
                    server_notification_thread_target(notification)
                && self.voice_owner_thread_id() == Some(thread_id)
            {
                self.deliver_background_voice_notification(thread_id, notification);
            }
            return;
        }
        self.handle_backend_event(app_server, event).await;
    }

    pub(super) async fn handle_backend_event(
        &mut self,
        app_server: &AppServerSession,
        event: AppServerSessionEvent,
    ) {
        if event.backend_id != app_server.backend_id() {
            tracing::warn!("ignoring server event from a different backend");
            return;
        }
        self.handle_app_server_event(app_server, event.event).await;
    }
}
