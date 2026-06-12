use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Result;
use chrono::Utc;
use crossterm::{
    event::{Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal, TerminalOptions, Viewport,
    backend::CrosstermBackend,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use serde_json::Value;
use tokio::sync::mpsc;
use uuid::Uuid;

use medousa::{
    TuiPlatformBuildConfig, TuiPlatformMode, TuiRuntime, build_tui_platform,
    events::TuiEvent,
    resolve_daemon_url, resolve_llm_base_url, resolve_llm_model,
    resolve_llm_provider, resolve_tui_platform_mode,
    session::{
        ApiKeyStorageBackend, ConversationTurn, SessionHistorySummary,
        detect_tui_api_key_storage_backend, load_history, load_last_session_id, load_tui_api_key,
        load_tui_defaults, save_last_session_id, save_tui_api_key, save_tui_defaults,
    },
    settings_guard::{invalid_module_ids, parse_allowed_modules},
    stage_routing::StageRoutingMatrix,
    tui::allowlist_preview::analyze_allowlist_preview,
    tui::editor_buffer::TextBuffer,
    tui::settings::{
        RuntimeSettings, cycle_backend, cycle_host_turn_bus_mode, cycle_tool_call_mode,
        cycle_web_search_provider,
        env_overrides_validation_errors,
        parse_bool_with_default, parse_env_overrides, parse_f32_with_bounds,
        parse_usize_with_bounds, resolve_backend_name, resolve_bool_arg, resolve_f32_arg,
        resolve_theme_id_name, resolve_tool_call_mode_name, resolve_usize_arg,
        settings_validation_errors,
    },
};
#[path = "medousa_tui/tui_stderr_guard.rs"]
mod tui_stderr_guard;
#[path = "medousa_tui/budget_slash_services.rs"]
mod budget_slash_services;
#[path = "medousa_tui/cli_helpers.rs"]
mod cli_helpers;
#[path = "medousa_tui/command_preview_ui.rs"]
mod command_preview_ui;
#[path = "medousa_tui/daemon_commands.rs"]
mod daemon_commands;
#[path = "medousa_tui/history_services.rs"]
mod history_services;
#[path = "medousa_tui/session_name_services.rs"]
mod session_name_services;
#[path = "medousa_tui/editor_runtime.rs"]
mod editor_runtime;
#[path = "medousa_tui/event_reducer.rs"]
mod event_reducer;
#[path = "medousa_tui/input_router.rs"]
mod input_router;
#[path = "medousa_tui/markdown_cache.rs"]
mod markdown_cache;
#[path = "medousa_tui/perf.rs"]
mod perf;
#[path = "medousa_tui/settings_runtime.rs"]
mod settings_runtime;
#[path = "medousa_tui/settings_rows.rs"]
mod settings_rows;
#[path = "medousa_tui/settings_ui.rs"]
mod settings_ui;
#[path = "medousa_tui/slash_command_services.rs"]
mod slash_command_services;
#[path = "medousa_tui/slash_command_artifact_services.rs"]
mod slash_command_artifact_services;
#[path = "medousa_tui/slash_command_stage_services.rs"]
mod slash_command_stage_services;
#[path = "medousa_tui/slash_commands.rs"]
mod slash_commands;
#[path = "medousa_tui/theme_ui.rs"]
mod theme_ui;
#[path = "medousa_tui/tui_presentation.rs"]
mod tui_presentation;
#[path = "medousa_tui/ui_helpers.rs"]
mod ui_helpers;
#[path = "medousa_tui/ui_render.rs"]
mod ui_render;
#[path = "medousa_tui/workers.rs"]
mod workers;

use agent_runtime::{start_prompt_run, stop_active_generation};
use cli_helpers::{find_arg_value, print_help};
use editor_runtime::{load_editor_file, run_editor_source_via_runtime, save_editor_buffer};
#[cfg(test)]
use editor_runtime::{resolve_editor_run_source, validate_editor_run_allowlist, write_editor_file};
use event_reducer::{flush_pending_agent_chunks, handle_tui_event};
use input_router::{handle_key_event, keep_editor_cursor_visible};
use markdown_cache::invalidate_markdown_cache;
use perf::{
    PerfSnapshot, UiPerfStats, capture_perf_snapshot, format_perf_delta, format_perf_snapshot,
    mark_ui_activity, note_frame_rendered,
};
use settings_runtime::{
    apply_env_overrides, apply_settings, finalize_settings_apply_if_ready,
    handle_runtime_env_key_event, handle_settings_key_event, next_ui_wake_delay,
};
use slash_commands::handle_slash_command;
use ui_helpers::{
    centered_rect, set_active_ui_theme, ui_accent_primary, ui_accent_warn, ui_bg, ui_border,
    ui_modal_bg, ui_panel_bg, ui_theme_display_name, ui_theme_ids,
};
use ui_render::render;
use workers::{
    WorkerCommand, WorkerResult, handle_worker_result, next_worker_request_id,
    queue_worker_command, worker_loop,
};

#[derive(Debug, Clone)]
struct ObsEvent {
    text: String,
}

#[derive(Debug, Clone)]
struct JobHistoryEntry {
    job_id: String,
    job_type: String,
    status: String,
}

struct TuiState {
    conversation: Vec<ConversationTurn>,
    observability: VecDeque<ObsEvent>,
    observability_filter: ObservabilityFilter,
    job_history: VecDeque<JobHistoryEntry>,
    input_buffer: String,
    conv_scroll: u16,
    conv_max_scroll: u16,
    is_processing: bool,
    active_request_task: Option<tokio::task::JoinHandle<()>>,
    auto_scroll: bool,
    active_agent_turn_id: u64,
    open_stream_turn_id: Option<u64>,
    active_agent_stream_turn: Option<usize>,
    mode: UiMode,
    startup_selected: usize,
    history_items: Vec<SessionHistorySummary>,
    history_selected: usize,
    history_scroll: u16,
    history_max_scroll: u16,
    history_show_verification_detail: bool,
    command_query: String,
    command_tab: usize,
    command_selected: usize,
    command_scroll: u16,
    command_max_scroll: u16,
    command_usage_counts: HashMap<String, u64>,
    settings: RuntimeSettings,
    settings_draft: RuntimeSettings,
    allowlist_preview_source: String,
    editor_buffer: TextBuffer,
    editor_file_path: Option<PathBuf>,
    editor_status: String,
    editor_dirty: bool,
    editor_preferred_col: Option<usize>,
    editor_scroll: u16,
    settings_tab: usize,
    settings_selected: usize,
    settings_editing: bool,
    settings_scroll: u16,
    settings_max_scroll: u16,
    theme_menu_selected: usize,
    theme_menu_scroll: u16,
    theme_menu_max_scroll: u16,
    theme_menu_return_mode: UiMode,
    theme_menu_original_theme_id: String,
    theme_menu_original_draft_theme_id: String,
    routing_editor_role_idx: usize,
    runtime_env_editing: bool,
    provider_model: String,
    response_depth_mode: String,
    session_id: String,
    session_display_name: Option<String>,
    selected_context_pack_query: Option<String>,
    stage_routing: StageRoutingMatrix,
    stage_routing_draft: StageRoutingMatrix,
    thinking_trace: VecDeque<String>,
    pending_thinking_buffer: String,
    thinking_scroll: u16,
    thinking_max_scroll: u16,
    grapheme_console: VecDeque<String>,
    grapheme_console_scroll: u16,
    grapheme_console_max_scroll: u16,
    obs_scroll: u16,
    obs_max_scroll: u16,
    job_scroll: u16,
    job_max_scroll: u16,
    in_thinking_tag: bool,
    stream_tag_tail: String,
    received_native_reasoning: bool,
    pending_response_verified: Option<bool>,
    daemon_url: String,
    /// When true, chat turns skip daemon and use the in-process local runtime only.
    local_runtime_only: bool,
    next_settings_apply_request_id: u64,
    active_settings_apply_request_id: Option<u64>,
    pending_settings_apply: Option<PendingSettingsApply>,
    ui_dirty: bool,
    pending_agent_chunk_delta: String,
    pending_agent_chunk_count: u64,
    turn_parts: medousa::turn_parts::TurnPartsAccumulator,
    pending_paint_since: Option<Instant>,
    perf: UiPerfStats,
    worker_cmd_tx: mpsc::Sender<WorkerCommand>,
    next_worker_request_id: u64,
    latest_daemon_health_request_id: u64,
    latest_daemon_ask_request_id: u64,
    latest_watch_add_request_id: u64,
    pending_budget_request_id: Option<String>,
    pending_budget_requested_rounds: Option<usize>,
    markdown_cache: RefCell<HashMap<MarkdownCacheKey, Vec<Line<'static>>>>,
    markdown_cache_order: RefCell<VecDeque<MarkdownCacheKey>>,
    perf_baseline: Option<PerfSnapshot>,
}

pub(crate) fn build_tui_platform_config(state: &TuiState) -> TuiPlatformBuildConfig {
    TuiPlatformBuildConfig::from_names(
        state.settings.backend.trim(),
        Some(state.settings.provider.as_str()),
        Some(state.settings.model.as_str()),
        if state.settings.base_url.trim().is_empty() {
            None
        } else {
            Some(state.settings.base_url.as_str())
        },
        parse_allowed_modules(&state.settings.allowed_modules),
        &state.session_id,
        &state.daemon_url,
        state.local_runtime_only,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ObservabilityFilter {
    All,
    ReceiptsOnly,
    ArtifactsOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct MarkdownCacheKey {
    width: u16,
    content_hash: u64,
}

struct SettingsApplySnapshot {
    backend: String,
    theme_id: String,
    provider: String,
    model: String,
    base_url: Option<String>,
    env_overrides_raw: String,
    allowed_modules: Vec<String>,
    tool_call_mode: String,
    max_tool_rounds: usize,
    thinking_capture: bool,
    stasis_otel_enabled: bool,
    thinking_max_lines: usize,
    activation_direct_answer_max_prompt_chars: usize,
    activation_long_session_turn_threshold: usize,
    activation_long_session_max_prompt_chars: usize,
    slice_hot_window_turns: usize,
    slice_cold_window_turns: usize,
    retry_runtime_max_retries: usize,
    retry_runtime_max_rounds: usize,
    host_bus_max_tool_rounds: usize,
    host_turn_bus_mode: String,
    activation_tool_intent_max_rounds: usize,
    activation_short_turn_max_tool_rounds: usize,
    continuation_max_tool_rounds: usize,
    max_text_only_stuck_continues: usize,
    classifier_restricted_max_tool_rounds: usize,
    verifier_min_citation_coverage: f32,
    verifier_min_avg_support_strength: f32,
    verifier_min_supported_claim_ratio: f32,
    verifier_min_claim_support_strength: f32,
    web_search_preferred_provider: String,
    web_search_try_fallbacks: bool,
    stage_routing: StageRoutingMatrix,
    api_key: String,
}

struct PendingSettingsApply {
    request_id: u64,
    changed_env_count: usize,
    snapshot: SettingsApplySnapshot,
    handle: tokio::task::JoinHandle<std::result::Result<TuiRuntime, String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UiMode {
    Startup,
    Chat,
    History,
    CommandPalette,
    Settings,
    RuntimeEnv,
    ThemeMenu,
    ObservabilityPanel,
    AllowlistPreview,
    Editor,
    ThinkingPeek,
    ThinkingPanel,
    GraphemeConsole,
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    let _stderr_guard = tui_stderr_guard::TuiStderrGuard::attach().ok();
    let _stderr_guard = tui_stderr_guard::attach().ok();
    let args: Vec<String> = std::env::args().skip(1).collect();

    let provider = find_arg_value(&args, "--provider");
    let model = find_arg_value(&args, "--model");
    let base_url = find_arg_value(&args, "--base-url");
    let backend = find_arg_value(&args, "--backend");
    let tool_call_mode = find_arg_value(&args, "--tool-call-mode");
    let max_tool_rounds = find_arg_value(&args, "--max-tool-rounds");
    let thinking_capture = find_arg_value(&args, "--thinking-capture");
    let thinking_max_lines = find_arg_value(&args, "--thinking-max-lines");
    let daemon_url = find_arg_value(&args, "--daemon-url");
    let explicit_session = find_arg_value(&args, "--session");
    let defaults = load_tui_defaults();

    medousa::apply_workshop_llm_env();
    medousa::runtime::stasis_otel::apply_stasis_otel_from_defaults(&defaults);

    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return Ok(());
    }

    let local_runtime_only = args.iter().any(|a| a == "--local-runtime-only")
        || std::env::var("MEDOUSA_TUI_LOCAL_RUNTIME")
            .ok()
            .is_some_and(|value| {
                value == "1" || value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("yes")
            });

    let resolved_provider = resolve_llm_provider(provider.or(defaults.provider.as_deref()));
    let resolved_model = resolve_llm_model(model.or(defaults.model.as_deref()));
    let mut resolved_backend = resolve_backend_name(backend.or(defaults.backend.as_deref()));
    let resolved_theme_id = resolve_theme_id_name(defaults.theme_id.as_deref());
    let resolved_tool_call_mode =
        resolve_tool_call_mode_name(tool_call_mode.or(defaults.tool_call_mode.as_deref()));
    let resolved_max_tool_rounds = resolve_usize_arg(
        max_tool_rounds,
        defaults.max_tool_rounds.unwrap_or(10),
        medousa::agent_runtime::ROUND_LIMIT_MIN,
        medousa::agent_runtime::ROUND_LIMIT_MAX,
    );
    let resolved_thinking_capture =
        resolve_bool_arg(thinking_capture, defaults.thinking_capture.unwrap_or(true));
    let resolved_stasis_otel_enabled =
        resolve_bool_arg(None, defaults.stasis_otel_enabled.unwrap_or(false));
    let resolved_thinking_max_lines = resolve_usize_arg(
        thinking_max_lines,
        defaults.thinking_max_lines.unwrap_or(300),
        50,
        5000,
    );
    let resolved_activation_direct_answer_max_prompt_chars = resolve_usize_arg(
        None,
        defaults
            .activation_direct_answer_max_prompt_chars
            .unwrap_or(320),
        64,
        4000,
    );
    let resolved_activation_long_session_turn_threshold = resolve_usize_arg(
        None,
        defaults
            .activation_long_session_turn_threshold
            .unwrap_or(28),
        8,
        500,
    );
    let resolved_activation_long_session_max_prompt_chars = resolve_usize_arg(
        None,
        defaults
            .activation_long_session_max_prompt_chars
            .unwrap_or(420),
        64,
        4000,
    );
    let resolved_slice_hot_window_turns =
        resolve_usize_arg(None, defaults.slice_hot_window_turns.unwrap_or(8), 2, 32);
    let resolved_slice_cold_window_turns =
        resolve_usize_arg(None, defaults.slice_cold_window_turns.unwrap_or(24), 4, 128)
            .max(resolved_slice_hot_window_turns);
    let resolved_retry_runtime_max_retries = resolve_usize_arg(
        None,
        defaults.retry_runtime_max_retries.unwrap_or(1),
        medousa::agent_runtime::RETRY_LIMIT_MIN,
        medousa::agent_runtime::RETRY_LIMIT_MAX,
    );
    let resolved_retry_runtime_max_rounds = resolve_usize_arg(
        None,
        defaults
            .retry_runtime_max_rounds
            .unwrap_or(medousa::agent_runtime::turn_orchestrator::DEFAULT_RETRY_RUNTIME_MAX_ROUNDS),
        medousa::agent_runtime::ROUND_LIMIT_MIN,
        medousa::agent_runtime::ROUND_LIMIT_MAX,
    );
    let resolved_host_bus_max_tool_rounds = resolve_usize_arg(
        None,
        defaults
            .host_bus_max_tool_rounds
            .unwrap_or(medousa::agent_runtime::DEFAULT_HOST_BUS_MAX_TOOL_ROUNDS),
        medousa::agent_runtime::ROUND_LIMIT_MIN,
        medousa::agent_runtime::ROUND_LIMIT_MAX,
    );
    let resolved_host_turn_bus_mode = defaults
        .host_turn_bus_mode
        .clone()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| {
            medousa::agent_runtime::default_host_turn_bus_mode_label().to_string()
        });
    let resolved_activation_tool_intent_max_rounds = resolve_usize_arg(
        None,
        defaults
            .activation_tool_intent_max_rounds
            .unwrap_or(medousa::agent_runtime::DEFAULT_ACTIVATION_TOOL_INTENT_MAX_ROUNDS),
        medousa::agent_runtime::ROUND_LIMIT_MIN,
        medousa::agent_runtime::ROUND_LIMIT_MAX,
    );
    let resolved_activation_short_turn_max_tool_rounds = resolve_usize_arg(
        None,
        defaults.activation_short_turn_max_tool_rounds.unwrap_or(
            medousa::agent_runtime::DEFAULT_ACTIVATION_SHORT_TURN_MAX_TOOL_ROUNDS,
        ),
        medousa::agent_runtime::ROUND_LIMIT_MIN,
        medousa::agent_runtime::ROUND_LIMIT_MAX,
    );
    let resolved_continuation_max_tool_rounds = resolve_usize_arg(
        None,
        defaults
            .continuation_max_tool_rounds
            .unwrap_or(medousa::agent_runtime::DEFAULT_CONTINUATION_MAX_TOOL_ROUNDS),
        medousa::agent_runtime::ROUND_LIMIT_MIN,
        medousa::agent_runtime::ROUND_LIMIT_MAX,
    );
    let resolved_max_text_only_stuck_continues = resolve_usize_arg(
        None,
        defaults
            .max_text_only_stuck_continues
            .unwrap_or(medousa::agent_runtime::DEFAULT_MAX_TEXT_ONLY_STUCK_CONTINUES),
        medousa::agent_runtime::ROUND_LIMIT_MIN,
        medousa::agent_runtime::ROUND_LIMIT_MAX,
    );
    let resolved_classifier_restricted_max_tool_rounds = resolve_usize_arg(
        None,
        defaults.classifier_restricted_max_tool_rounds.unwrap_or(
            medousa::agent_runtime::DEFAULT_CLASSIFIER_RESTRICTED_MAX_TOOL_ROUNDS,
        ),
        medousa::agent_runtime::ROUND_LIMIT_MIN,
        medousa::agent_runtime::ROUND_LIMIT_MAX,
    );
    let resolved_verifier_min_citation_coverage = resolve_f32_arg(
        None,
        defaults.verifier_min_citation_coverage.unwrap_or(0.60),
        0.0,
        1.0,
    );
    let resolved_verifier_min_avg_support_strength = resolve_f32_arg(
        None,
        defaults.verifier_min_avg_support_strength.unwrap_or(0.70),
        0.0,
        1.0,
    );
    let resolved_verifier_min_supported_claim_ratio = resolve_f32_arg(
        None,
        defaults.verifier_min_supported_claim_ratio.unwrap_or(0.60),
        0.0,
        1.0,
    );
    let resolved_verifier_min_claim_support_strength = resolve_f32_arg(
        None,
        defaults.verifier_min_claim_support_strength.unwrap_or(0.65),
        0.0,
        1.0,
    );
    let resolved_web_search_preferred_provider = defaults
        .web_search_preferred_provider
        .clone()
        .unwrap_or_default();
    let resolved_web_search_try_fallbacks = resolve_bool_arg(
        None,
        defaults.web_search_try_fallbacks.unwrap_or(true),
    );
    let resolved_base_url = resolve_llm_base_url(
        Some(&resolved_provider),
        base_url.or(defaults.base_url.as_deref()),
    );
    let resolved_api_key = load_tui_api_key().unwrap_or_default();
    let resolved_allowed_modules = defaults
        .allowed_modules
        .clone()
        .unwrap_or_default()
        .join(",");
    let provider_model = format!("{resolved_provider}:{resolved_model}");
    let resolved_stage_routing = defaults
        .stage_routing
        .clone()
        .unwrap_or_else(|| StageRoutingMatrix::default_for(&resolved_provider, &resolved_model));
    let resolved_response_depth_mode = normalize_response_depth_mode(
        defaults
            .response_depth_mode
            .as_deref()
            .unwrap_or("standard"),
    );
    let resolved_daemon_url = resolve_daemon_url(daemon_url);
    let tui_platform_config = TuiPlatformBuildConfig::from_names(
        &resolved_backend,
        Some(&resolved_provider),
        Some(&resolved_model),
        resolved_base_url.as_deref(),
        parse_allowed_modules(&resolved_allowed_modules),
        "", // session id filled below
        &resolved_daemon_url,
        local_runtime_only,
    );
    let (_, platform_mode) = resolve_tui_platform_mode(&tui_platform_config);
    if platform_mode == TuiPlatformMode::ClientStub {
        resolved_backend = "in-memory".to_string();
    }

    let session_id = explicit_session
        .map(str::to_string)
        .or_else(|| {
            load_last_session_id().and_then(|id| {
                let trimmed = id.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            })
        })
        .unwrap_or_else(|| Uuid::new_v4().simple().to_string());
    save_last_session_id(&session_id);

    let history = match daemon_commands::daemon_load_session_history(&resolved_daemon_url, &session_id).await {
        Ok(response) => response.turns,
        Err(_) => load_history(&session_id),
    };

    let (event_tx, mut event_rx) = mpsc::channel::<TuiEvent>(256);
    let (worker_cmd_tx, worker_cmd_rx) = mpsc::channel::<WorkerCommand>(32);
    let (worker_result_tx, mut worker_result_rx) = mpsc::channel::<WorkerResult>(64);

    tokio::spawn(worker_loop(worker_cmd_rx, worker_result_tx));

    let mut tui_platform_config = tui_platform_config;
    tui_platform_config.session_id = session_id.clone();

    let daemon_agent_primary = !local_runtime_only && platform_mode == TuiPlatformMode::ClientStub;

    let mut tui_rt = build_tui_platform(tui_platform_config, event_tx.clone()).await?;

    // ── Terminal setup ────────────────────────────────────────────────────────
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Fullscreen,
        },
    )?;
    terminal.hide_cursor()?;

    let initial_settings = RuntimeSettings {
        backend: resolved_backend.clone(),
        theme_id: resolved_theme_id,
        provider: resolved_provider.clone(),
        model: resolved_model.clone(),
        base_url: resolved_base_url.clone().unwrap_or_default(),
        env_overrides: defaults.env_overrides.clone().unwrap_or_default(),
        api_key: resolved_api_key.clone(),
        allowed_modules: resolved_allowed_modules.clone(),
        tool_call_mode: resolved_tool_call_mode.clone(),
        max_tool_rounds: resolved_max_tool_rounds.to_string(),
        thinking_capture: resolved_thinking_capture.to_string(),
        stasis_otel_enabled: resolved_stasis_otel_enabled.to_string(),
        thinking_max_lines: resolved_thinking_max_lines.to_string(),
        activation_direct_answer_max_prompt_chars:
            resolved_activation_direct_answer_max_prompt_chars.to_string(),
        activation_long_session_turn_threshold: resolved_activation_long_session_turn_threshold
            .to_string(),
        activation_long_session_max_prompt_chars: resolved_activation_long_session_max_prompt_chars
            .to_string(),
        slice_hot_window_turns: resolved_slice_hot_window_turns.to_string(),
        slice_cold_window_turns: resolved_slice_cold_window_turns.to_string(),
        retry_runtime_max_retries: resolved_retry_runtime_max_retries.to_string(),
        retry_runtime_max_rounds: resolved_retry_runtime_max_rounds.to_string(),
        host_bus_max_tool_rounds: resolved_host_bus_max_tool_rounds.to_string(),
        host_turn_bus_mode: resolved_host_turn_bus_mode,
        activation_tool_intent_max_rounds: resolved_activation_tool_intent_max_rounds.to_string(),
        activation_short_turn_max_tool_rounds: resolved_activation_short_turn_max_tool_rounds
            .to_string(),
        continuation_max_tool_rounds: resolved_continuation_max_tool_rounds.to_string(),
        max_text_only_stuck_continues: resolved_max_text_only_stuck_continues.to_string(),
        classifier_restricted_max_tool_rounds: resolved_classifier_restricted_max_tool_rounds
            .to_string(),
        verifier_min_citation_coverage: format!("{:.2}", resolved_verifier_min_citation_coverage),
        verifier_min_avg_support_strength: format!(
            "{:.2}",
            resolved_verifier_min_avg_support_strength
        ),
        verifier_min_supported_claim_ratio: format!(
            "{:.2}",
            resolved_verifier_min_supported_claim_ratio
        ),
        verifier_min_claim_support_strength: format!(
            "{:.2}",
            resolved_verifier_min_claim_support_strength
        ),
        web_search_preferred_provider: resolved_web_search_preferred_provider,
        web_search_try_fallbacks: resolved_web_search_try_fallbacks.to_string(),
    };

    let mut state = TuiState {
        conversation: history,
        observability: VecDeque::new(),
        observability_filter: ObservabilityFilter::All,
        job_history: VecDeque::new(),
        input_buffer: String::new(),
        conv_scroll: 0,
        conv_max_scroll: 0,
        is_processing: false,
        active_request_task: None,
        auto_scroll: true,
        active_agent_turn_id: 0,
        open_stream_turn_id: None,
        active_agent_stream_turn: None,
        mode: UiMode::Startup,
        startup_selected: 0,
        history_items: Vec::new(),
        history_selected: 0,
        history_scroll: 0,
        history_max_scroll: 0,
        history_show_verification_detail: false,
        command_query: String::new(),
        command_tab: 0,
        command_selected: 0,
        command_scroll: 0,
        command_max_scroll: 0,
        command_usage_counts: defaults.command_usage_counts.unwrap_or_default(),
        settings: initial_settings.clone(),
        settings_draft: initial_settings,
        allowlist_preview_source: String::new(),
        editor_buffer: TextBuffer::default(),
        editor_file_path: None,
        editor_status: "No file loaded".to_string(),
        editor_dirty: false,
        editor_preferred_col: None,
        editor_scroll: 0,
        settings_tab: 0,
        settings_selected: 0,
        settings_editing: false,
        settings_scroll: 0,
        settings_max_scroll: 0,
        theme_menu_selected: 0,
        theme_menu_scroll: 0,
        theme_menu_max_scroll: 0,
        theme_menu_return_mode: UiMode::Chat,
        theme_menu_original_theme_id: String::new(),
        theme_menu_original_draft_theme_id: String::new(),
        routing_editor_role_idx: 0,
        runtime_env_editing: false,
        provider_model,
        response_depth_mode: resolved_response_depth_mode,
        session_id: session_id.clone(),
        session_display_name: medousa::session::get_session_display_name(&session_id),
        selected_context_pack_query: None,
        stage_routing: resolved_stage_routing.clone(),
        stage_routing_draft: resolved_stage_routing,
        thinking_trace: VecDeque::new(),
        pending_thinking_buffer: String::new(),
        thinking_scroll: 0,
        thinking_max_scroll: 0,
        grapheme_console: VecDeque::new(),
        grapheme_console_scroll: 0,
        grapheme_console_max_scroll: 0,
        obs_scroll: 0,
        obs_max_scroll: 0,
        job_scroll: 0,
        job_max_scroll: 0,
        in_thinking_tag: false,
        stream_tag_tail: String::new(),
        received_native_reasoning: false,
        pending_response_verified: None,
        daemon_url: resolved_daemon_url.clone(),
        local_runtime_only,
        next_settings_apply_request_id: 0,
        active_settings_apply_request_id: None,
        pending_settings_apply: None,
        ui_dirty: false,
        pending_agent_chunk_delta: String::new(),
        pending_agent_chunk_count: 0,
        turn_parts: medousa::turn_parts::TurnPartsAccumulator::default(),
        pending_paint_since: None,
        perf: UiPerfStats::default(),
        worker_cmd_tx,
        next_worker_request_id: 0,
        latest_daemon_health_request_id: 0,
        latest_daemon_ask_request_id: 0,
        latest_watch_add_request_id: 0,
        pending_budget_request_id: None,
        pending_budget_requested_rounds: None,
        markdown_cache: RefCell::new(HashMap::new()),
        markdown_cache_order: RefCell::new(VecDeque::new()),
        perf_baseline: None,
    };

    if local_runtime_only {
        push_obs(
            &mut state,
            "◈ local-runtime-only — agent turns stay in-process (offline/dev mode)".to_string(),
        );
    } else if daemon_agent_primary {
        push_obs(
            &mut state,
            format!(
                "◈ daemon-primary — agent turns via {} ({})",
                resolved_daemon_url,
                medousa::agent_runtime::AGENT_RUNTIME_VERSION
            ),
        );
    } else {
        push_obs(
            &mut state,
            format!(
                "◈ daemon unavailable at {} — local runtime fallback for chat",
                resolved_daemon_url
            ),
        );
    }

    // ── Keyboard reader (spawn_blocking to keep async event loop clean) ───────
    let (key_tx, mut key_rx) = mpsc::channel::<Event>(64);
    tokio::task::spawn_blocking(move || {
        loop {
            if crossterm::event::poll(Duration::from_millis(50)).unwrap_or(false) {
                match crossterm::event::read() {
                    Ok(event) => {
                        if key_tx.blocking_send(event).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        }
    });

    // ── Main event loop ───────────────────────────────────────────────────────
    let initial_render_started = Instant::now();
    terminal.draw(|f| render(f, &mut state))?;
    note_frame_rendered(&mut state, initial_render_started);
    loop {
        let wake_after = next_ui_wake_delay(&state);
        tokio::select! {
            Some(event) = key_rx.recv() => {
                mark_ui_activity(&mut state);
                match handle_key_event(event, &mut state, &mut tui_rt, &event_tx).await {
                    EventOutcome::Break => break,
                    EventOutcome::Continue => {
                        state.ui_dirty = true;
                    }
                }
            }
            Some(tui_event) = event_rx.recv() => {
                mark_ui_activity(&mut state);
                handle_tui_event(tui_event, &mut state).await;
                state.ui_dirty = true;
            }
            Some(worker_result) = worker_result_rx.recv() => {
                mark_ui_activity(&mut state);
                handle_worker_result(worker_result, &mut state);
                state.ui_dirty = true;
            }
            _ = tokio::time::sleep(wake_after) => {}
        }

        match drain_pending_key_events(&mut key_rx, &mut state, &mut tui_rt, &event_tx).await {
            EventOutcome::Break => break,
            EventOutcome::Continue => {}
        }

        flush_pending_agent_chunks(&mut state);

        if finalize_settings_apply_if_ready(&mut state, &mut tui_rt).await {
            mark_ui_activity(&mut state);
            state.ui_dirty = true;
        }

        if state.ui_dirty {
            let render_started = Instant::now();
            terminal.draw(|f| render(f, &mut state))?;
            note_frame_rendered(&mut state, render_started);
            state.ui_dirty = false;
        }
    }

    // ── Restore terminal ──────────────────────────────────────────────────────
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

fn normalize_response_depth_mode(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "concise" => "concise".to_string(),
        "deep" => "deep".to_string(),
        _ => "standard".to_string(),
    }
}

// ── Event handling ────────────────────────────────────────────────────────────

enum EventOutcome {
    Continue,
    Break,
}

async fn drain_pending_key_events(
    key_rx: &mut mpsc::Receiver<Event>,
    state: &mut TuiState,
    tui_rt: &mut TuiRuntime,
    event_tx: &mpsc::Sender<TuiEvent>,
) -> EventOutcome {
    let mut drained = 0usize;
    while drained < 32 {
        match key_rx.try_recv() {
            Ok(event) => {
                mark_ui_activity(state);
                match handle_key_event(event, state, tui_rt, event_tx).await {
                    EventOutcome::Break => return EventOutcome::Break,
                    EventOutcome::Continue => {
                        state.ui_dirty = true;
                    }
                }
                drained = drained.saturating_add(1);
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
            | Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => break,
        }
    }

    if drained > 0 {
        state.perf.coalesced_key_events = state
            .perf
            .coalesced_key_events
            .saturating_add(drained as u64);
    }

    EventOutcome::Continue
}

async fn handle_command_palette_key_event(
    code: KeyCode,
    state: &mut TuiState,
    tui_rt: &mut TuiRuntime,
    event_tx: &mpsc::Sender<TuiEvent>,
) -> EventOutcome {
    command_preview_ui::handle_command_palette_key_event(code, state, tui_rt, event_tx).await
}

fn open_theme_menu(state: &mut TuiState, return_mode: UiMode) {
    theme_ui::open_theme_menu(state, return_mode);
}

fn handle_theme_menu_key_event(code: KeyCode, state: &mut TuiState) -> EventOutcome {
    theme_ui::handle_theme_menu_key_event(code, state)
}

async fn handle_history_key_event(code: KeyCode, state: &mut TuiState) -> EventOutcome {
    match code {
        KeyCode::Char('v') | KeyCode::Char('V') => {
            state.history_show_verification_detail = !state.history_show_verification_detail;
        }
        KeyCode::Up => {
            state.history_selected = state.history_selected.saturating_sub(1);
        }
        KeyCode::Down => {
            if !state.history_items.is_empty() {
                state.history_selected =
                    (state.history_selected + 1).min(state.history_items.len().saturating_sub(1));
            }
        }
        KeyCode::PageUp => {
            state.history_scroll = state.history_scroll.saturating_sub(8);
            state.history_selected = state.history_selected.saturating_sub(8);
        }
        KeyCode::PageDown => {
            state.history_scroll = state
                .history_scroll
                .saturating_add(8)
                .min(state.history_max_scroll);
            if !state.history_items.is_empty() {
                state.history_selected =
                    (state.history_selected + 8).min(state.history_items.len().saturating_sub(1));
            }
        }
        KeyCode::Home => {
            state.history_scroll = 0;
            state.history_selected = 0;
        }
        KeyCode::End => {
            state.history_scroll = state.history_max_scroll;
            if !state.history_items.is_empty() {
                state.history_selected = state.history_items.len().saturating_sub(1);
            }
        }
        KeyCode::Enter => {
            if let Some(selected) = state.history_items.get(state.history_selected).cloned() {
                stop_active_generation(state);
                state.session_id = selected.session_id.clone();
                state.session_display_name = selected.display_name.clone();
                let session_id = state.session_id.clone();
                state.conversation = history_services::load_history_daemon_first(state, &session_id).await;
                session_name_services::refresh_session_display_name(state);
                invalidate_markdown_cache(state);
                state.thinking_trace.clear();
                state.pending_thinking_buffer.clear();
                state.thinking_scroll = 0;
                state.thinking_max_scroll = 0;
                state.in_thinking_tag = false;
                state.stream_tag_tail.clear();
                state.received_native_reasoning = false;
                state.input_buffer.clear();
                state.is_processing = false;
                state.open_stream_turn_id = None;
                state.active_agent_stream_turn = None;
                state.auto_scroll = true;
                state.conv_scroll = state.conv_max_scroll;
                save_last_session_id(&state.session_id);
                state.mode = UiMode::Chat;
            }
        }
        _ => {}
    }

    EventOutcome::Continue
}

fn push_obs(state: &mut TuiState, text: String) {
    state.observability.push_front(ObsEvent { text });
    if state.observability.len() > 50 {
        state.observability.pop_back();
    }
    invalidate_markdown_cache(state);
}

fn push_grapheme_console_entry(
    state: &mut TuiState,
    source_label: &str,
    job_id: &str,
    succeeded: bool,
    diagnostics: &Value,
) {
    let status = if succeeded { "succeeded" } else { "failed" };
    let mut entry = format!("[{status}] {source_label} ({job_id})");
    let console_json = diagnostics
        .get("final_state")
        .cloned()
        .unwrap_or_else(|| diagnostics.clone());

    let rendered =
        serde_json::to_string_pretty(&console_json).unwrap_or_else(|_| console_json.to_string());
    if !rendered.trim().is_empty() {
        entry.push('\n');
        entry.push_str(&rendered);
    }

    state.grapheme_console.push_front(entry);
    if state.grapheme_console.len() > 100 {
        state.grapheme_console.pop_back();
    }
    invalidate_markdown_cache(state);
}

fn push_thinking(state: &mut TuiState, raw: String) {
    if !parse_bool_with_default(&state.settings.thinking_capture, true) {
        state.pending_thinking_buffer.clear();
        return;
    }

    let fragment = raw.replace('\r', "");
    if fragment.trim().is_empty() {
        return;
    }

    append_thinking_fragment(&mut state.pending_thinking_buffer, &fragment);

    if should_flush_thinking_buffer(&state.pending_thinking_buffer) {
        flush_thinking_buffer(state);
    }
}

fn append_thinking_fragment(buffer: &mut String, fragment: &str) {
    let trimmed = fragment.trim();
    if trimmed.is_empty() {
        return;
    }

    let needs_space = !buffer.is_empty()
        && buffer
            .chars()
            .last()
            .map(|c| c.is_ascii_alphanumeric())
            .unwrap_or(false)
        && trimmed
            .chars()
            .next()
            .map(|c| c.is_ascii_alphanumeric())
            .unwrap_or(false);

    if needs_space {
        buffer.push(' ');
    }

    buffer.push_str(trimmed);
}

fn should_flush_thinking_buffer(buffer: &str) -> bool {
    const THINKING_BUFFER_FLUSH_CHARS: usize = 120;

    if buffer.contains('\n') {
        return true;
    }

    let trimmed = buffer.trim_end();
    if trimmed.chars().count() >= THINKING_BUFFER_FLUSH_CHARS {
        return true;
    }

    trimmed
        .chars()
        .last()
        .map(|c| matches!(c, '.' | '!' | '?' | ';' | ':'))
        .unwrap_or(false)
}

pub(crate) fn flush_thinking_buffer(state: &mut TuiState) {
    let buffered = std::mem::take(&mut state.pending_thinking_buffer);
    for line in buffered.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let mut text: String = trimmed.chars().take(180).collect();
        if trimmed.chars().count() > 180 {
            text.push_str("...");
        }
        let stamp = Utc::now().format("%H:%M:%S").to_string();
        state.thinking_trace.push_front(format!("[{stamp}] {text}"));
    }

    let max_lines = parse_usize_with_bounds(&state.settings.thinking_max_lines, 300, 50, 5000);
    while state.thinking_trace.len() > max_lines {
        state.thinking_trace.pop_back();
    }
}

// ── Export ────────────────────────────────────────────────────────────────────

fn export_current_session(state: &TuiState, format: &str) -> std::result::Result<PathBuf, String> {
    let exports_dir = std::env::current_dir()
        .map_err(|e| e.to_string())?
        .join("exports");
    std::fs::create_dir_all(&exports_dir).map_err(|e| e.to_string())?;

    let ts = Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let verification_runs = medousa::verification_store::list_verifications(&state.session_id, 500);
    match format {
        "jsonl" => {
            let path = exports_dir.join(format!("{}-{ts}.jsonl", state.session_id));
            let mut out = String::new();
            for turn in &state.conversation {
                let line = serde_json::to_string(&serde_json::json!({
                    "kind": "turn",
                    "session_id": state.session_id,
                    "payload": turn,
                }))
                .map_err(|e| e.to_string())?;
                out.push_str(&line);
                out.push('\n');
            }
            for verification in &verification_runs {
                let line = serde_json::to_string(&serde_json::json!({
                    "kind": "verification",
                    "session_id": state.session_id,
                    "payload": verification,
                }))
                .map_err(|e| e.to_string())?;
                out.push_str(&line);
                out.push('\n');
            }
            std::fs::write(&path, out).map_err(|e| e.to_string())?;
            Ok(path)
        }
        _ => {
            let path = exports_dir.join(format!("{}-{ts}.md", state.session_id));
            let mut out = format!("# Medousa Session {}\n\n", state.session_id);
            for turn in &state.conversation {
                let title = if turn.role == "user" {
                    "User"
                } else {
                    "Assistant"
                };
                out.push_str(&format!("## {title} ({})\n\n", turn.timestamp.to_rfc3339()));
                out.push_str(&medousa::turn_parts::compose_turn_markdown(turn));
                out.push_str("\n\n");
            }

            out.push_str("## Verification Runs\n\n");
            if verification_runs.is_empty() {
                out.push_str("No verification runs recorded for this session.\n\n");
            } else {
                for run in verification_runs {
                    out.push_str(&format!(
                        "- {}  pack={}  verified={}  confidence={:.2}  source={}  {}\n",
                        run.verification_id,
                        run.pack_id,
                        run.is_verified,
                        run.confidence_score,
                        run.source,
                        run.created_at_utc.to_rfc3339(),
                    ));
                }
                out.push('\n');
            }

            std::fs::write(&path, out).map_err(|e| e.to_string())?;
            Ok(path)
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn mask_secret_value(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return "(not set)".to_string();
    }

    let visible_suffix_len = trimmed.chars().count().min(4);
    let suffix: String = trimmed
        .chars()
        .rev()
        .take(visible_suffix_len)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("********{suffix}")
}

fn api_key_storage_backend_label() -> &'static str {
    match detect_tui_api_key_storage_backend() {
        ApiKeyStorageBackend::KeychainActive => "keychain(active)",
        ApiKeyStorageBackend::KeychainReady => "keychain(ready)",
        ApiKeyStorageBackend::FileFallbackActive => "file-fallback(active)",
        ApiKeyStorageBackend::FileFallbackReady => "file-fallback(ready)",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        JobHistoryEntry, ObservabilityFilter, RuntimeSettings, StageRoutingMatrix, TextBuffer,
        TuiState, UiMode, UiPerfStats, WorkerCommand, load_editor_file,
        parse_allowed_modules, resolve_editor_run_source, run_editor_source_via_runtime,
        validate_editor_run_allowlist, write_editor_file,
    };
    use medousa::build_tui_runtime;
    use medousa::parse_backend;
    use medousa::events::TuiEvent;
    use medousa::session::{ConversationTurn, SessionHistorySummary};
    use std::collections::{HashMap, VecDeque};
    use std::path::PathBuf;
    use tokio::sync::mpsc;
    use tokio::sync::mpsc::error::TryRecvError;

    fn temp_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "medousa_tui_editor_{name}_{}",
            uuid::Uuid::new_v4()
        ));
        path
    }

    #[test]
    fn load_editor_file_returns_none_for_missing_path() {
        let path = temp_path("missing");
        let loaded = load_editor_file(&path).expect("load should not fail for missing path");
        assert!(loaded.is_none());
    }

    #[test]
    fn write_editor_file_creates_parent_dirs_and_roundtrips() {
        let dir = temp_path("roundtrip");
        let path = dir.join("nested").join("script.gr");

        write_editor_file(&path, "run {\n  ok: true\n}\n").expect("write should succeed");

        let loaded = load_editor_file(&path)
            .expect("read should succeed")
            .expect("file should exist");
        assert_eq!(loaded, "run {\n  ok: true\n}\n");

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn resolve_editor_run_source_fails_for_missing_override_path() {
        let missing = temp_path("missing_run").join("script.gr");
        let result = resolve_editor_run_source(Some(missing.to_string_lossy().as_ref()), None, "");
        assert!(result.is_err());
        let err = result.err().unwrap_or_default();
        assert!(err.contains("file not found"));
    }

    #[test]
    fn validate_editor_run_allowlist_rejects_blocked_ops() {
        let source = "query Run { websearch.search(query: \"x\") { ok } }";
        let result = validate_editor_run_allowlist(source, "http.fetch");
        assert!(result.is_err());
        let err = result.err().unwrap_or_default();
        assert!(err.contains("run blocked by allowlist"));
        assert!(err.contains("websearch.search"));
    }

    fn test_settings() -> RuntimeSettings {
        RuntimeSettings {
            backend: "in-memory".to_string(),
            theme_id: "medousa-default".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4o-mini".to_string(),
            base_url: String::new(),
            env_overrides: String::new(),
            api_key: String::new(),
            allowed_modules: "http.fetch".to_string(),
            tool_call_mode: "auto".to_string(),
            max_tool_rounds: "10".to_string(),
            host_bus_max_tool_rounds: "8".to_string(),
            host_turn_bus_mode: "auto".to_string(),
            activation_tool_intent_max_rounds: "12".to_string(),
            activation_short_turn_max_tool_rounds: "1".to_string(),
            continuation_max_tool_rounds: "4".to_string(),
            max_text_only_stuck_continues: "10".to_string(),
            classifier_restricted_max_tool_rounds: "1".to_string(),
            thinking_capture: "true".to_string(),
            stasis_otel_enabled: "false".to_string(),
            thinking_max_lines: "300".to_string(),
            activation_direct_answer_max_prompt_chars: "320".to_string(),
            activation_long_session_turn_threshold: "28".to_string(),
            activation_long_session_max_prompt_chars: "420".to_string(),
            slice_hot_window_turns: "8".to_string(),
            slice_cold_window_turns: "24".to_string(),
            retry_runtime_max_retries: "1".to_string(),
            retry_runtime_max_rounds: "10".to_string(),
            verifier_min_citation_coverage: "0.60".to_string(),
            verifier_min_avg_support_strength: "0.70".to_string(),
            verifier_min_supported_claim_ratio: "0.60".to_string(),
            verifier_min_claim_support_strength: "0.65".to_string(),
            web_search_preferred_provider: String::new(),
            web_search_try_fallbacks: "true".to_string(),
        }
    }

    fn test_state(settings: RuntimeSettings) -> TuiState {
        TuiState {
            worker_cmd_tx: mpsc::channel::<WorkerCommand>(8).0,
            conversation: Vec::<ConversationTurn>::new(),
            observability: VecDeque::new(),
            observability_filter: ObservabilityFilter::All,
            job_history: VecDeque::<JobHistoryEntry>::new(),
            input_buffer: String::new(),
            conv_scroll: 0,
            conv_max_scroll: 0,
            is_processing: false,
            active_request_task: None,
            auto_scroll: true,
            active_agent_turn_id: 0,
            open_stream_turn_id: None,
            active_agent_stream_turn: None,
            mode: UiMode::Chat,
            startup_selected: 0,
            history_items: Vec::<SessionHistorySummary>::new(),
            history_selected: 0,
            history_scroll: 0,
            history_max_scroll: 0,
            history_show_verification_detail: false,
            command_query: String::new(),
            command_tab: 0,
            command_selected: 0,
            command_scroll: 0,
            command_max_scroll: 0,
            command_usage_counts: HashMap::new(),
            settings: settings.clone(),
            settings_draft: settings,
            allowlist_preview_source: String::new(),
            editor_buffer: TextBuffer::from_text(
                "query Run { websearch.search(query: \"x\") { ok } }".to_string(),
            ),
            editor_file_path: None,
            editor_status: String::new(),
            editor_dirty: false,
            editor_preferred_col: None,
            editor_scroll: 0,
            settings_tab: 0,
            settings_selected: 0,
            settings_editing: false,
            settings_scroll: 0,
            settings_max_scroll: 0,
            theme_menu_selected: 0,
            theme_menu_scroll: 0,
            theme_menu_max_scroll: 0,
            theme_menu_return_mode: UiMode::Chat,
            theme_menu_original_theme_id: String::new(),
            theme_menu_original_draft_theme_id: String::new(),
            routing_editor_role_idx: 0,
            runtime_env_editing: false,
            provider_model: "openai:gpt-4o-mini".to_string(),
            response_depth_mode: "standard".to_string(),
            session_id: "test-session".to_string(),
            session_display_name: None,
            selected_context_pack_query: None,
            stage_routing: StageRoutingMatrix::default_for("openai", "gpt-4o-mini"),
            stage_routing_draft: StageRoutingMatrix::default_for("openai", "gpt-4o-mini"),
            thinking_trace: VecDeque::new(),
            pending_thinking_buffer: String::new(),
            thinking_scroll: 0,
            thinking_max_scroll: 0,
            grapheme_console: VecDeque::new(),
            grapheme_console_scroll: 0,
            grapheme_console_max_scroll: 0,
            obs_scroll: 0,
            obs_max_scroll: 0,
            job_scroll: 0,
            job_max_scroll: 0,
            in_thinking_tag: false,
            stream_tag_tail: String::new(),
            received_native_reasoning: false,
            pending_response_verified: None,
            daemon_url: "http://127.0.0.1:8787".to_string(),
            local_runtime_only: false,
            next_settings_apply_request_id: 0,
            active_settings_apply_request_id: None,
            pending_settings_apply: None,
            ui_dirty: false,
            pending_agent_chunk_delta: String::new(),
            pending_agent_chunk_count: 0,
        turn_parts: medousa::turn_parts::TurnPartsAccumulator::default(),
            pending_paint_since: None,
            perf: UiPerfStats::default(),
            next_worker_request_id: 0,
            latest_daemon_health_request_id: 0,
            latest_daemon_ask_request_id: 0,
            latest_watch_add_request_id: 0,
            pending_budget_request_id: None,
            pending_budget_requested_rounds: None,
            markdown_cache: std::cell::RefCell::new(std::collections::HashMap::new()),
            markdown_cache_order: std::cell::RefCell::new(VecDeque::new()),
            perf_baseline: None,
        }
    }

    #[tokio::test]
    async fn blocked_run_does_not_emit_runtime_events() {
        let settings = test_settings();
        let (event_tx, mut event_rx) = mpsc::channel::<TuiEvent>(64);

        let tui_rt = build_tui_runtime(
            parse_backend(Some(&settings.backend)),
            Some(&settings.provider),
            Some(&settings.model),
            None,
            parse_allowed_modules(&settings.allowed_modules),
            "test-session",
            event_tx.clone(),
        )
        .await
        .expect("runtime should build");

        while event_rx.try_recv().is_ok() {}

        let mut state = test_state(settings);
        run_editor_source_via_runtime(&mut state, &tui_rt, &event_tx, None).await;

        let obs = state
            .observability
            .front()
            .map(|v| v.text.clone())
            .unwrap_or_default();
        assert!(obs.contains("run blocked by allowlist"));

        match event_rx.try_recv() {
            Err(TryRecvError::Empty) => {}
            Ok(evt) => panic!("unexpected runtime event emitted: {evt:?}"),
            Err(err) => panic!("unexpected channel state: {err}"),
        }
    }
}
