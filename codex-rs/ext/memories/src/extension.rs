use std::sync::Arc;

use codex_config::types::MemoryScope;
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
use codex_extension_api::TurnInputContext;
use codex_extension_api::TurnInputContributor;
use codex_features::Feature;
use codex_otel::MetricsClient;
use codex_protocol::MemoryVersion;
use codex_utils_absolute_path::AbsolutePathBuf;

use crate::local::LocalMemoriesBackend;
use crate::prompts::build_memory_context_from_root;
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
    pub(crate) scope: MemoryScope,
}

impl MemoriesExtensionConfig {
    fn from_config(config: &Config) -> Self {
        Self {
            enabled: config.features.enabled(Feature::MemoryTool) && config.memories.use_memories,
            dedicated_tools: config.memories.dedicated_tools,
            root: config.memories.memory_root(&config.codex_home),
            version: config.memories.version,
            scope: config.memories.scope,
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

            let Some(root) = config.root.as_ref() else {
                return Vec::new();
            };
            let Some((instructions, policy_only)) =
                build_memory_context_from_root(root, config.version).await
            else {
                return Vec::new();
            };
            vec![PromptFragment::developer_policy(
                instructions,
                ContentItemKind(
                    if policy_only {
                        "memories.policy"
                    } else {
                        MemoryContextFragment::READ_INSTRUCTIONS_KIND
                    }
                    .to_string(),
                ),
            )]
        })
    }
}

impl TurnInputContributor for MemoriesExtension {
    fn contribute<'a>(
        &'a self,
        input: TurnInputContext<'a>,
        _extension_metrics: Option<Arc<dyn codex_extension_api::ExtensionMetrics>>,
        _session_store: &'a ExtensionData,
        thread_store: &'a ExtensionData,
        _turn_store: &'a ExtensionData,
    ) -> ExtensionFuture<'a, Vec<Box<dyn ContextualUserFragment + Send>>> {
        Box::pin(async move {
            let Some(config) = thread_store.get::<MemoriesExtensionConfig>() else {
                return Vec::new();
            };
            // A content-free host marker lets prompt projection distinguish v2
            // legacy payloads from the shared v1 instruction kind. Emit it even
            // without candidates or when memory has just been disabled.
            let (boundary, candidate) = crate::retrieval::prepare_for_version(
                config.root.as_ref(),
                &input.user_input,
                config.enabled,
                config.version,
                config.scope == MemoryScope::Global,
            )
            .await;
            let mut fragments: Vec<Box<dyn ContextualUserFragment + Send>> =
                vec![Box::new(boundary)];
            if let Some(fragment) = candidate {
                fragments.push(Box::new(fragment));
            }
            fragments
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
            config.scope = previous.scope;
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
        if !config.enabled || !config.dedicated_tools {
            return Vec::new();
        }

        let Some(root) = config.root.as_ref() else {
            return Vec::new();
        };
        tools::memory_tools(
            LocalMemoriesBackend::from_memory_root_and_version(root.to_path_buf(), config.version),
            self.metrics_client.clone(),
        )
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
    registry.turn_input_contributor(extension.clone());
    registry.tool_contributor(extension);
}
