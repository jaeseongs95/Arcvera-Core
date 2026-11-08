use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use clap::ArgGroup;
use clap::builder::TypedValueParser;
use codex_config::types::AppToolApproval;
use codex_config::types::McpServerAuth;
use codex_config::types::McpServerConfig;
use codex_config::types::McpServerOAuthConfig;
use codex_config::types::McpServerTransportConfig;
use codex_core::McpManager;
use codex_core::config::Config;
use codex_core::config::ConfigBuilder;
use codex_core::config::LoaderOverrides;
use codex_core::config::edit::ConfigEditsBuilder;
use codex_core::config::find_codex_home;
use codex_core::config::load_global_mcp_servers;
use codex_core::plugins_manager_for_config;
use codex_exec_server::EnvironmentManager;
use codex_exec_server::HttpClient;
use codex_exec_server::RouteAwareHttpClient;
use codex_login::AuthManager;
use codex_mcp::McpOAuthLoginSupport;
use codex_mcp::McpRuntimeContext;
use codex_mcp::apply_http_headers_helper;
use codex_mcp::compute_auth_statuses;
use codex_mcp::discover_supported_scopes;
use codex_mcp::ema_auth_scope;
use codex_mcp::oauth_login_support;
use codex_mcp::resolve_oauth_callback;
use codex_mcp::resolve_oauth_scopes;
use codex_protocol::protocol::McpAuthStatus;
use codex_rmcp_client::McpOAuthCallbackMode;
use codex_rmcp_client::McpOAuthClientRegistration;
use codex_rmcp_client::OAuthDiscoveryTimeout;
use codex_rmcp_client::StreamableHttpRedirectMode;
use codex_rmcp_client::delete_enterprise_oauth_tokens;
use codex_rmcp_client::delete_oauth_tokens;
use codex_rmcp_client::resolve_mcp_oauth_callback_url;
use codex_utils_cli::CliConfigOverrides;
use codex_utils_cli::format_env_display;
use codex_utils_redacted_string::RedactedString;

use crate::cloud_config;
use crate::mcp_login::McpLoginMode;
use crate::mcp_login::perform_oauth_login_retry_without_scopes;
use crate::plugin_cmd::load_cli_auth_manager;

#[cfg(test)]
#[path = "mcp_cmd_tests.rs"]
mod tests;

/// Subcommands:
/// - `list`   — list configured servers (with `--json`)
/// - `get`    — show a single server (with `--json`)
/// - `add`    — add a server launcher entry to `~/.codex/config.toml`
/// - `remove` — delete a server entry
/// - `login`  — authenticate with MCP server using OAuth
/// - `logout` — remove OAuth credentials for MCP server
#[derive(Debug, clap::Parser)]
pub struct McpCli {
    #[clap(flatten)]
    pub config_overrides: CliConfigOverrides,

    #[command(subcommand)]
    pub subcommand: McpSubcommand,
}

#[derive(Debug, clap::Subcommand)]
pub enum McpSubcommand {
    List(ListArgs),
    Get(GetArgs),
    Add(AddArgs),
    Remove(RemoveArgs),
    Login(LoginArgs),
    Logout(LogoutArgs),
}

#[derive(Debug, clap::Parser)]
pub struct ListArgs {
    /// Output the configured servers as JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, clap::Parser)]
pub struct GetArgs {
    /// Name of the MCP server to display.
    pub name: String,

    /// Output the server configuration as JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, clap::Parser)]
#[command(override_usage = "codex mcp add [OPTIONS] <NAME> (--url <URL> | -- <COMMAND>...)")]
pub struct AddArgs {
    /// Name for the MCP server configuration.
    pub name: String,

    #[command(flatten)]
    pub transport_args: AddMcpTransportArgs,
}

#[derive(Debug, clap::Args)]
#[command(
    group(
        ArgGroup::new("transport")
            .args(["command", "url"])
            .required(true)
            .multiple(false)
    )
)]
pub struct AddMcpTransportArgs {
    #[command(flatten)]
    pub stdio: Option<AddMcpStdioArgs>,

    #[command(flatten)]
    pub streamable_http: Option<AddMcpStreamableHttpArgs>,
}

#[derive(Debug, clap::Args)]
pub struct AddMcpStdioArgs {
    /// Command to launch the MCP server.
    /// Use --url for a streamable HTTP server.
    #[arg(
            trailing_var_arg = true,
            num_args = 0..,
        )]
    pub command: Vec<String>,

    /// Environment variables to set when launching the server.
    /// Only valid with stdio servers.
    #[arg(
        long,
        value_parser = parse_env_pair,
        value_name = "KEY=VALUE",
    )]
    pub env: Vec<(String, String)>,
}

#[derive(Debug, clap::Args)]
pub struct AddMcpStreamableHttpArgs {
    /// URL for a streamable HTTP MCP server.
    #[arg(long)]
    pub url: String,

    /// Optional environment variable to read for a bearer token.
    /// Only valid with streamable HTTP servers.
    #[arg(
        long = "bearer-token-env-var",
        value_name = "ENV_VAR",
        requires = "url"
    )]
    pub bearer_token_env_var: Option<String>,

    /// Optional OAuth client identifier to use for this MCP server.
    #[arg(long = "oauth-client-id", value_name = "CLIENT_ID", requires = "url")]
    pub oauth_client_id: Option<String>,

    /// Optional OAuth client secret for the pre-registered client.
    #[arg(
        long = "oauth-client-secret",
        value_name = "CLIENT_SECRET",
        value_parser = clap::builder::StringValueParser::new().map(RedactedString::from),
        requires_all = ["url", "oauth_client_id"]
    )]
    pub oauth_client_secret: Option<RedactedString>,

    /// OAuth client-registration strategy for the immediate login only.
    #[arg(
        long = "oauth-client-registration",
        value_enum,
        value_name = "AUTO|CIMD|DCR",
        requires = "url"
    )]
    pub oauth_client_registration: Option<McpOAuthClientRegistrationArg>,

    /// Optional OAuth resource parameter to include during MCP login.
    #[arg(long = "oauth-resource", value_name = "RESOURCE", requires = "url")]
    pub oauth_resource: Option<String>,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum McpOAuthClientRegistrationArg {
    Auto,
    Cimd,
    Dcr,
}

impl From<McpOAuthClientRegistrationArg> for McpOAuthClientRegistration {
    fn from(value: McpOAuthClientRegistrationArg) -> Self {
        match value {
            McpOAuthClientRegistrationArg::Auto => Self::Auto,
            McpOAuthClientRegistrationArg::Cimd => Self::Cimd,
            McpOAuthClientRegistrationArg::Dcr => Self::Dcr,
        }
    }
}

#[derive(Debug, clap::Parser)]
pub struct RemoveArgs {
    /// Name of the MCP server configuration to remove.
    pub name: String,
}

#[derive(Debug, clap::Parser)]
pub struct LoginArgs {
    /// Name of the MCP server to authenticate with oauth.
    pub name: String,

    /// Print the authorization URL and accept the callback URL without opening a browser.
    #[arg(long)]
    pub no_browser: bool,

    /// Comma-separated list of OAuth scopes to request.
    #[arg(long, value_delimiter = ',', value_name = "SCOPE,SCOPE")]
    pub scopes: Vec<String>,

    /// OAuth client-registration strategy for this login only.
    #[arg(
        long = "oauth-client-registration",
        value_enum,
        value_name = "AUTO|CIMD|DCR"
    )]
    pub oauth_client_registration: Option<McpOAuthClientRegistrationArg>,
}

#[derive(Debug, clap::Parser)]
pub struct LogoutArgs {
    /// Name of the MCP server to deauthenticate.
    pub name: String,
}

impl McpCli {
    pub async fn run(self, loader_overrides: LoaderOverrides) -> Result<()> {
        let McpCli {
            config_overrides,
            subcommand,
        } = self;

        if loader_overrides.user_config_profile.is_some() {
            validate_profile_v2_migration(&config_overrides, loader_overrides.clone()).await?;
        }

        match subcommand {
            McpSubcommand::List(args) => {
                let config = cloud_config::load_config(&config_overrides, loader_overrides).await?;
                run_list(&config, args).await?;
            }
            McpSubcommand::Get(args) => {
                let config = cloud_config::load_config(&config_overrides, loader_overrides).await?;
                run_get(&config, args).await?;
            }
            McpSubcommand::Add(args) => {
                let config = cloud_config::load_config(&config_overrides, loader_overrides).await?;
                run_add(&config, args).await?;
            }
            McpSubcommand::Remove(args) => {
                run_remove(&config_overrides, args).await?;
            }
            McpSubcommand::Login(args) => {
                let config = cloud_config::load_config(&config_overrides, loader_overrides).await?;
                run_login(&config, args).await?;
            }
            McpSubcommand::Logout(args) => {
                let config = cloud_config::load_config(&config_overrides, loader_overrides).await?;
                run_logout(&config, args).await?;
            }
        }

        Ok(())
    }
}

async fn validate_profile_v2_migration(
    config_overrides: &CliConfigOverrides,
    loader_overrides: LoaderOverrides,
) -> Result<()> {
    let overrides = config_overrides
        .parse_overrides()
        .map_err(anyhow::Error::msg)?;
    ConfigBuilder::default()
        .cli_overrides(overrides)
        .loader_overrides(loader_overrides)
        .build()
        .await
        .context(codex_tui::i18n::text("failed to load configuration"))?;
    Ok(())
}

async fn run_add(config: &Config, add_args: AddArgs) -> Result<()> {
    let AddArgs {
        name,
        transport_args,
    } = add_args;

    validate_server_name(&name)?;

    let codex_home = &config.codex_home;

    let (transport, oauth_client_id, oauth_client_secret, client_registration, oauth_resource) =
        match transport_args {
            AddMcpTransportArgs {
                stdio: Some(stdio), ..
            } => {
                let mut command_parts = stdio.command.into_iter();
                let command_bin = command_parts
                    .next()
                    .ok_or_else(|| anyhow!("{}", codex_tui::i18n::text("command is required")))?;
                let command_args: Vec<String> = command_parts.collect();

                let env_map = if stdio.env.is_empty() {
                    None
                } else {
                    Some(stdio.env.into_iter().collect::<HashMap<_, _>>())
                };
                (
                    McpServerTransportConfig::Stdio {
                        command: command_bin,
                        args: command_args,
                        env: env_map,
                        env_vars: Vec::new(),
                        cwd: None,
                    },
                    None,
                    None,
                    McpOAuthClientRegistration::Auto,
                    None,
                )
            }
            AddMcpTransportArgs {
                streamable_http:
                    Some(AddMcpStreamableHttpArgs {
                        url,
                        bearer_token_env_var,
                        oauth_client_id,
                        oauth_client_secret,
                        oauth_client_registration,
                        oauth_resource,
                    }),
                ..
            } => (
                McpServerTransportConfig::StreamableHttp {
                    url,
                    bearer_token_env_var,
                    http_headers: None,
                    env_http_headers: None,
                    http_headers_helper: None,
                },
                oauth_client_id,
                oauth_client_secret,
                oauth_client_registration
                    .map(McpOAuthClientRegistration::from)
                    .unwrap_or_default(),
                oauth_resource,
            ),
            AddMcpTransportArgs { .. } => {
                bail!(
                    "{}",
                    codex_tui::i18n::text("exactly one of --command or --url must be provided")
                )
            }
        };

    if let Some(secret) = &oauth_client_secret {
        if secret.trim().is_empty() {
            bail!(
                "{}",
                codex_tui::i18n::text("--oauth-client-secret must not be empty")
            );
        }
        if oauth_client_id
            .as_deref()
            .is_none_or(|client_id| client_id.trim().is_empty())
        {
            bail!(
                "{}",
                codex_tui::i18n::text(
                    "--oauth-client-secret requires a nonempty --oauth-client-id"
                )
            );
        }
    }

    // Discover once before saving so a new registered client keeps the exact
    // callback its provider expects, including issuer-bound stable callbacks.
    let http_client: Arc<dyn HttpClient> = Arc::new(
        RouteAwareHttpClient::new(config.http_client_factory()).with_tls_backend_fallback(),
    );
    let login_support = oauth_login_support(
        &transport,
        Arc::clone(&http_client),
        OAuthDiscoveryTimeout::LOCAL,
        StreamableHttpRedirectMode::Legacy,
    )
    .await;
    // A stable callback requires confirmed issuer support. Discovery failures
    // and servers without support both retain the callback-ID mix-up defense.
    let callback_mode = match &login_support {
        McpOAuthLoginSupport::Supported(oauth_config) => oauth_config.callback_mode,
        McpOAuthLoginSupport::Unsupported | McpOAuthLoginSupport::Unknown(_) => {
            McpOAuthCallbackMode::CallbackSpecific
        }
    };

    // DCR and CIMD register their callbacks during login; only an existing
    // client ID has a redirect that must be persisted with the MCP server.
    let callback_url = oauth_client_id
        .as_ref()
        .map(|_| {
            let McpServerTransportConfig::StreamableHttp { url, .. } = &transport else {
                bail!(
                    "{}",
                    codex_tui::i18n::text("OAuth client IDs require a streamable HTTP MCP server")
                );
            };
            resolve_mcp_oauth_callback_url(
                url,
                config.mcp_oauth_callback_url.as_deref(),
                callback_mode,
            )
        })
        .transpose()?;

    let new_entry = McpServerConfig {
        auth: Default::default(),
        transport: transport.clone(),
        environment_id: codex_config::DEFAULT_MCP_SERVER_ENVIRONMENT_ID.to_string(),
        enabled: true,
        required: false,
        startup_readiness: Default::default(),
        supports_parallel_tool_calls: false,
        tool_input_schema_max_bytes: None,
        omit_tools_from: None,
        disabled_reason: None,
        startup_timeout_sec: None,
        tool_timeout_sec: None,
        default_tools_approval_mode: None,
        enabled_tools: None,
        disabled_tools: None,
        scopes: None,
        oauth: oauth_client_id
            .clone()
            .map(|client_id| McpServerOAuthConfig {
                client_id: Some(client_id),
                client_secret: oauth_client_secret,
                callback_url: callback_url.clone(),
                callback_port: None,
                ..Default::default()
            }),
        oauth_resource: oauth_resource.clone(),
        tools: HashMap::new(),
    };

    let mut servers = load_global_mcp_servers(codex_home).await.with_context(|| {
        codex_tui::i18n::message(
            "r39-cli-mcp-load-servers",
            &[("path", &codex_home.display().to_string())],
        )
    })?;
    let credential_name = new_entry.oauth_credential_name(&name);
    servers.insert(name.clone(), new_entry);

    ConfigEditsBuilder::new(codex_home)
        .replace_mcp_servers(&servers)
        .apply()
        .await
        .with_context(|| {
            codex_tui::i18n::message(
                "r39-cli-mcp-write-servers",
                &[("path", &codex_home.display().to_string())],
            )
        })?;

    println!(
        "{}",
        codex_tui::i18n::message("r39-cli-mcp-added-server", &[("name", &name)])
    );
    if let Some(callback_url) = &callback_url {
        println!(
            "{}",
            codex_tui::i18n::message("r39-cli-mcp-callback-url", &[("url", callback_url)])
        );
    }
    match login_support {
        McpOAuthLoginSupport::Supported(oauth_config) => {
            println!(
                "{}",
                codex_tui::i18n::text("Detected OAuth support. Starting OAuth flow…")
            );
            let resolved_scopes = resolve_oauth_scopes(
                /*explicit_scopes*/ None,
                /*configured_scopes*/ None,
                oauth_config.discovered_scopes.clone(),
            );
            perform_oauth_login_retry_without_scopes(
                credential_name.as_ref(),
                &oauth_config.url,
                config.mcp_oauth_credentials_store_mode,
                config.auth_keyring_backend_kind(),
                oauth_config.http_headers,
                oauth_config.env_http_headers,
                &resolved_scopes,
                servers[&name].oauth.as_ref(),
                client_registration,
                oauth_resource.as_deref(),
                config.mcp_oauth_callback_port,
                callback_url
                    .as_deref()
                    .or(config.mcp_oauth_callback_url.as_deref()),
                config.mcp_oauth_callback_url.as_deref(),
                http_client,
                McpLoginMode::Browser,
            )
            .await?;
            println!("{}", codex_tui::i18n::text("Successfully logged in."));
        }
        McpOAuthLoginSupport::Unsupported => {}
        McpOAuthLoginSupport::Unknown(_) => println!(
            "{}",
            codex_tui::i18n::message("r39-cli-mcp-login-unknown", &[("name", &name)])
        ),
    }

    Ok(())
}

async fn run_remove(config_overrides: &CliConfigOverrides, remove_args: RemoveArgs) -> Result<()> {
    config_overrides
        .parse_overrides()
        .map_err(anyhow::Error::msg)?;

    let RemoveArgs { name } = remove_args;

    validate_server_name(&name)?;

    let codex_home =
        find_codex_home().context(codex_tui::i18n::text("failed to resolve CODEX_HOME"))?;
    let mut servers = load_global_mcp_servers(&codex_home)
        .await
        .with_context(|| {
            codex_tui::i18n::message(
                "r39-cli-mcp-load-servers",
                &[("path", &codex_home.display().to_string())],
            )
        })?;

    let removed = servers.remove(&name).is_some();

    if removed {
        ConfigEditsBuilder::new(&codex_home)
            .replace_mcp_servers(&servers)
            .apply()
            .await
            .with_context(|| {
                codex_tui::i18n::message(
                    "r39-cli-mcp-write-servers",
                    &[("path", &codex_home.display().to_string())],
                )
            })?;
    }

    if removed {
        println!(
            "{}",
            codex_tui::i18n::message("r39-cli-mcp-removed-server", &[("name", &name)])
        );
    } else {
        println!(
            "{}",
            codex_tui::i18n::message("r39-cli-mcp-server-not-found", &[("name", &name)])
        );
    }

    Ok(())
}

async fn load_mcp_manager(config: &Config) -> Result<McpManager> {
    let plugins_manager = Arc::new(plugins_manager_for_config(
        config,
        load_cli_auth_manager(config).await?,
    ));
    Ok(McpManager::new(plugins_manager))
}

async fn run_login(config: &Config, login_args: LoginArgs) -> Result<()> {
    let mcp_manager = load_mcp_manager(config).await?;
    let mcp_servers = mcp_manager.configured_servers(config).await;

    let LoginArgs {
        name,
        no_browser,
        scopes,
        oauth_client_registration,
    } = login_args;
    let client_registration = oauth_client_registration
        .map(McpOAuthClientRegistration::from)
        .unwrap_or_default();

    let Some(server) = mcp_servers.get(&name) else {
        bail!(
            "{}",
            codex_tui::i18n::message("r39-cli-mcp-server-not-found", &[("name", &name)])
        );
    };

    if matches!(server.auth, codex_config::types::McpServerAuth::EmaAuth) {
        bail!(
            "{}",
            codex_tui::i18n::text(
                "Enterprise MCP authorization is managed by Codex account sign-in. Open Codex to sign in."
            )
        );
    }

    let (url, http_headers, env_http_headers) = match &server.transport {
        McpServerTransportConfig::StreamableHttp {
            url,
            http_headers,
            env_http_headers,
            ..
        } => (url.clone(), http_headers.clone(), env_http_headers.clone()),
        _ => bail!(
            "{}",
            codex_tui::i18n::text("OAuth login is only supported for streamable HTTP servers.")
        ),
    };

    // Standalone `mcp login` runs OAuth from the local CLI process; execution
    // environment routing belongs to app-server and session MCP flows.
    let http_client: Arc<dyn HttpClient> = Arc::new(
        RouteAwareHttpClient::new(config.http_client_factory()).with_tls_backend_fallback(),
    );
    let http_client = apply_http_headers_helper(http_client, server, config.cwd.to_path_buf())
        .map_err(anyhow::Error::msg)?;
    let explicit_scopes = (!scopes.is_empty()).then_some(scopes);
    let discovered_scopes = if explicit_scopes.is_none() && server.scopes.is_none() {
        discover_supported_scopes(
            &server.transport,
            Arc::clone(&http_client),
            OAuthDiscoveryTimeout::LOCAL,
            StreamableHttpRedirectMode::Legacy,
        )
        .await
    } else {
        None
    };
    let resolved_scopes =
        resolve_oauth_scopes(explicit_scopes, server.scopes.clone(), discovered_scopes);
    let credential_name = server.oauth_credential_name(&name);
    let callback_url =
        resolve_oauth_callback(server, &url, config.mcp_oauth_callback_url.as_deref())?;

    perform_oauth_login_retry_without_scopes(
        credential_name.as_ref(),
        &url,
        config.mcp_oauth_credentials_store_mode,
        config.auth_keyring_backend_kind(),
        http_headers,
        env_http_headers,
        &resolved_scopes,
        server.oauth.as_ref(),
        client_registration,
        server.oauth_resource.as_deref(),
        server.oauth_callback_port(config.mcp_oauth_callback_port),
        callback_url.as_deref(),
        config.mcp_oauth_callback_url.as_deref(),
        http_client,
        if no_browser {
            McpLoginMode::PasteCallback
        } else {
            McpLoginMode::Browser
        },
    )
    .await?;
    println!(
        "{}",
        codex_tui::i18n::message("r39-cli-mcp-logged-in", &[("name", &name)])
    );
    Ok(())
}

async fn run_logout(config: &Config, logout_args: LogoutArgs) -> Result<()> {
    let mcp_manager = load_mcp_manager(config).await?;
    let mcp_servers = mcp_manager.configured_servers(config).await;

    let LogoutArgs { name } = logout_args;

    let server = mcp_servers.get(&name).ok_or_else(|| {
        anyhow!(
            "{}",
            codex_tui::i18n::message("r39-cli-mcp-server-missing-config", &[("name", &name)])
        )
    })?;

    if matches!(server.auth, McpServerAuth::EmaAuth) {
        let auth_manager =
            AuthManager::shared_from_config(config, /*enable_codex_api_key_env*/ true).await?;
        let auth = auth_manager.auth().await;
        let scope = ema_auth_scope(auth.as_ref()).context(codex_tui::i18n::text(
            "Sign in to your Codex account before removing enterprise authorization",
        ))?;
        // Cleanup must remain available even when this server is no longer eligible
        // for EMA (for example, after disabling the feature or changing its transport).
        let profile =
            config
                .mcp_enterprise_managed_auth
                .as_ref()
                .context(codex_tui::i18n::text(
                    "EMA logout requires a trusted enterprise IdP profile",
                ))?;
        let idp = &profile.idp;
        let credential_name = idp.credential_name(&scope);
        match delete_enterprise_oauth_tokens(
            &credential_name,
            &idp.issuer,
            config.auth_keyring_backend_kind(),
        )
        .await
        {
            Ok(true) => println!(
                "{}",
                codex_tui::i18n::message("r39-cli-mcp-removed-enterprise", &[("name", &name)])
            ),
            Ok(false) => println!(
                "{}",
                codex_tui::i18n::text("No shared enterprise authorization is stored.")
            ),
            Err(_) => {
                return Err(anyhow!(
                    "{}",
                    codex_tui::i18n::text("failed to delete enterprise authorization")
                ));
            }
        }
        return Ok(());
    }

    let url = match &server.transport {
        McpServerTransportConfig::StreamableHttp { url, .. } => url.clone(),
        _ => bail!(
            "{}",
            codex_tui::i18n::text("OAuth logout is only supported for streamable_http transports.")
        ),
    };
    let credential_name = server.oauth_credential_name(&name);

    match delete_oauth_tokens(
        credential_name.as_ref(),
        &url,
        config.mcp_oauth_credentials_store_mode,
        config.auth_keyring_backend_kind(),
    )
    .await
    {
        Ok(true) => println!(
            "{}",
            codex_tui::i18n::message("r39-cli-mcp-removed-oauth", &[("name", &name)])
        ),
        Ok(false) => println!(
            "{}",
            codex_tui::i18n::message("r39-cli-mcp-no-oauth", &[("name", &name)])
        ),
        Err(err) => {
            return Err(anyhow!(
                "{}",
                codex_tui::i18n::message(
                    "r39-cli-mcp-delete-oauth-failed",
                    &[("error", &err.to_string())]
                )
            ));
        }
    }

    Ok(())
}

async fn run_list(config: &Config, list_args: ListArgs) -> Result<()> {
    let mcp_manager = load_mcp_manager(config).await?;
    let auth_manager =
        AuthManager::shared_from_config(config, /*enable_codex_api_key_env*/ true).await?;
    let auth = auth_manager.auth().await;
    let mcp_servers = mcp_manager.configured_servers(config).await;
    let runtime_config = mcp_manager.runtime_config(config).await;
    let effective_mcp_servers = codex_mcp::effective_mcp_servers(&runtime_config, auth.as_ref());

    let mut entries: Vec<_> = mcp_servers.iter().collect();
    entries.sort_by_key(|(name, _)| *name);
    // Standalone `mcp list` only discovers auth through local HTTP; it has
    // never initialized or routed through remote execution environments.
    let runtime_context = McpRuntimeContext::new(
        Arc::new(EnvironmentManager::without_environments(
            config.http_client_factory(),
        )),
        config.cwd.to_path_buf(),
    );
    let auth_statuses = compute_auth_statuses(
        effective_mcp_servers.iter(),
        config.mcp_oauth_credentials_store_mode,
        config.auth_keyring_backend_kind(),
        auth.as_ref(),
        Some(&auth_manager),
        &runtime_config.ema_state,
        &runtime_context,
    )
    .await;

    if list_args.json {
        let json_entries: Vec<_> = entries
            .into_iter()
            .map(|(name, cfg)| {
                let auth_status = auth_statuses
                    .get(name.as_str())
                    .map(|entry| McpAuthStatus::from(entry.auth_state))
                    .unwrap_or(McpAuthStatus::Unsupported);
                let transport = match &cfg.transport {
                    McpServerTransportConfig::Stdio {
                        command,
                        args,
                        env,
                        env_vars,
                        cwd,
                    } => serde_json::json!({
                        "type": "stdio",
                        "command": command,
                        "args": args,
                        "env": env,
                        "env_vars": env_vars,
                        "cwd": cwd,
                    }),
                    McpServerTransportConfig::StreamableHttp {
                        url,
                        bearer_token_env_var,
                        http_headers,
                        env_http_headers,
                        http_headers_helper,
                    } => {
                        serde_json::json!({
                            "type": "streamable_http",
                            "url": url,
                            "bearer_token_env_var": bearer_token_env_var,
                            "http_headers": http_headers,
                            "env_http_headers": env_http_headers,
                            "http_headers_helper": http_headers_helper
                                .as_ref()
                                .map(|_| "<redacted>"),
                        })
                    }
                };

                serde_json::json!({
                    "name": name,
                    "enabled": cfg.enabled,
                    "disabled_reason": cfg.disabled_reason.as_ref().map(ToString::to_string),
                    "transport": transport,
                    "startup_timeout_sec": cfg
                        .startup_timeout_sec
                        .map(|timeout| timeout.as_secs_f64()),
                    "tool_timeout_sec": cfg
                        .tool_timeout_sec
                        .map(|timeout| timeout.as_secs_f64()),
                    "auth_status": auth_status,
                })
            })
            .collect();
        let output = serde_json::to_string_pretty(&json_entries)?;
        println!("{output}");
        return Ok(());
    }

    if entries.is_empty() {
        println!(
            "{}",
            codex_tui::i18n::text(
                "No MCP servers configured yet. Try `codex mcp add my-tool -- my-command`."
            )
        );
        return Ok(());
    }

    let mut stdio_rows: Vec<[String; 7]> = Vec::new();
    let mut http_rows: Vec<[String; 5]> = Vec::new();

    for (name, cfg) in entries {
        match &cfg.transport {
            McpServerTransportConfig::Stdio {
                command,
                args,
                env,
                env_vars,
                cwd,
            } => {
                let args_display = if args.is_empty() {
                    "-".to_string()
                } else {
                    args.join(" ")
                };
                let env_display = format_env_display(env.as_ref(), env_vars);
                let cwd_display = cwd
                    .as_ref()
                    .map(ToString::to_string)
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| "-".to_string());
                let status = format_mcp_status(cfg);
                let auth_status = auth_statuses
                    .get(name.as_str())
                    .map(|entry| McpAuthStatus::from(entry.auth_state))
                    .unwrap_or(McpAuthStatus::Unsupported)
                    .to_string();
                let auth_status = codex_tui::i18n::text(&auth_status).to_string();
                stdio_rows.push([
                    name.clone(),
                    command.clone(),
                    args_display,
                    env_display,
                    cwd_display,
                    status,
                    auth_status,
                ]);
            }
            McpServerTransportConfig::StreamableHttp {
                url,
                bearer_token_env_var,
                ..
            } => {
                let status = format_mcp_status(cfg);
                let auth_status = auth_statuses
                    .get(name.as_str())
                    .map(|entry| McpAuthStatus::from(entry.auth_state))
                    .unwrap_or(McpAuthStatus::Unsupported)
                    .to_string();
                let auth_status = codex_tui::i18n::text(&auth_status).to_string();
                let bearer_token_display =
                    bearer_token_env_var.as_deref().unwrap_or("-").to_string();
                http_rows.push([
                    name.clone(),
                    url.clone(),
                    bearer_token_display,
                    status,
                    auth_status,
                ]);
            }
        }
    }

    if !stdio_rows.is_empty() {
        let mut widths = [
            crate::localization::display_width(codex_tui::i18n::text("Name")),
            crate::localization::display_width(codex_tui::i18n::text("Command")),
            crate::localization::display_width(codex_tui::i18n::text("Args")),
            crate::localization::display_width(codex_tui::i18n::text("Env")),
            crate::localization::display_width(codex_tui::i18n::text("Cwd")),
            crate::localization::display_width(codex_tui::i18n::text("Status")),
            crate::localization::display_width(codex_tui::i18n::text("Auth")),
        ];
        for row in &stdio_rows {
            for (i, cell) in row.iter().enumerate() {
                widths[i] = widths[i].max(crate::localization::display_width(cell));
            }
        }

        println!(
            "{}  {}  {}  {}  {}  {}  {}",
            crate::localization::pad_to_width(codex_tui::i18n::text("Name"), widths[0]),
            crate::localization::pad_to_width(codex_tui::i18n::text("Command"), widths[1]),
            crate::localization::pad_to_width(codex_tui::i18n::text("Args"), widths[2]),
            crate::localization::pad_to_width(codex_tui::i18n::text("Env"), widths[3]),
            crate::localization::pad_to_width(codex_tui::i18n::text("Cwd"), widths[4]),
            crate::localization::pad_to_width(codex_tui::i18n::text("Status"), widths[5]),
            crate::localization::pad_to_width(codex_tui::i18n::text("Auth"), widths[6])
        );

        for row in &stdio_rows {
            println!(
                "{}  {}  {}  {}  {}  {}  {}",
                crate::localization::pad_to_width(row[0].as_str(), widths[0]),
                crate::localization::pad_to_width(row[1].as_str(), widths[1]),
                crate::localization::pad_to_width(row[2].as_str(), widths[2]),
                crate::localization::pad_to_width(row[3].as_str(), widths[3]),
                crate::localization::pad_to_width(row[4].as_str(), widths[4]),
                crate::localization::pad_to_width(row[5].as_str(), widths[5]),
                crate::localization::pad_to_width(row[6].as_str(), widths[6])
            );
        }
    }

    if !stdio_rows.is_empty() && !http_rows.is_empty() {
        println!();
    }

    if !http_rows.is_empty() {
        let mut widths = [
            crate::localization::display_width(codex_tui::i18n::text("Name")),
            crate::localization::display_width(codex_tui::i18n::text("Url")),
            crate::localization::display_width(codex_tui::i18n::text("Bearer Token Env Var")),
            crate::localization::display_width(codex_tui::i18n::text("Status")),
            crate::localization::display_width(codex_tui::i18n::text("Auth")),
        ];
        for row in &http_rows {
            for (i, cell) in row.iter().enumerate() {
                widths[i] = widths[i].max(crate::localization::display_width(cell));
            }
        }

        println!(
            "{}  {}  {}  {}  {}",
            crate::localization::pad_to_width(codex_tui::i18n::text("Name"), widths[0]),
            crate::localization::pad_to_width(codex_tui::i18n::text("Url"), widths[1]),
            crate::localization::pad_to_width(
                codex_tui::i18n::text("Bearer Token Env Var"),
                widths[2]
            ),
            crate::localization::pad_to_width(codex_tui::i18n::text("Status"), widths[3]),
            crate::localization::pad_to_width(codex_tui::i18n::text("Auth"), widths[4])
        );

        for row in &http_rows {
            println!(
                "{}  {}  {}  {}  {}",
                crate::localization::pad_to_width(row[0].as_str(), widths[0]),
                crate::localization::pad_to_width(row[1].as_str(), widths[1]),
                crate::localization::pad_to_width(row[2].as_str(), widths[2]),
                crate::localization::pad_to_width(row[3].as_str(), widths[3]),
                crate::localization::pad_to_width(row[4].as_str(), widths[4])
            );
        }
    }

    Ok(())
}

async fn run_get(config: &Config, get_args: GetArgs) -> Result<()> {
    let mcp_manager = load_mcp_manager(config).await?;
    let mcp_servers = mcp_manager.configured_servers(config).await;

    let Some(server) = mcp_servers.get(&get_args.name) else {
        bail!(
            "{}",
            codex_tui::i18n::message("r39-cli-mcp-server-not-found", &[("name", &get_args.name)])
        );
    };

    if get_args.json {
        let transport = match &server.transport {
            McpServerTransportConfig::Stdio {
                command,
                args,
                env,
                env_vars,
                cwd,
            } => serde_json::json!({
                "type": "stdio",
                "command": command,
                "args": args,
                "env": env,
                "env_vars": env_vars,
                "cwd": cwd,
            }),
            McpServerTransportConfig::StreamableHttp {
                url,
                bearer_token_env_var,
                http_headers,
                env_http_headers,
                http_headers_helper,
            } => serde_json::json!({
                "type": "streamable_http",
                "url": url,
                "bearer_token_env_var": bearer_token_env_var,
                "http_headers": http_headers,
                "env_http_headers": env_http_headers,
                "http_headers_helper": http_headers_helper
                    .as_ref()
                    .map(|_| "<redacted>"),
            }),
        };
        let output = serde_json::to_string_pretty(&serde_json::json!({
            "name": get_args.name,
            "enabled": server.enabled,
            "disabled_reason": server.disabled_reason.as_ref().map(ToString::to_string),
            "transport": transport,
            "enabled_tools": server.enabled_tools.clone(),
            "disabled_tools": server.disabled_tools.clone(),
            "startup_timeout_sec": server
                .startup_timeout_sec
                .map(|timeout| timeout.as_secs_f64()),
            "tool_timeout_sec": server
                .tool_timeout_sec
                .map(|timeout| timeout.as_secs_f64()),
        }))?;
        println!("{output}");
        return Ok(());
    }

    if !server.enabled {
        if let Some(reason) = server.disabled_reason.as_ref() {
            println!(
                "{}",
                codex_tui::i18n::message(
                    "r39-cli-mcp-disabled-reason",
                    &[("name", &get_args.name), ("reason", &reason.to_string())]
                )
            );
        } else {
            println!(
                "{}",
                codex_tui::i18n::message("r39-cli-mcp-disabled-name", &[("name", &get_args.name)])
            );
        }
        return Ok(());
    }

    println!("{}", get_args.name);
    println!(
        "{}",
        codex_tui::i18n::message(
            "r39-cli-mcp-field-enabled",
            &[("value", &server.enabled.to_string())]
        )
    );
    let format_tool_list = |tools: &Option<Vec<String>>| -> String {
        match tools {
            Some(list) if list.is_empty() => "[]".to_string(),
            Some(list) => list.join(", "),
            None => "-".to_string(),
        }
    };
    if server.enabled_tools.is_some() {
        let enabled_tools_display = format_tool_list(&server.enabled_tools);
        println!(
            "{}",
            codex_tui::i18n::message(
                "r39-cli-mcp-field-enabled-tools",
                &[("value", &enabled_tools_display)]
            )
        );
    }
    if server.disabled_tools.is_some() {
        let disabled_tools_display = format_tool_list(&server.disabled_tools);
        println!(
            "{}",
            codex_tui::i18n::message(
                "r39-cli-mcp-field-disabled-tools",
                &[("value", &disabled_tools_display)]
            )
        );
    }
    match &server.transport {
        McpServerTransportConfig::Stdio {
            command,
            args,
            env,
            env_vars,
            cwd,
        } => {
            println!("{}", codex_tui::i18n::text("  transport: stdio"));
            println!(
                "{}",
                codex_tui::i18n::message("r39-cli-mcp-field-command", &[("value", command)])
            );
            let args_display = if args.is_empty() {
                "-".to_string()
            } else {
                args.join(" ")
            };
            println!(
                "{}",
                codex_tui::i18n::message("r39-cli-mcp-field-args", &[("value", &args_display)])
            );
            let cwd_display = cwd
                .as_ref()
                .map(ToString::to_string)
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "-".to_string());
            println!(
                "{}",
                codex_tui::i18n::message("r39-cli-mcp-field-cwd", &[("value", &cwd_display)])
            );
            let env_display = format_env_display(env.as_ref(), env_vars);
            println!(
                "{}",
                codex_tui::i18n::message("r39-cli-mcp-field-env", &[("value", &env_display)])
            );
        }
        McpServerTransportConfig::StreamableHttp {
            url,
            bearer_token_env_var,
            http_headers,
            env_http_headers,
            http_headers_helper,
        } => {
            println!("{}", codex_tui::i18n::text("  transport: streamable_http"));
            println!(
                "{}",
                codex_tui::i18n::message("r39-cli-mcp-field-url", &[("value", url)])
            );
            let bearer_token_display = bearer_token_env_var.as_deref().unwrap_or("-");
            println!(
                "{}",
                codex_tui::i18n::message(
                    "r39-cli-mcp-field-bearer-env",
                    &[("value", bearer_token_display)]
                )
            );
            let headers_display = match http_headers {
                Some(map) if !map.is_empty() => {
                    let mut pairs: Vec<_> = map.iter().collect();
                    pairs.sort_by_key(|(name, _)| *name);
                    pairs
                        .into_iter()
                        .map(|(k, _)| format!("{k}=*****"))
                        .collect::<Vec<_>>()
                        .join(", ")
                }
                _ => "-".to_string(),
            };
            println!(
                "{}",
                codex_tui::i18n::message(
                    "r39-cli-mcp-field-headers",
                    &[("value", &headers_display)]
                )
            );
            let env_headers_display = match env_http_headers {
                Some(map) if !map.is_empty() => {
                    let mut pairs: Vec<_> = map.iter().collect();
                    pairs.sort_by_key(|(name, _)| *name);
                    pairs
                        .into_iter()
                        .map(|(k, var)| format!("{k}={var}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                }
                _ => "-".to_string(),
            };
            println!(
                "{}",
                codex_tui::i18n::message(
                    "r39-cli-mcp-field-env-headers",
                    &[("value", &env_headers_display)]
                )
            );
            let helper_display = http_headers_helper.as_ref().map_or("-", |_| "<redacted>");
            println!(
                "{}",
                codex_tui::i18n::message(
                    "r39-cli-mcp-field-headers-helper",
                    &[("value", helper_display)]
                )
            );
        }
    }
    if let Some(timeout) = server.startup_timeout_sec {
        println!(
            "{}",
            codex_tui::i18n::message(
                "r39-cli-mcp-field-startup-timeout",
                &[("value", &timeout.as_secs_f64().to_string())]
            )
        );
    }
    if let Some(timeout) = server.tool_timeout_sec {
        println!(
            "{}",
            codex_tui::i18n::message(
                "r39-cli-mcp-field-tool-timeout",
                &[("value", &timeout.as_secs_f64().to_string())]
            )
        );
    }
    if let Some(approval_mode) = server.default_tools_approval_mode {
        let approval_mode = match approval_mode {
            AppToolApproval::Auto => "auto",
            AppToolApproval::Prompt => "prompt",
            AppToolApproval::Writes => "writes",
            AppToolApproval::Approve => "approve",
        };
        println!(
            "{}",
            codex_tui::i18n::message(
                "r39-cli-mcp-field-approval-mode",
                &[("value", approval_mode)]
            )
        );
    }
    println!(
        "{}",
        codex_tui::i18n::message("r39-cli-mcp-remove-command", &[("name", &get_args.name)])
    );

    Ok(())
}

fn parse_env_pair(raw: &str) -> Result<(String, String), String> {
    let mut parts = raw.splitn(2, '=');
    let key = parts
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            codex_tui::i18n::text("environment entries must be in KEY=VALUE form").to_string()
        })?;
    let value = parts.next().map(str::to_string).ok_or_else(|| {
        codex_tui::i18n::text("environment entries must be in KEY=VALUE form").to_string()
    })?;

    Ok((key.to_string(), value))
}

fn validate_server_name(name: &str) -> Result<()> {
    let is_valid = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '@' | '/' | '.'));

    if is_valid {
        Ok(())
    } else {
        bail!(
            "{}",
            codex_tui::i18n::message("r39-cli-mcp-invalid-server-name", &[("name", name)])
        );
    }
}

fn format_mcp_status(config: &McpServerConfig) -> String {
    if config.enabled {
        codex_tui::i18n::text("enabled").to_string()
    } else if let Some(reason) = config.disabled_reason.as_ref() {
        codex_tui::i18n::message(
            "r39-cli-mcp-disabled-state",
            &[("reason", &reason.to_string())],
        )
    } else {
        codex_tui::i18n::text("disabled").to_string()
    }
}
