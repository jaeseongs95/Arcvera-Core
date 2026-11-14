// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
//! Connection-owned MCP configuration. Never serialized or inherited through config.

use crate::config::Config;
use codex_config::McpServerConfig;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use tokio_util::sync::CancellationToken;

pub struct RuntimeMcpServers {
    registration_id: String,
    pub(crate) owner: u64,
    pub(crate) disconnected: CancellationToken,
    servers: HashMap<String, McpServerConfig>,
    revocation_observed: AtomicBool,
}

impl RuntimeMcpServers {
    pub(crate) fn servers(&self) -> &HashMap<String, McpServerConfig> {
        &self.servers
    }
    pub(crate) fn matches(&self, owner: u64, servers: &HashMap<String, McpServerConfig>) -> bool {
        self.owner == owner && !self.disconnected.is_cancelled() && &self.servers == servers
    }
    pub fn new(
        owner: u64,
        disconnected: CancellationToken,
        servers: HashMap<String, McpServerConfig>,
    ) -> Self {
        Self {
            registration_id: uuid::Uuid::new_v4().to_string(),
            owner,
            disconnected,
            servers,
            revocation_observed: AtomicBool::new(false),
        }
    }

    /// Non-secret identity for correlating this registration with a published binding.
    pub fn registration_id(&self) -> &str {
        &self.registration_id
    }

    pub(crate) fn with_registration_id(mut self, registration_id: String) -> Self {
        self.registration_id = registration_id;
        self
    }

    /// Invalidate once for this exact registration; never remove a newer owner's slot.
    pub(crate) fn observe_disconnect(&self) -> bool {
        self.disconnected.is_cancelled() && !self.revocation_observed.swap(true, Ordering::AcqRel)
    }

    pub fn validate(&self, config: &Config) -> Result<(), String> {
        self.project(config).map(|_| ())
    }

    pub(crate) fn project(&self, config: &Config) -> Result<Config, String> {
        let mut projected = config.clone();
        let mut servers = config.mcp_servers.get().clone();
        for (name, server) in &self.servers {
            if servers.contains_key(name) {
                return Err(format!(
                    "runtime MCP server collides with configured server: {name}"
                ));
            }
            servers.insert(name.clone(), server.clone());
        }
        // Reuse the managed constraint, including its normalization. No allow_any shortcut.
        projected
            .mcp_servers
            .set(servers)
            .map_err(|_| "runtime MCP servers violate managed policy".to_string())?;
        for (name, server) in &self.servers {
            if projected.mcp_servers.get().get(name) != Some(server) || !server.enabled {
                return Err(format!(
                    "runtime MCP server is disabled or constrained by managed policy: {name}"
                ));
            }
        }
        Ok(projected)
    }
}
