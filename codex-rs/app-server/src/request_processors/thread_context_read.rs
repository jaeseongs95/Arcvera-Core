// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
//! Read-only, body-free composition for an already loaded thread.
use super::*;
use crate::error_code::passive_runtime_unavailable;
use codex_app_server_protocol::{ThreadContextReadParams, ThreadContextReadResponse};

fn validate_page_size(response: &impl serde::Serialize) -> Result<(), JSONRPCErrorError> {
    let bytes = serde_json::to_vec(response).map_err(|error| internal_error(error.to_string()))?;
    if bytes.len() > 65_536 {
        return Err(invalid_request(
            "context page exceeds 65536 bytes; request a smaller limit",
        ));
    }
    Ok(())
}

impl ThreadRequestProcessor {
    pub(crate) async fn thread_context_read(
        &self,
        params: ThreadContextReadParams,
    ) -> Result<Option<ClientResponsePayload>, JSONRPCErrorError> {
        let id = ThreadId::from_string(&params.thread_id)
            .map_err(|error| invalid_params(format!("invalid thread id: {error}")))?;
        let limit = params.limit.unwrap_or(25);
        if !(1..=100).contains(&limit) {
            return Err(invalid_params("limit must be in 1..100"));
        }
        if params.cursor != 0 && params.snapshot_id.is_none() {
            return Err(invalid_params("snapshotId is required for nonzero cursor"));
        }
        let thread = self
            .thread_manager
            .get_thread(id)
            .await
            .map_err(|_| invalid_request("thread/context/read requires a loaded thread"))?;
        let _activity = self
            .thread_manager
            .admit_thread_activity(id)
            .map_err(|_| invalid_request("thread is closing"))?;
        drop(
            self.thread_manager
                .lock_loaded_thread(id, &thread)
                .await
                .map_err(passive_runtime_unavailable)?,
        );
        let page = thread
            .context_composition_read(params.snapshot_id.as_deref(), params.cursor, limit)
            .await
            .map_err(|error| {
                if error == "stale context snapshot" {
                    let mut rpc = invalid_request("context snapshot is stale; restart pagination");
                    rpc.data = Some(serde_json::json!({"reason":"staleContextSnapshot"}));
                    rpc
                } else {
                    invalid_request(error)
                }
            })?;
        let mut value =
            serde_json::to_value(page).map_err(|error| internal_error(error.to_string()))?;
        value
            .as_object_mut()
            .ok_or_else(|| internal_error("invalid context page"))?
            .insert("threadId".into(), id.to_string().into());
        let response: ThreadContextReadResponse = serde_json::from_value(value)
            .map_err(|error| internal_error(format!("context page contract: {error}")))?;
        validate_page_size(&response)?;
        // Hold the registry guard only for the short final identity check and return.
        let _identity = self
            .thread_manager
            .lock_loaded_thread(id, &thread)
            .await
            .map_err(passive_runtime_unavailable)?;
        Ok(Some(response.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialized_page_limit_is_inclusive_and_counts_utf8_bytes() {
        let exact = "x".repeat(65_534);
        assert!(validate_page_size(&exact).is_ok());
        assert!(validate_page_size(&format!("{exact}x")).is_err());
        assert!(validate_page_size(&format!("{exact}한")).is_err());
    }
}
