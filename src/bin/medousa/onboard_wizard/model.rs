use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderChoice {
    Ollama,
    OpenAi,
    Custom,
}

impl ProviderChoice {
    pub(crate) fn from_provider_id(value: &str) -> Self {
        let normalized = value.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "ollama" => Self::Ollama,
            "openai" => Self::OpenAi,
            _ => Self::Custom,
        }
    }

    pub(crate) fn as_provider_id(self, custom_provider: &str) -> String {
        match self {
            Self::Ollama => "ollama".to_string(),
            Self::OpenAi => "openai".to_string(),
            Self::Custom => {
                let trimmed = custom_provider.trim();
                if trimmed.is_empty() {
                    "openai".to_string()
                } else {
                    trimmed.to_ascii_lowercase()
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BackendChoice {
    InMemory,
    SurrealMem,
    SurrealKv { path: String },
    SurrealWs { endpoint: String },
}

impl BackendChoice {
    pub(crate) fn from_backend_id(value: &str) -> Self {
        let trimmed = value.trim();
        if trimmed.eq_ignore_ascii_case("surreal-mem") {
            return Self::SurrealMem;
        }
        if let Some(path) = trimmed.strip_prefix("surreal-kv:") {
            let p = path.trim().to_string();
            if !p.is_empty() {
                return Self::SurrealKv { path: p };
            }
        }
        if let Some(endpoint) = trimmed.strip_prefix("surreal-ws:") {
            let e = endpoint.trim().to_string();
            if !e.is_empty() {
                return Self::SurrealWs { endpoint: e };
            }
        }
        Self::InMemory
    }

    pub(crate) fn as_backend_id(&self) -> String {
        match self {
            Self::InMemory => "in-memory".to_string(),
            Self::SurrealMem => "surreal-mem".to_string(),
            Self::SurrealKv { path } => format!("surreal-kv:{}", path),
            Self::SurrealWs { endpoint } => format!("surreal-ws:{}", endpoint),
        }
    }


}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WizardStep {
    Welcome,
    Provider,
    CustomProvider,
    Model,
    BaseUrl,
    ApiKey,
    Backend,
    BackendSurrealKvPath,
    BackendSurrealWsEndpoint,
    BackendSurrealWsUsername,
    BackendSurrealWsPassword,
    BackendSurrealNamespace,
    BackendSurrealDatabase,
    DaemonUrl,
    DaemonBind,
    LaunchDaemon,
    McpGateway,
    LaunchMcpGateway,
    LaunchChat,
    Discord,
    DiscordToken,
    DiscordPrefix,
    DiscordHeartbeat,
    LaunchDiscord,
    Telegram,
    TelegramToken,
    TelegramAllowUserIds,
    TelegramHeartbeat,
    LaunchTelegram,
    Slack,
    SlackBotToken,
    SlackAppToken,
    SlackAllowUserIds,
    LaunchSlack,
    WhatsApp,
    WhatsAppDeliverBind,
    WhatsAppAllowUserIds,
    LaunchWhatsApp,
    TuiResponseDepth,
    Confirm,
}

#[derive(Debug, Clone)]
pub(crate) struct WizardBootstrap {
    pub(crate) ollama_detected: bool,
    pub(crate) advanced_mode: bool,
    pub(crate) existing_api_key: bool,
    pub(crate) existing_discord_token: bool,
    pub(crate) existing_telegram_token: bool,
    pub(crate) existing_slack_bot_token: bool,
    pub(crate) existing_slack_app_token: bool,
    pub(crate) existing_mcp_gateway_config: bool,
    pub(crate) initial_telegram_allow_user_ids: Option<String>,
    pub(crate) initial_slack_allow_user_ids: String,
    pub(crate) initial_whatsapp_deliver_bind: String,
    pub(crate) initial_whatsapp_allow_user_ids: String,
    pub(crate) initial_daemon_bind: String,
    pub(crate) initial_discord_command_prefix: String,
    pub(crate) initial_discord_heartbeat_nudges: bool,
    pub(crate) initial_discord_heartbeat_channel_ids: String,
    pub(crate) initial_telegram_heartbeat_nudges: bool,
    pub(crate) initial_telegram_heartbeat_chat_ids: String,
    pub(crate) initial_tui_response_depth: String,
    pub(crate) initial_provider: String,
    pub(crate) initial_model: String,
    pub(crate) initial_base_url: Option<String>,
    pub(crate) initial_api_key: Option<String>,
    pub(crate) initial_backend: String,
    pub(crate) initial_daemon_url: String,
    pub(crate) default_openai_model: String,
    pub(crate) default_ollama_model: String,
    pub(crate) default_ollama_base_url: String,
    pub(crate) surreal_kv_default_path: String,
    pub(crate) initial_surreal_username: String,
    pub(crate) initial_surreal_password: String,
    pub(crate) initial_surreal_namespace: String,
    pub(crate) initial_surreal_database: String,
    pub(crate) default_surreal_namespace: String,
    pub(crate) default_surreal_database: String,
    pub(crate) force_daemon: bool,
    pub(crate) force_no_daemon: bool,
    pub(crate) force_tui: bool,
    pub(crate) force_no_tui: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct WizardOutput {
    pub(crate) provider: String,
    pub(crate) model: String,
    pub(crate) base_url: Option<String>,
    pub(crate) api_key: Option<String>,
    pub(crate) backend: String,
    pub(crate) daemon_url: String,
    pub(crate) start_daemon: bool,
    pub(crate) launch_tui: bool,
    pub(crate) configure_discord: bool,
    pub(crate) discord_token: Option<String>,
    pub(crate) start_discord: bool,
    pub(crate) configure_telegram: bool,
    pub(crate) telegram_token: Option<String>,
    pub(crate) telegram_allow_user_ids: Option<String>,
    pub(crate) start_telegram: bool,
    pub(crate) configure_slack: bool,
    pub(crate) slack_bot_token: Option<String>,
    pub(crate) slack_app_token: Option<String>,
    pub(crate) slack_allow_user_ids: Option<String>,
    pub(crate) start_slack: bool,
    pub(crate) configure_whatsapp: bool,
    pub(crate) whatsapp_deliver_bind: String,
    pub(crate) whatsapp_allow_user_ids: Option<String>,
    pub(crate) start_whatsapp: bool,
    pub(crate) configure_mcp_gateway: bool,
    pub(crate) start_mcp_gateway: bool,
    pub(crate) daemon_bind: String,
    pub(crate) discord_command_prefix: String,
    pub(crate) discord_heartbeat_nudges_enabled: bool,
    pub(crate) discord_heartbeat_channel_ids: Option<String>,
    pub(crate) telegram_heartbeat_nudges_enabled: bool,
    pub(crate) telegram_heartbeat_chat_ids: Option<String>,
    pub(crate) tui_response_depth_mode: String,
    pub(crate) surreal_endpoint: String,
    pub(crate) surreal_username: String,
    pub(crate) surreal_password: String,
    pub(crate) surreal_namespace: String,
    pub(crate) surreal_database: String,
}

pub(crate) enum WizardTransition {
    Continue,
    Finished(WizardOutput),
    Cancelled,
}

pub(crate) struct WizardState {
    pub(crate) bootstrap: WizardBootstrap,
    pub(crate) step: WizardStep,
    pub(crate) provider_choice: ProviderChoice,
    pub(crate) custom_provider: String,
    pub(crate) model: String,
    pub(crate) base_url: String,
    pub(crate) api_key: String,
    pub(crate) backend_choice: BackendChoice,
    pub(crate) backend_config_input: String,
    pub(crate) daemon_url: String,
    pub(crate) start_daemon: bool,
    pub(crate) launch_tui: bool,
    pub(crate) configure_discord: bool,
    pub(crate) discord_token: String,
    pub(crate) start_discord: bool,
    pub(crate) configure_telegram: bool,
    pub(crate) telegram_token: String,
    pub(crate) telegram_allow_user_ids: String,
    pub(crate) start_telegram: bool,
    pub(crate) configure_slack: bool,
    pub(crate) slack_bot_token: String,
    pub(crate) slack_app_token: String,
    pub(crate) slack_allow_user_ids: String,
    pub(crate) start_slack: bool,
    pub(crate) configure_whatsapp: bool,
    pub(crate) whatsapp_deliver_bind: String,
    pub(crate) whatsapp_allow_user_ids: String,
    pub(crate) start_whatsapp: bool,
    pub(crate) configure_mcp_gateway: bool,
    pub(crate) start_mcp_gateway: bool,
    pub(crate) daemon_bind: String,
    pub(crate) discord_command_prefix: String,
    pub(crate) discord_heartbeat_nudges_enabled: bool,
    pub(crate) discord_heartbeat_channel_ids: String,
    pub(crate) telegram_heartbeat_nudges_enabled: bool,
    pub(crate) telegram_heartbeat_chat_ids: String,
    pub(crate) tui_response_depth_mode: String,
    pub(crate) surreal_username: String,
    pub(crate) surreal_password: String,
    pub(crate) surreal_namespace: String,
    pub(crate) surreal_database: String,
    pub(crate) status_message: Option<String>,
}

impl WizardState {
    pub(crate) fn new(bootstrap: WizardBootstrap) -> Self {
        let provider_choice = ProviderChoice::from_provider_id(&bootstrap.initial_provider);
        let custom_provider = if provider_choice == ProviderChoice::Custom {
            bootstrap.initial_provider.trim().to_ascii_lowercase()
        } else {
            String::new()
        };
        let initial_telegram_allow_user_ids = bootstrap
            .initial_telegram_allow_user_ids
            .as_deref()
            .unwrap_or("")
            .trim()
            .to_string();
        let configure_telegram = bootstrap.existing_telegram_token
            || !initial_telegram_allow_user_ids.is_empty();
        let configure_slack = (bootstrap.existing_slack_bot_token
            && bootstrap.existing_slack_app_token)
            || !bootstrap.initial_slack_allow_user_ids.is_empty();
        let configure_whatsapp = !bootstrap.initial_whatsapp_allow_user_ids.is_empty();

        let mut start_daemon = true;
        if bootstrap.force_no_daemon {
            start_daemon = false;
        }
        if bootstrap.force_daemon {
            start_daemon = true;
        }

        let mut launch_tui = true;
        if bootstrap.force_no_tui {
            launch_tui = false;
        }
        if bootstrap.force_tui {
            launch_tui = true;
        }

        let backend_choice = BackendChoice::from_backend_id(&bootstrap.initial_backend);
        let backend_config_input = match &backend_choice {
            BackendChoice::SurrealKv { path } => path.clone(),
            BackendChoice::SurrealWs { endpoint } => endpoint.clone(),
            _ => String::new(),
        };

        let mut state = Self {
            model: if bootstrap.initial_model.trim().is_empty() {
                default_model_for_choice(&bootstrap, provider_choice)
            } else {
                bootstrap.initial_model.trim().to_string()
            },
            base_url: bootstrap
                .initial_base_url
                .as_deref()
                .unwrap_or("")
                .trim()
                .to_string(),
            api_key: bootstrap
                .initial_api_key
                .as_deref()
                .unwrap_or("")
                .trim()
                .to_string(),
            backend_choice,
            backend_config_input,
            daemon_url: bootstrap.initial_daemon_url.trim().to_string(),
            step: WizardStep::Welcome,
            provider_choice,
            custom_provider,
            start_daemon,
            launch_tui,
            configure_discord: bootstrap.existing_discord_token,
            discord_token: String::new(),
            start_discord: false,
            configure_telegram,
            telegram_token: String::new(),
            telegram_allow_user_ids: initial_telegram_allow_user_ids,
            start_telegram: false,
            configure_slack,
            slack_bot_token: String::new(),
            slack_app_token: String::new(),
            slack_allow_user_ids: bootstrap.initial_slack_allow_user_ids.clone(),
            start_slack: false,
            configure_whatsapp,
            whatsapp_deliver_bind: bootstrap.initial_whatsapp_deliver_bind.clone(),
            whatsapp_allow_user_ids: bootstrap.initial_whatsapp_allow_user_ids.clone(),
            start_whatsapp: false,
            configure_mcp_gateway: !bootstrap.existing_mcp_gateway_config,
            start_mcp_gateway: true,
            daemon_bind: bootstrap.initial_daemon_bind.clone(),
            discord_command_prefix: bootstrap.initial_discord_command_prefix.clone(),
            discord_heartbeat_nudges_enabled: bootstrap.initial_discord_heartbeat_nudges,
            discord_heartbeat_channel_ids: bootstrap.initial_discord_heartbeat_channel_ids.clone(),
            telegram_heartbeat_nudges_enabled: bootstrap.initial_telegram_heartbeat_nudges,
            telegram_heartbeat_chat_ids: bootstrap.initial_telegram_heartbeat_chat_ids.clone(),
            tui_response_depth_mode: bootstrap.initial_tui_response_depth.clone(),
            surreal_username: bootstrap.initial_surreal_username.clone(),
            surreal_password: bootstrap.initial_surreal_password.clone(),
            surreal_namespace: if bootstrap.initial_surreal_namespace.trim().is_empty() {
                bootstrap.default_surreal_namespace.clone()
            } else {
                bootstrap.initial_surreal_namespace.clone()
            },
            surreal_database: if bootstrap.initial_surreal_database.trim().is_empty() {
                bootstrap.default_surreal_database.clone()
            } else {
                bootstrap.initial_surreal_database.clone()
            },
            status_message: None,
            bootstrap,
        };

        state.apply_provider_defaults(false);
        state
    }

    pub(crate) fn step_title(&self) -> &'static str {
        match self.step {
            WizardStep::Welcome => "Welcome",
            WizardStep::Provider => "Choose Provider",
            WizardStep::CustomProvider => "Custom Provider",
            WizardStep::Model => "Model",
            WizardStep::BaseUrl => "Base URL",
            WizardStep::ApiKey => "API Key",
            WizardStep::Backend => "Storage Backend",
            WizardStep::BackendSurrealKvPath => "SurrealKV Path",
            WizardStep::BackendSurrealWsEndpoint => "SurrealWS Endpoint",
            WizardStep::BackendSurrealWsUsername => "SurrealDB Username",
            WizardStep::BackendSurrealWsPassword => "SurrealDB Password",
            WizardStep::BackendSurrealNamespace => "Surreal Namespace",
            WizardStep::BackendSurrealDatabase => "Surreal Database",
            WizardStep::DaemonUrl => "Runtime URL",
            WizardStep::DaemonBind => "Runtime Bind",
            WizardStep::LaunchDaemon => "Background Runtime",
            WizardStep::McpGateway => "MCP Gateway Config",
            WizardStep::LaunchMcpGateway => "Start MCP Gateway",
            WizardStep::LaunchChat => "Launch Chat",
            WizardStep::Discord => "Discord Adapter",
            WizardStep::DiscordToken => "Discord Token",
            WizardStep::DiscordPrefix => "Discord Prefix",
            WizardStep::DiscordHeartbeat => "Discord Heartbeat",
            WizardStep::LaunchDiscord => "Start Discord",
            WizardStep::Telegram => "Telegram Adapter",
            WizardStep::TelegramToken => "Telegram Token",
            WizardStep::TelegramAllowUserIds => "Telegram Allowlist",
            WizardStep::TelegramHeartbeat => "Telegram Heartbeat",
            WizardStep::LaunchTelegram => "Start Telegram",
            WizardStep::Slack => "Slack Adapter",
            WizardStep::SlackBotToken => "Slack Bot Token",
            WizardStep::SlackAppToken => "Slack App Token",
            WizardStep::SlackAllowUserIds => "Slack Allowlist",
            WizardStep::LaunchSlack => "Start Slack",
            WizardStep::WhatsApp => "WhatsApp Adapter",
            WizardStep::WhatsAppDeliverBind => "WhatsApp Deliver Bind",
            WizardStep::WhatsAppAllowUserIds => "WhatsApp Allowlist",
            WizardStep::LaunchWhatsApp => "Start WhatsApp",
            WizardStep::TuiResponseDepth => "Response Depth",
            WizardStep::Confirm => "Confirm",
        }
    }

    pub(crate) fn provider_id(&self) -> String {
        self.provider_choice.as_provider_id(&self.custom_provider)
    }

    pub(crate) fn flow_steps(&self) -> Vec<WizardStep> {
        let mut flow = vec![WizardStep::Welcome, WizardStep::Provider];

        if self.provider_choice == ProviderChoice::Custom {
            flow.push(WizardStep::CustomProvider);
        }

        flow.push(WizardStep::Model);

        if self.show_base_url_step() {
            flow.push(WizardStep::BaseUrl);
        }

        if self.show_api_key_step() {
            flow.push(WizardStep::ApiKey);
        }

        if self.bootstrap.advanced_mode {
            flow.push(WizardStep::Backend);
            match &self.backend_choice {
                BackendChoice::SurrealKv { .. } => {
                    flow.push(WizardStep::BackendSurrealKvPath);
                }
                BackendChoice::SurrealWs { .. } => {
                    flow.push(WizardStep::BackendSurrealWsEndpoint);
                    flow.push(WizardStep::BackendSurrealWsUsername);
                    flow.push(WizardStep::BackendSurrealWsPassword);
                    flow.push(WizardStep::BackendSurrealNamespace);
                    flow.push(WizardStep::BackendSurrealDatabase);
                }
                _ => {}
            }
            flow.push(WizardStep::DaemonUrl);
        }

        flow.push(WizardStep::DaemonBind);

        if !self.bootstrap.force_daemon && !self.bootstrap.force_no_daemon {
            flow.push(WizardStep::LaunchDaemon);
            flow.push(WizardStep::McpGateway);
            flow.push(WizardStep::LaunchMcpGateway);
        }

        if !self.bootstrap.force_tui && !self.bootstrap.force_no_tui {
            flow.push(WizardStep::LaunchChat);
        }

        flow.push(WizardStep::Discord);
        if self.configure_discord {
            flow.push(WizardStep::DiscordToken);
            flow.push(WizardStep::DiscordPrefix);
            flow.push(WizardStep::DiscordHeartbeat);
            flow.push(WizardStep::LaunchDiscord);
        }

        flow.push(WizardStep::Telegram);
        if self.configure_telegram {
            flow.push(WizardStep::TelegramToken);
            flow.push(WizardStep::TelegramAllowUserIds);
            flow.push(WizardStep::TelegramHeartbeat);
            flow.push(WizardStep::LaunchTelegram);
        }

        flow.push(WizardStep::Slack);
        if self.configure_slack {
            flow.push(WizardStep::SlackBotToken);
            flow.push(WizardStep::SlackAppToken);
            flow.push(WizardStep::SlackAllowUserIds);
            flow.push(WizardStep::LaunchSlack);
        }

        flow.push(WizardStep::WhatsApp);
        if self.configure_whatsapp {
            flow.push(WizardStep::WhatsAppDeliverBind);
            flow.push(WizardStep::WhatsAppAllowUserIds);
            flow.push(WizardStep::LaunchWhatsApp);
        }

        flow.push(WizardStep::TuiResponseDepth);
        flow.push(WizardStep::Confirm);
        flow
    }

    pub(crate) fn step_position(&self) -> (usize, usize) {
        let flow = self.flow_steps();
        let total = flow.len();
        let position = flow
            .iter()
            .position(|step| *step == self.step)
            .map(|idx| idx + 1)
            .unwrap_or(1);
        (position, total)
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> WizardTransition {
        self.status_message = None;

        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return WizardTransition::Cancelled;
        }

        match key.code {
            KeyCode::Esc => return WizardTransition::Cancelled,
            KeyCode::Left => {
                self.move_prev();
                return WizardTransition::Continue;
            }
            KeyCode::BackTab => {
                self.move_prev();
                return WizardTransition::Continue;
            }
            _ => {}
        }

        match self.step {
            WizardStep::Welcome => {
                if matches!(key.code, KeyCode::Enter | KeyCode::Right) {
                    self.move_next();
                }
            }
            WizardStep::Provider => match key.code {
                KeyCode::Up => self.cycle_provider(-1),
                KeyCode::Down => self.cycle_provider(1),
                KeyCode::Enter | KeyCode::Right => self.move_next(),
                _ => {}
            },
            WizardStep::CustomProvider => {
                if key.code == KeyCode::Enter {
                    if self.custom_provider.trim().is_empty() {
                        self.status_message = Some("Provider id cannot be empty.".to_string());
                    } else {
                        self.custom_provider = self.custom_provider.trim().to_ascii_lowercase();
                        self.move_next();
                    }
                } else {
                    edit_text_field(&mut self.custom_provider, key);
                }
            }
            WizardStep::Model => {
                if key.code == KeyCode::Enter {
                    if self.model.trim().is_empty() {
                        self.status_message = Some("Model cannot be empty.".to_string());
                    } else {
                        self.model = self.model.trim().to_string();
                        self.move_next();
                    }
                } else {
                    edit_text_field(&mut self.model, key);
                }
            }
            WizardStep::BaseUrl => {
                if key.code == KeyCode::Enter {
                    if self.provider_choice == ProviderChoice::Ollama
                        && self.base_url.trim().is_empty()
                    {
                        self.base_url = self.bootstrap.default_ollama_base_url.clone();
                    }
                    self.move_next();
                } else {
                    edit_text_field(&mut self.base_url, key);
                }
            }
            WizardStep::ApiKey => {
                if key.code == KeyCode::Enter {
                    self.api_key = self.api_key.trim().to_string();
                    self.move_next();
                } else {
                    edit_text_field(&mut self.api_key, key);
                }
            }
            WizardStep::Backend => match key.code {
                KeyCode::Up => self.cycle_backend(-1),
                KeyCode::Down => self.cycle_backend(1),
                KeyCode::Enter | KeyCode::Right => self.move_next(),
                _ => {}
            },
            WizardStep::BackendSurrealKvPath => {
                if key.code == KeyCode::Enter {
                    let path = self.backend_config_input.trim().to_string();
                    let default = self.bootstrap.surreal_kv_default_path.clone();
                    let final_path = if path.is_empty() { default } else { path };
                    self.backend_choice = BackendChoice::SurrealKv { path: final_path };
                    self.backend_config_input.clear();
                    self.move_next();
                } else {
                    edit_text_field(&mut self.backend_config_input, key);
                }
            }
            WizardStep::BackendSurrealWsEndpoint => {
                if key.code == KeyCode::Enter {
                    let endpoint = self.backend_config_input.trim().to_string();
                    if endpoint.is_empty() {
                        self.status_message = Some("Endpoint cannot be empty.".to_string());
                    } else {
                        self.backend_choice = BackendChoice::SurrealWs { endpoint };
                        self.backend_config_input.clear();
                        self.move_next();
                    }
                } else {
                    edit_text_field(&mut self.backend_config_input, key);
                }
            }
            WizardStep::BackendSurrealWsUsername => {
                if key.code == KeyCode::Enter {
                    self.surreal_username = self.surreal_username.trim().to_string();
                    self.move_next();
                } else {
                    edit_text_field(&mut self.surreal_username, key);
                }
            }
            WizardStep::BackendSurrealWsPassword => {
                if key.code == KeyCode::Enter {
                    self.surreal_password = self.surreal_password.trim().to_string();
                    self.move_next();
                } else {
                    edit_text_field(&mut self.surreal_password, key);
                }
            }
            WizardStep::BackendSurrealNamespace => {
                if key.code == KeyCode::Enter {
                    if self.surreal_namespace.trim().is_empty() {
                        self.surreal_namespace = self.bootstrap.default_surreal_namespace.clone();
                    } else {
                        self.surreal_namespace = self.surreal_namespace.trim().to_string();
                    }
                    self.move_next();
                } else {
                    edit_text_field(&mut self.surreal_namespace, key);
                }
            }
            WizardStep::BackendSurrealDatabase => {
                if key.code == KeyCode::Enter {
                    if self.surreal_database.trim().is_empty() {
                        self.surreal_database = self.bootstrap.default_surreal_database.clone();
                    } else {
                        self.surreal_database = self.surreal_database.trim().to_string();
                    }
                    self.move_next();
                } else {
                    edit_text_field(&mut self.surreal_database, key);
                }
            }
            WizardStep::DaemonUrl => {
                if key.code == KeyCode::Enter {
                    if self.daemon_url.trim().is_empty() {
                        self.daemon_url = self.bootstrap.initial_daemon_url.clone();
                    } else {
                        self.daemon_url = self.daemon_url.trim().to_string();
                    }
                    self.move_next();
                } else {
                    edit_text_field(&mut self.daemon_url, key);
                }
            }
            WizardStep::DaemonBind => {
                if key.code == KeyCode::Enter {
                    if self.daemon_bind.trim().is_empty() {
                        self.daemon_bind = self.bootstrap.initial_daemon_bind.clone();
                    } else {
                        self.daemon_bind = self.daemon_bind.trim().to_string();
                    }
                    self.move_next();
                } else {
                    edit_text_field(&mut self.daemon_bind, key);
                }
            }
            WizardStep::LaunchDaemon => match key.code {
                KeyCode::Enter | KeyCode::Right => self.move_next(),
                KeyCode::Char(' ') | KeyCode::Up | KeyCode::Down => {
                    self.start_daemon = !self.start_daemon;
                }
                _ => {}
            },
            WizardStep::McpGateway => match key.code {
                KeyCode::Enter | KeyCode::Right => self.move_next(),
                KeyCode::Char(' ') | KeyCode::Up | KeyCode::Down => {
                    self.configure_mcp_gateway = !self.configure_mcp_gateway;
                }
                _ => {}
            },
            WizardStep::LaunchMcpGateway => match key.code {
                KeyCode::Enter | KeyCode::Right => self.move_next(),
                KeyCode::Char(' ') | KeyCode::Up | KeyCode::Down => {
                    self.start_mcp_gateway = !self.start_mcp_gateway;
                }
                _ => {}
            },
            WizardStep::LaunchChat => match key.code {
                KeyCode::Enter | KeyCode::Right => self.move_next(),
                KeyCode::Char(' ') | KeyCode::Up | KeyCode::Down => {
                    self.launch_tui = !self.launch_tui;
                }
                _ => {}
            },
            WizardStep::Discord => match key.code {
                KeyCode::Enter | KeyCode::Right => self.move_next(),
                KeyCode::Char(' ') | KeyCode::Up | KeyCode::Down => {
                    self.configure_discord = !self.configure_discord;
                }
                _ => {}
            },
            WizardStep::DiscordToken => {
                if key.code == KeyCode::Enter {
                    self.discord_token = self.discord_token.trim().to_string();
                    if self.discord_token.is_empty() && !self.bootstrap.existing_discord_token {
                        self.status_message =
                            Some("Discord token is required to enable Discord setup.".to_string());
                    } else {
                        self.move_next();
                    }
                } else {
                    edit_text_field(&mut self.discord_token, key);
                }
            }
            WizardStep::DiscordPrefix => {
                if key.code == KeyCode::Enter {
                    if self.discord_command_prefix.trim().is_empty() {
                        self.discord_command_prefix =
                            self.bootstrap.initial_discord_command_prefix.clone();
                    } else {
                        self.discord_command_prefix = self.discord_command_prefix.trim().to_string();
                    }
                    self.move_next();
                } else {
                    edit_text_field(&mut self.discord_command_prefix, key);
                }
            }
            WizardStep::DiscordHeartbeat => match key.code {
                KeyCode::Enter | KeyCode::Right => self.move_next(),
                KeyCode::Char(' ') | KeyCode::Up | KeyCode::Down => {
                    self.discord_heartbeat_nudges_enabled = !self.discord_heartbeat_nudges_enabled;
                }
                KeyCode::Left => {}
                _ => edit_text_field(&mut self.discord_heartbeat_channel_ids, key),
            },
            WizardStep::LaunchDiscord => match key.code {
                KeyCode::Enter | KeyCode::Right => self.move_next(),
                KeyCode::Char(' ') | KeyCode::Up | KeyCode::Down => {
                    self.start_discord = !self.start_discord;
                }
                _ => {}
            },
            WizardStep::Telegram => match key.code {
                KeyCode::Enter | KeyCode::Right => self.move_next(),
                KeyCode::Char(' ') | KeyCode::Up | KeyCode::Down => {
                    self.configure_telegram = !self.configure_telegram;
                }
                _ => {}
            },
            WizardStep::TelegramToken => {
                if key.code == KeyCode::Enter {
                    self.telegram_token = self.telegram_token.trim().to_string();
                    if self.telegram_token.is_empty() && !self.bootstrap.existing_telegram_token {
                        self.status_message =
                            Some("Telegram token is required to enable Telegram setup.".to_string());
                    } else {
                        self.move_next();
                    }
                } else {
                    edit_text_field(&mut self.telegram_token, key);
                }
            }
            WizardStep::TelegramAllowUserIds => {
                if key.code == KeyCode::Enter {
                    match normalize_telegram_user_ids_csv(&self.telegram_allow_user_ids) {
                        Ok(normalized) => {
                            self.telegram_allow_user_ids = normalized;
                            self.move_next();
                        }
                        Err(message) => {
                            self.status_message = Some(message);
                        }
                    }
                } else {
                    edit_text_field(&mut self.telegram_allow_user_ids, key);
                }
            }
            WizardStep::TelegramHeartbeat => match key.code {
                KeyCode::Enter | KeyCode::Right => self.move_next(),
                KeyCode::Char(' ') | KeyCode::Up | KeyCode::Down => {
                    self.telegram_heartbeat_nudges_enabled = !self.telegram_heartbeat_nudges_enabled;
                }
                KeyCode::Left => {}
                _ => edit_text_field(&mut self.telegram_heartbeat_chat_ids, key),
            },
            WizardStep::LaunchTelegram => match key.code {
                KeyCode::Enter | KeyCode::Right => self.move_next(),
                KeyCode::Char(' ') | KeyCode::Up | KeyCode::Down => {
                    self.start_telegram = !self.start_telegram;
                }
                _ => {}
            },
            WizardStep::Slack => match key.code {
                KeyCode::Enter | KeyCode::Right => self.move_next(),
                KeyCode::Char(' ') | KeyCode::Up | KeyCode::Down => {
                    self.configure_slack = !self.configure_slack;
                }
                _ => {}
            },
            WizardStep::SlackBotToken => {
                if key.code == KeyCode::Enter {
                    self.slack_bot_token = self.slack_bot_token.trim().to_string();
                    if self.slack_bot_token.is_empty() && !self.bootstrap.existing_slack_bot_token {
                        self.status_message =
                            Some("Slack bot token (xoxb-…) is required when Slack is enabled.".to_string());
                    } else {
                        self.move_next();
                    }
                } else {
                    edit_text_field(&mut self.slack_bot_token, key);
                }
            }
            WizardStep::SlackAppToken => {
                if key.code == KeyCode::Enter {
                    self.slack_app_token = self.slack_app_token.trim().to_string();
                    if self.slack_app_token.is_empty() && !self.bootstrap.existing_slack_app_token {
                        self.status_message = Some(
                            "Slack app token (xapp-… Socket Mode) is required when Slack is enabled."
                                .to_string(),
                        );
                    } else {
                        self.move_next();
                    }
                } else {
                    edit_text_field(&mut self.slack_app_token, key);
                }
            }
            WizardStep::SlackAllowUserIds => {
                if key.code == KeyCode::Enter {
                    self.slack_allow_user_ids = normalize_slack_user_ids_csv(&self.slack_allow_user_ids);
                    self.move_next();
                } else {
                    edit_text_field(&mut self.slack_allow_user_ids, key);
                }
            }
            WizardStep::LaunchSlack => match key.code {
                KeyCode::Enter | KeyCode::Right => self.move_next(),
                KeyCode::Char(' ') | KeyCode::Up | KeyCode::Down => {
                    self.start_slack = !self.start_slack;
                }
                _ => {}
            },
            WizardStep::WhatsApp => match key.code {
                KeyCode::Enter | KeyCode::Right => self.move_next(),
                KeyCode::Char(' ') | KeyCode::Up | KeyCode::Down => {
                    self.configure_whatsapp = !self.configure_whatsapp;
                }
                _ => {}
            },
            WizardStep::WhatsAppDeliverBind => {
                if key.code == KeyCode::Enter {
                    if self.whatsapp_deliver_bind.trim().is_empty() {
                        self.whatsapp_deliver_bind =
                            self.bootstrap.initial_whatsapp_deliver_bind.clone();
                    } else {
                        self.whatsapp_deliver_bind = self.whatsapp_deliver_bind.trim().to_string();
                    }
                    self.move_next();
                } else {
                    edit_text_field(&mut self.whatsapp_deliver_bind, key);
                }
            }
            WizardStep::WhatsAppAllowUserIds => {
                if key.code == KeyCode::Enter {
                    self.whatsapp_allow_user_ids =
                        normalize_whatsapp_allowlist_csv(&self.whatsapp_allow_user_ids);
                    self.move_next();
                } else {
                    edit_text_field(&mut self.whatsapp_allow_user_ids, key);
                }
            }
            WizardStep::LaunchWhatsApp => match key.code {
                KeyCode::Enter | KeyCode::Right => self.move_next(),
                KeyCode::Char(' ') | KeyCode::Up | KeyCode::Down => {
                    self.start_whatsapp = !self.start_whatsapp;
                }
                _ => {}
            },
            WizardStep::TuiResponseDepth => match key.code {
                KeyCode::Up => self.cycle_response_depth(-1),
                KeyCode::Down => self.cycle_response_depth(1),
                KeyCode::Enter | KeyCode::Right => self.move_next(),
                _ => {}
            },
            WizardStep::Confirm => {
                if matches!(key.code, KeyCode::Enter | KeyCode::Right) {
                    return WizardTransition::Finished(self.build_output());
                }
            }
        }

        WizardTransition::Continue
    }

    fn show_base_url_step(&self) -> bool {
        true
    }

    fn show_api_key_step(&self) -> bool {
        if self.provider_choice == ProviderChoice::Ollama {
            return false;
        }

        if self.bootstrap.existing_api_key
            && !self.bootstrap.advanced_mode
            && self.bootstrap.initial_api_key.is_none()
        {
            return false;
        }

        true
    }

    fn cycle_provider(&mut self, delta: i32) {
        let provider_choices = [
            ProviderChoice::Ollama,
            ProviderChoice::OpenAi,
            ProviderChoice::Custom,
        ];
        let current_idx = provider_choices
            .iter()
            .position(|choice| *choice == self.provider_choice)
            .unwrap_or(0) as i32;
        let next_idx = (current_idx + delta).rem_euclid(provider_choices.len() as i32) as usize;
        self.provider_choice = provider_choices[next_idx];
        self.apply_provider_defaults(true);
    }

    fn cycle_backend(&mut self, delta: i32) {
        let variants: Vec<BackendChoice> = vec![
            BackendChoice::InMemory,
            BackendChoice::SurrealMem,
            BackendChoice::SurrealKv {
                path: self.bootstrap.surreal_kv_default_path.clone(),
            },
            BackendChoice::SurrealWs {
                endpoint: String::new(),
            },
        ];
        let current_idx = variants
            .iter()
            .position(|v| {
                std::mem::discriminant(v) == std::mem::discriminant(&self.backend_choice)
            })
            .unwrap_or(0) as i32;
        let next_idx = (current_idx + delta).rem_euclid(variants.len() as i32) as usize;
        // Transfer existing config if same variant
        let next_variant = &variants[next_idx];
        self.backend_choice = match (&self.backend_choice, next_variant) {
            (BackendChoice::SurrealKv { path }, BackendChoice::SurrealKv { .. }) => {
                BackendChoice::SurrealKv { path: path.clone() }
            }
            (BackendChoice::SurrealWs { endpoint }, BackendChoice::SurrealWs { .. }) => {
                BackendChoice::SurrealWs {
                    endpoint: endpoint.clone(),
                }
            }
            _ => next_variant.clone(),
        };
    }

    fn apply_provider_defaults(&mut self, reset_model: bool) {
        if reset_model {
            self.model = default_model_for_choice(&self.bootstrap, self.provider_choice);
        }

        if self.provider_choice == ProviderChoice::Ollama {
            if self.base_url.trim().is_empty() {
                self.base_url = self.bootstrap.default_ollama_base_url.clone();
            }
        } else if self.base_url.trim() == self.bootstrap.default_ollama_base_url.trim() {
            self.base_url.clear();
        }

        if self.provider_choice != ProviderChoice::Custom {
            self.custom_provider.clear();
        }
    }

    fn move_next(&mut self) {
        let flow = self.flow_steps();
        if flow.is_empty() {
            return;
        }

        if let Some(position) = flow.iter().position(|step| *step == self.step) {
            let next_position = (position + 1).min(flow.len() - 1);
            self.step = flow[next_position];
        } else {
            self.step = flow[0];
        }
    }

    fn move_prev(&mut self) {
        let flow = self.flow_steps();
        if flow.is_empty() {
            return;
        }

        if let Some(position) = flow.iter().position(|step| *step == self.step) {
            let prev_position = position.saturating_sub(1);
            self.step = flow[prev_position];
        } else {
            self.step = flow[0];
        }
    }

    fn build_output(&self) -> WizardOutput {
        let provider = self.provider_id();

        let model = if self.model.trim().is_empty() {
            default_model_for_choice(&self.bootstrap, self.provider_choice)
        } else {
            self.model.trim().to_string()
        };

        let base_url = if self.provider_choice == ProviderChoice::Ollama {
            let trimmed = self.base_url.trim();
            if trimmed.is_empty() {
                Some(self.bootstrap.default_ollama_base_url.clone())
            } else {
                Some(trimmed.to_string())
            }
        } else {
            let trimmed = self.base_url.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        };

        let api_key = if self.provider_choice == ProviderChoice::Ollama {
            None
        } else {
            let trimmed = self.api_key.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        };

        let daemon_url = if self.daemon_url.trim().is_empty() {
            self.bootstrap.initial_daemon_url.clone()
        } else {
            self.daemon_url.trim().to_string()
        };

        let mut start_daemon = self.start_daemon;
        if self.bootstrap.force_no_daemon {
            start_daemon = false;
        }
        if self.bootstrap.force_daemon {
            start_daemon = true;
        }

        let mut launch_tui = self.launch_tui;
        if self.bootstrap.force_no_tui {
            launch_tui = false;
        }
        if self.bootstrap.force_tui {
            launch_tui = true;
        }

        if launch_tui && !start_daemon && !self.bootstrap.force_no_daemon {
            start_daemon = true;
        }

        let configure_discord = self.configure_discord;
        let discord_token = if self.discord_token.trim().is_empty() {
            None
        } else {
            Some(self.discord_token.trim().to_string())
        };
        let start_discord = configure_discord && self.start_discord;

        let configure_telegram = self.configure_telegram;
        let telegram_token = if self.telegram_token.trim().is_empty() {
            None
        } else {
            Some(self.telegram_token.trim().to_string())
        };
        let telegram_allow_user_ids = if self.telegram_allow_user_ids.trim().is_empty() {
            None
        } else {
            Some(self.telegram_allow_user_ids.trim().to_string())
        };
        let start_telegram = configure_telegram && self.start_telegram;
        let configure_slack = self.configure_slack;
        let slack_bot_token = if self.slack_bot_token.trim().is_empty() {
            None
        } else {
            Some(self.slack_bot_token.trim().to_string())
        };
        let slack_app_token = if self.slack_app_token.trim().is_empty() {
            None
        } else {
            Some(self.slack_app_token.trim().to_string())
        };
        let slack_allow_user_ids = if self.slack_allow_user_ids.trim().is_empty() {
            None
        } else {
            Some(self.slack_allow_user_ids.trim().to_string())
        };
        let start_slack = configure_slack && self.start_slack;
        let configure_whatsapp = self.configure_whatsapp;
        let whatsapp_allow_user_ids = if self.whatsapp_allow_user_ids.trim().is_empty() {
            None
        } else {
            Some(self.whatsapp_allow_user_ids.trim().to_string())
        };
        let start_whatsapp = configure_whatsapp && self.start_whatsapp;
        let configure_mcp_gateway = self.configure_mcp_gateway;
        let start_mcp_gateway = self.start_mcp_gateway;

        if (start_discord
            || start_telegram
            || start_slack
            || start_whatsapp
            || start_mcp_gateway)
            && !start_daemon
            && !self.bootstrap.force_no_daemon
        {
            start_daemon = true;
        }

        WizardOutput {
            provider,
            model,
            base_url,
            api_key,
            backend: self.backend_choice.as_backend_id(),
            daemon_url,
            daemon_bind: if self.daemon_bind.trim().is_empty() {
                self.bootstrap.initial_daemon_bind.clone()
            } else {
                self.daemon_bind.trim().to_string()
            },
            start_daemon,
            launch_tui,
            configure_discord,
            discord_token,
            discord_command_prefix: if self.discord_command_prefix.trim().is_empty() {
                self.bootstrap.initial_discord_command_prefix.clone()
            } else {
                self.discord_command_prefix.trim().to_string()
            },
            discord_heartbeat_nudges_enabled: self.discord_heartbeat_nudges_enabled,
            discord_heartbeat_channel_ids: if self.discord_heartbeat_channel_ids.trim().is_empty()
            {
                None
            } else {
                Some(self.discord_heartbeat_channel_ids.trim().to_string())
            },
            start_discord,
            configure_telegram,
            telegram_token,
            telegram_allow_user_ids,
            telegram_heartbeat_nudges_enabled: self.telegram_heartbeat_nudges_enabled,
            telegram_heartbeat_chat_ids: if self.telegram_heartbeat_chat_ids.trim().is_empty() {
                None
            } else {
                Some(self.telegram_heartbeat_chat_ids.trim().to_string())
            },
            start_telegram,
            configure_slack,
            slack_bot_token,
            slack_app_token,
            slack_allow_user_ids,
            start_slack,
            configure_whatsapp,
            whatsapp_deliver_bind: if self.whatsapp_deliver_bind.trim().is_empty() {
                self.bootstrap.initial_whatsapp_deliver_bind.clone()
            } else {
                self.whatsapp_deliver_bind.trim().to_string()
            },
            whatsapp_allow_user_ids,
            start_whatsapp,
            configure_mcp_gateway,
            start_mcp_gateway,
            tui_response_depth_mode: self.tui_response_depth_mode.clone(),
            surreal_endpoint: match &self.backend_choice {
                BackendChoice::SurrealWs { endpoint } => endpoint.clone(),
                _ => String::new(),
            },
            surreal_username: self.surreal_username.trim().to_string(),
            surreal_password: self.surreal_password.trim().to_string(),
            surreal_namespace: if self.surreal_namespace.trim().is_empty() {
                self.bootstrap.default_surreal_namespace.clone()
            } else {
                self.surreal_namespace.trim().to_string()
            },
            surreal_database: if self.surreal_database.trim().is_empty() {
                self.bootstrap.default_surreal_database.clone()
            } else {
                self.surreal_database.trim().to_string()
            },
        }
    }

    fn cycle_response_depth(&mut self, delta: i32) {
        let modes = ["concise", "standard", "deep"];
        let current_idx = modes
            .iter()
            .position(|mode| *mode == self.tui_response_depth_mode.as_str())
            .unwrap_or(1) as i32;
        let next_idx = (current_idx + delta).rem_euclid(modes.len() as i32) as usize;
        self.tui_response_depth_mode = modes[next_idx].to_string();
    }
}

fn default_model_for_choice(bootstrap: &WizardBootstrap, provider_choice: ProviderChoice) -> String {
    if provider_choice == ProviderChoice::Ollama {
        bootstrap.default_ollama_model.clone()
    } else {
        bootstrap.default_openai_model.clone()
    }
}

fn normalize_whatsapp_allowlist_csv(raw: &str) -> String {
    raw.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(",")
}

fn normalize_slack_user_ids_csv(raw: &str) -> String {
    raw.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(",")
}

fn normalize_telegram_user_ids_csv(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }

    let mut ids = Vec::new();
    for token in trimmed.split(',') {
        let part = token.trim();
        if part.is_empty() {
            continue;
        }

        let parsed = part.parse::<u64>().map_err(|_| {
            format!(
                "Invalid Telegram user id '{}'. Use comma-separated numeric ids.",
                part
            )
        })?;
        ids.push(parsed);
    }

    ids.sort_unstable();
    ids.dedup();
    Ok(ids
        .into_iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(","))
}

fn edit_text_field(buffer: &mut String, key: KeyEvent) {
    match key.code {
        KeyCode::Char(ch)
            if !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            buffer.push(ch);
        }
        KeyCode::Backspace => {
            buffer.pop();
        }
        KeyCode::Delete => {
            buffer.clear();
        }
        _ => {}
    }
}
