//! Clap argv for `medousa_tui`.

use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "medousa_tui",
    about = "medousa-tui — persistent cognitive terminal agent",
    after_help = "Keys: Enter submit · Ctrl+; panes · Ctrl+; o/f/r/t/w notes/code/review/term/connection · Ctrl+O observability · Ctrl+K palette · Ctrl+, settings · Ctrl+C quit\nPanes: Ctrl+; then % \" h/j/k/l z x c o f r t/T w n/p 1-4\nSlash: /help · /new · /notes · /code · /review · /terminal · /connection · /history · /settings · /close\nRun with no unknown flags — typos like --modl are rejected."
)]
pub struct TuiCli {
    #[arg(long, env = "MEDOUSA_LLM_PROVIDER")]
    pub provider: Option<String>,
    #[arg(long, env = "MEDOUSA_LLM_MODEL")]
    pub model: Option<String>,
    #[arg(long = "base-url", env = "MEDOUSA_LLM_BASE_URL")]
    pub base_url: Option<String>,
    #[arg(long, env = "MEDOUSA_BACKEND")]
    pub backend: Option<String>,
    #[arg(long = "tool-call-mode")]
    pub tool_call_mode: Option<String>,
    #[arg(long = "max-tool-rounds")]
    pub max_tool_rounds: Option<String>,
    #[arg(long = "thinking-capture")]
    pub thinking_capture: Option<String>,
    #[arg(long = "thinking-max-lines")]
    pub thinking_max_lines: Option<String>,
    #[arg(long = "daemon-url", env = "MEDOUSA_DAEMON_URL")]
    pub daemon_url: Option<String>,
    #[arg(long)]
    pub session: Option<String>,
    #[arg(long = "local-runtime-only")]
    pub local_runtime_only: bool,
}
