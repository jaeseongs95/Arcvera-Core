// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
// Modified for OpenDIY Codex R32 (2026-11-01); see MODIFICATIONS.md.
use std::sync::Arc;

use codex_core::config::Config;
use codex_core::context::ContextualUserFragment;
use codex_core::context::MemoryContextFragment;
use codex_extension_api::ConfigContributor;
use codex_extension_api::ContentItemKind;
use codex_extension_api::ContextContributor;
use codex_extension_api::ExtensionData;
use codex_extension_api::ExtensionFuture;
use codex_extension_api::ExtensionRegistryBuilder;
use codex_extension_api::PromptFragment;
use codex_extension_api::ThreadLifecycleContributor;
use codex_extension_api::ThreadStartInput;
use codex_extension_api::ToolContributor;
use codex_features::Feature;
use codex_otel::MetricsClient;
use codex_protocol::MemoryVersion;
use codex_utils_absolute_path::AbsolutePathBuf;

use crate::local::LocalMemoriesBackend;
use crate::prompts::build_memory_instructions_from_snapshot;
use crate::tools;

/// Contributes Codex memory read-path prompt context and memory read tools.
#[derive(Clone, Default)]
pub(crate) struct MemoriesExtension {
    metrics_client: Option<MetricsClient>,
}

impl MemoriesExtension {
    fn new(metrics_client: Option<MetricsClient>) -> Self {
        Self { metrics_client }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct MemoriesExtensionConfig {
    pub(crate) enabled: bool,
    pub(crate) dedicated_tools: bool,
    pub(crate) root: Option<AbsolutePathBuf>,
    pub(crate) version: MemoryVersion,
}

impl MemoriesExtensionConfig {
    fn from_config(config: &Config) -> Self {
        Self {
            enabled: config.features.enabled(Feature::MemoryTool) && config.memories.use_memories,
            dedicated_tools: config.memories.dedicated_tools,
            root: config.memories.memory_root(&config.codex_home),
            version: config.memories.version,
        }
    }
}

impl ContextContributor for MemoriesExtension {
    fn contribute_thread_context<'a>(
        &'a self,
        _session_store: &'a ExtensionData,
        thread_store: &'a ExtensionData,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Vec<PromptFragment>> + Send + 'a>> {
        Box::pin(async move {
            let Some(config) = thread_store.get::<MemoriesExtensionConfig>() else {
                return Vec::new();
            };
            if !config.enabled {
                return Vec::new();
            }

            let Some(_root) = config.root.as_ref() else {
                return Vec::new();
            };
            let warning = || vec![PromptFragment::developer_policy(
                "Memory snapshot is unavailable. Do not infer that memories are absent or that corrections were applied.".to_string(),
                ContentItemKind(MemoryContextFragment::READ_INSTRUCTIONS_KIND.to_string()),
            )];
            let Some(capability) = thread_store.get::<codex_memories_read::publication_snapshot::MemorySnapshotCapability>() else {
                return warning();
            };
            let snapshot = match capability.0.capture().await {
                Ok(snapshot) => snapshot,
                Err(_) => return warning(),
            };
            if snapshot.legacy_empty {
                return vec![PromptFragment::developer_policy(
                    "The captured legacy baseline has no generated summary. Original completeness and note application are unverified.".to_string(),
                    ContentItemKind(MemoryContextFragment::READ_INSTRUCTIONS_KIND.to_string()),
                )];
            }
            let Ok(derived) = AbsolutePathBuf::from_absolute_path(snapshot.derived_root) else { return warning(); };
            let Ok(source) = AbsolutePathBuf::from_absolute_path(snapshot.original_root) else { return warning(); };
            let instructions = match build_memory_instructions_from_snapshot(
                &derived, &source, config.version, snapshot.current_for_notes, snapshot.provenance.as_deref(), snapshot.artifact_manifest
            ).await {
                Ok(Some(instructions)) => instructions,
                Ok(None) | Err(_) => return warning(),
            };
            let instructions = match config.version {
                MemoryVersion::V1 => vec![instructions],
                MemoryVersion::V2 => {
                    // Keep the complete summary while respecting each fragment's byte cap.
                    let mut remaining = instructions.as_str();
                    let mut fragments = Vec::new();
                    while !remaining.is_empty() {
                        let end = remaining.floor_char_boundary(remaining.len().min(8_900));
                        fragments.push(
                            MemoryContextFragment::ReadInstructions(remaining[..end].to_string())
                                .render()
                                .into_text()
                                .unwrap_or_default(),
                        );
                        remaining = &remaining[end..];
                    }
                    fragments
                }
            };
            instructions
                .into_iter()
                .map(|instructions| {
                    PromptFragment::developer_policy(
                        instructions,
                        ContentItemKind(MemoryContextFragment::READ_INSTRUCTIONS_KIND.to_string()),
                    )
                })
                .collect()
        })
    }
}

impl ThreadLifecycleContributor<Config> for MemoriesExtension {
    fn on_thread_start<'a>(
        &'a self,
        input: ThreadStartInput<'a, Config>,
    ) -> ExtensionFuture<'a, ()> {
        Box::pin(async move {
            let mut config = MemoriesExtensionConfig::from_config(input.config);
            if !input
                .config
                .memory_environment_supported(input.environments)
            {
                config.root = None;
            }
            input.thread_store.insert(config);
        })
    }
}

impl ConfigContributor<Config> for MemoriesExtension {
    fn on_config_changed(
        &self,
        _session_store: &ExtensionData,
        thread_store: &ExtensionData,
        _previous_config: &Config,
        new_config: &Config,
    ) {
        let mut config = MemoriesExtensionConfig::from_config(new_config);
        if let Some(previous) = thread_store.get::<MemoriesExtensionConfig>() {
            // The initial summary and retrieval tools must use the same namespace.
            config.version = previous.version;
            config.root = previous.root.clone();
        }
        thread_store.insert(config);
    }
}

impl ToolContributor for MemoriesExtension {
    fn tools(
        &self,
        _session_store: &ExtensionData,
        thread_store: &ExtensionData,
    ) -> Vec<
        Arc<dyn for<'call> codex_extension_api::ToolExecutor<codex_extension_api::ToolCall<'call>>>,
    > {
        let Some(config) = thread_store.get::<MemoriesExtensionConfig>() else {
            return Vec::new();
        };
        if !config.enabled {
            return Vec::new();
        }

        let Some(root) = config.root.as_ref() else {
            return Vec::new();
        };
        let snapshot = thread_store.get::<codex_memories_read::publication_snapshot::MemorySnapshotCapability>()
            .map(|capability| capability.as_ref().clone());
        let backend = LocalMemoriesBackend::from_snapshot(root.to_path_buf(), snapshot);
        if config.dedicated_tools {
            tools::memory_tools(backend, self.metrics_client.clone())
        } else {
            tools::memory_note_tool(backend, self.metrics_client.clone())
        }
    }
}

/// Installs the memories extension contributors into the extension registry.
pub fn install(
    registry: &mut ExtensionRegistryBuilder<Config>,
    metrics_client: Option<MetricsClient>,
) {
    let extension = Arc::new(MemoriesExtension::new(metrics_client));
    registry.thread_lifecycle_contributor(extension.clone());
    registry.config_contributor(extension.clone());
    registry.prompt_contributor(extension.clone());
    registry.tool_contributor(extension);
}
