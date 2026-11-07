// Modified for OpenDIY Codex R39 (2026-11-07); see MODIFICATIONS.md.
use super::*;
use codex_app_server_protocol::ThreadContextReadParams;
use codex_app_server_protocol::ThreadContextReadResponse;

impl ThreadRequestProcessor {
    pub(crate) async fn thread_context_read(
        &self,
        params: ThreadContextReadParams,
    ) -> Result<Option<ClientResponsePayload>, JSONRPCErrorError> {
        // This helper only resolves loaded ThreadManager handles; no persisted auto-resume.
        let (thread_id, thread) = self.load_thread(&params.thread_id).await?;
        Ok(Some(
            ThreadContextReadResponse {
                thread_id: thread_id.to_string(),
                snapshot: thread.context_snapshot().await,
            }
            .into(),
        ))
    }
}
