//! Clap front-end for the `medousa` operator binary.
//!
//! Parses argv (so `--help` never runs side effects and unknown flags error),
//! then rebuilds a legacy `&[String]` for the existing hand-rolled handlers.

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "medousa",
    about = "Medousa operator CLI — run and troubleshoot your engine.",
    long_about = "Everyday chat → open the Medousa app. This CLI is for operators and automation.\n\nRun medousa <command> --help for flags."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    #[command(visible_aliases = ["setup", "init"])]
    Onboard(OnboardArgs),
    Start(StartArgs),
    Tui(PassthroughArgs),
    Daemon(PassthroughArgs),
    Discord(AdapterPassthroughArgs),
    Telegram(TelegramArgs),
    Slack(SlackArgs),
    Whatsapp(WhatsappArgs),
    Doctor(DoctorArgs),
    #[command(name = "session-storage")]
    SessionStorage(SessionStorageArgs),
    Status,
    Stop(StopArgs),
    #[command(name = "identity-export")]
    IdentityExport(IdentityExportArgs),
    #[command(name = "identity-remember")]
    IdentityRemember(IdentityRememberArgs),
    #[command(name = "identity-profiles")]
    IdentityProfiles(IdentityProfilesArgs),
    #[command(name = "manuscript-list")]
    ManuscriptList,
    #[command(name = "manuscript-validate")]
    ManuscriptValidate {
        id: String,
    },
    #[command(name = "manuscript-install")]
    ManuscriptInstall {
        path: String,
        #[arg(long)]
        project: bool,
    },
    #[command(name = "skill-import")]
    SkillImport(SkillImportArgs),
    #[command(name = "openshell-probe")]
    OpenshellProbe(OpenshellProbeArgs),
    Workspace(WorkspaceArgs),
    Vault(VaultArgs),
    Pair(PairArgs),
    Credentials(CredentialsArgs),
    Peer(PeerArgs),
    #[cfg(feature = "iroh-transport")]
    Iroh(IrohArgs),
    Models(ModelsArgs),
    Pull(PullArgs),
    Update(UpdateArgs),
    Packages(PackagesArgs),
}

#[derive(Debug, Args)]
pub struct OnboardArgs {
    #[arg(long)]
    pub yes: bool,
    #[arg(long)]
    pub daemon: bool,
    #[arg(long = "no-daemon")]
    pub no_daemon: bool,
    #[arg(long)]
    pub tui: bool,
    #[arg(long = "no-tui")]
    pub no_tui: bool,
    #[arg(long)]
    pub provider: Option<String>,
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long)]
    pub backend: Option<String>,
    #[arg(long = "daemon-url")]
    pub daemon_url: Option<String>,
    #[arg(long = "base-url")]
    pub base_url: Option<String>,
    #[arg(long = "api-key")]
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum StartService {
    Daemon,
    #[value(name = "daemon-restart", alias = "restart-daemon")]
    DaemonRestart,
    #[value(name = "mcp-gateway", alias = "mcp_gateway", alias = "mcp")]
    McpGateway,
    Discord,
    Telegram,
    Slack,
    Whatsapp,
    All,
}

#[derive(Debug, Args)]
pub struct StartArgs {
    pub service: StartService,
    #[arg(long)]
    pub backend: Option<String>,
    #[arg(long)]
    pub bind: Option<String>,
    #[arg(long)]
    pub public: bool,
    #[arg(long = "daemon-url")]
    pub daemon_url: Option<String>,
    #[arg(long, visible_alias = "local-engine", alias = "private-brain")]
    pub inference: bool,
}

#[derive(Debug, Args)]
pub struct PassthroughArgs {
    #[arg(long)]
    pub backend: Option<String>,
    #[arg(long)]
    pub bind: Option<String>,
    #[arg(long)]
    pub public: bool,
    #[arg(long = "daemon-url")]
    pub daemon_url: Option<String>,
    #[arg(long, visible_alias = "local-engine", alias = "private-brain")]
    pub inference: bool,
    #[arg(long = "no-daemon")]
    pub no_daemon: bool,
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub rest: Vec<String>,
}

#[derive(Debug, Args)]
pub struct AdapterPassthroughArgs {
    #[arg(long = "daemon-url")]
    pub daemon_url: Option<String>,
    #[arg(long)]
    pub token: Option<String>,
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub rest: Vec<String>,
}

#[derive(Debug, Args)]
pub struct TelegramArgs {
    #[arg(long = "daemon-url")]
    pub daemon_url: Option<String>,
    #[arg(long)]
    pub token: Option<String>,
    #[arg(long = "allow-user-ids")]
    pub allow_user_ids: Option<String>,
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub rest: Vec<String>,
}

#[derive(Debug, Args)]
pub struct SlackArgs {
    #[arg(long = "daemon-url")]
    pub daemon_url: Option<String>,
    #[arg(long = "bot-token")]
    pub bot_token: Option<String>,
    #[arg(long = "app-token")]
    pub app_token: Option<String>,
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub rest: Vec<String>,
}

#[derive(Debug, Args)]
pub struct WhatsappArgs {
    #[arg(long = "daemon-url")]
    pub daemon_url: Option<String>,
    #[arg(long = "deliver-bind")]
    pub deliver_bind: Option<String>,
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub rest: Vec<String>,
}

#[derive(Debug, Args)]
pub struct DoctorArgs {
    #[arg(long)]
    pub config: bool,
    #[arg(long)]
    pub json: bool,
    #[arg(long = "local-engine")]
    pub local_engine: bool,
}

#[derive(Debug, Args)]
pub struct SessionStorageArgs {
    /// Apply safe, unambiguous migrations. The default is a read-only dry run.
    #[arg(long)]
    pub apply: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct StopArgs {
    #[arg(long = "local-engine")]
    pub local_engine: bool,
    #[arg(long)]
    pub all: bool,
}

#[derive(Debug, Args)]
pub struct IdentityExportArgs {
    #[arg(long = "user-id")]
    pub user_id: Option<String>,
    #[arg(long)]
    pub dir: Option<String>,
}

#[derive(Debug, Args)]
pub struct IdentityRememberArgs {
    #[arg(long)]
    pub kind: String,
    #[arg(long)]
    pub subject: String,
    #[arg(long)]
    pub statement: String,
    #[arg(long, default_value = "user_direct")]
    pub source: String,
    #[arg(long = "user-id")]
    pub user_id: Option<String>,
    #[arg(long)]
    pub attributes: Option<String>,
}

#[derive(Debug, Args)]
pub struct IdentityProfilesArgs {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub rest: Vec<String>,
}

#[derive(Debug, Args)]
pub struct SkillImportArgs {
    pub path: Option<String>,
    #[arg(long)]
    pub project: bool,
    #[arg(long)]
    pub force: bool,
    #[arg(long, default_value = "base-researcher")]
    pub extends: String,
    #[arg(long = "no-extends")]
    pub no_extends: bool,
    #[arg(long = "from-hermes")]
    pub from_hermes: bool,
    #[arg(long = "from-openclaw")]
    pub from_openclaw: bool,
    #[arg(long = "from-cursor")]
    pub from_cursor: bool,
}

#[derive(Debug, Args)]
pub struct OpenshellProbeArgs {
    pub manuscript_id_pos: Option<String>,
    #[arg(long = "from")]
    pub from: Option<String>,
    #[arg(long)]
    pub policy: Option<String>,
    #[arg(long = "skip-grapheme")]
    pub skip_grapheme: bool,
    #[arg(long = "manuscript-id")]
    pub manuscript_id: Option<String>,
    #[arg(long)]
    pub script: Option<String>,
}

#[derive(Debug, Args)]
pub struct WorkspaceArgs {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub rest: Vec<String>,
}

#[derive(Debug, Args)]
pub struct VaultArgs {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub rest: Vec<String>,
}

#[derive(Debug, Args)]
pub struct CredentialsArgs {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub rest: Vec<String>,
}

#[derive(Debug, Args)]
pub struct PairArgs {
    #[arg(long = "daemon-url")]
    pub daemon_url: Option<String>,
    #[command(subcommand)]
    pub command: Option<PairCommand>,
}

#[derive(Debug, Subcommand)]
pub enum PairCommand {
    Status,
    List,
    Qr {
        #[arg(long)]
        term: bool,
        #[arg(long)]
        open: bool,
        #[arg(long)]
        full: bool,
    },
    Remove {
        pairing_id: String,
        #[arg(long = "daemon-url")]
        daemon_url: Option<String>,
    },
    Lan {
        #[command(subcommand)]
        action: PairLanAction,
    },
}

#[derive(Debug, Subcommand)]
pub enum PairLanAction {
    Status,
    On,
    Off,
}

#[derive(Debug, Args)]
pub struct PeerArgs {
    #[arg(long = "daemon-url")]
    pub daemon_url: Option<String>,
    #[command(subcommand)]
    pub command: Option<PeerCommand>,
}

#[derive(Debug, Subcommand)]
pub enum PeerCommand {
    Nearby {
        #[arg(long)]
        portal: bool,
    },
    Connect {
        daemon_url: String,
        #[arg(long)]
        name: Option<String>,
    },
    List,
    Remove {
        id_or_label: String,
    },
    Send {
        id_or_label: String,
        message: Vec<String>,
    },
    Inbox {
        #[arg(long)]
        unread: bool,
    },
    Read {
        message_id: String,
    },
}

#[cfg(feature = "iroh-transport")]
#[derive(Debug, Args)]
pub struct IrohArgs {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub rest: Vec<String>,
}

#[derive(Debug, Args)]
pub struct ModelsArgs {
    #[arg(long = "daemon-url")]
    pub daemon_url: Option<String>,
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub rest: Vec<String>,
}

#[derive(Debug, Args)]
pub struct PullArgs {
    pub name: String,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct UpdateArgs {
    pub name: Option<String>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct PackagesArgs {
    #[arg(long)]
    pub json: bool,
    #[command(subcommand)]
    pub command: Option<PackagesCommand>,
}

#[derive(Debug, Subcommand)]
pub enum PackagesCommand {
    List,
    Status,
}

fn push_flag(out: &mut Vec<String>, flag: &str, on: bool) {
    if on {
        out.push(flag.to_string());
    }
}

fn push_opt(out: &mut Vec<String>, flag: &str, value: Option<&str>) {
    if let Some(v) = value {
        out.push(flag.to_string());
        out.push(v.to_string());
    }
}

impl OnboardArgs {
    pub fn to_legacy(&self) -> Vec<String> {
        let mut out = Vec::new();
        push_flag(&mut out, "--yes", self.yes);
        push_flag(&mut out, "--daemon", self.daemon);
        push_flag(&mut out, "--no-daemon", self.no_daemon);
        push_flag(&mut out, "--tui", self.tui);
        push_flag(&mut out, "--no-tui", self.no_tui);
        push_opt(&mut out, "--provider", self.provider.as_deref());
        push_opt(&mut out, "--model", self.model.as_deref());
        push_opt(&mut out, "--backend", self.backend.as_deref());
        push_opt(&mut out, "--daemon-url", self.daemon_url.as_deref());
        push_opt(&mut out, "--base-url", self.base_url.as_deref());
        push_opt(&mut out, "--api-key", self.api_key.as_deref());
        out
    }
}

impl StartArgs {
    pub fn to_legacy(&self) -> Vec<String> {
        let mut out = vec![match self.service {
            StartService::Daemon => "daemon".into(),
            StartService::DaemonRestart => "daemon-restart".into(),
            StartService::McpGateway => "mcp-gateway".into(),
            StartService::Discord => "discord".into(),
            StartService::Telegram => "telegram".into(),
            StartService::Slack => "slack".into(),
            StartService::Whatsapp => "whatsapp".into(),
            StartService::All => "all".into(),
        }];
        push_opt(&mut out, "--backend", self.backend.as_deref());
        push_opt(&mut out, "--bind", self.bind.as_deref());
        push_flag(&mut out, "--public", self.public);
        push_opt(&mut out, "--daemon-url", self.daemon_url.as_deref());
        push_flag(&mut out, "--inference", self.inference);
        out
    }
}

impl PassthroughArgs {
    pub fn to_legacy(&self) -> Vec<String> {
        let mut out = Vec::new();
        push_opt(&mut out, "--backend", self.backend.as_deref());
        push_opt(&mut out, "--bind", self.bind.as_deref());
        push_flag(&mut out, "--public", self.public);
        push_opt(&mut out, "--daemon-url", self.daemon_url.as_deref());
        push_flag(&mut out, "--inference", self.inference);
        push_flag(&mut out, "--no-daemon", self.no_daemon);
        out.extend(self.rest.clone());
        out
    }
}

impl AdapterPassthroughArgs {
    pub fn to_legacy(&self) -> Vec<String> {
        let mut out = Vec::new();
        push_opt(&mut out, "--daemon-url", self.daemon_url.as_deref());
        push_opt(&mut out, "--token", self.token.as_deref());
        out.extend(self.rest.clone());
        out
    }
}

impl TelegramArgs {
    pub fn to_legacy(&self) -> Vec<String> {
        let mut out = Vec::new();
        push_opt(&mut out, "--daemon-url", self.daemon_url.as_deref());
        push_opt(&mut out, "--token", self.token.as_deref());
        push_opt(&mut out, "--allow-user-ids", self.allow_user_ids.as_deref());
        out.extend(self.rest.clone());
        out
    }
}

impl SlackArgs {
    pub fn to_legacy(&self) -> Vec<String> {
        let mut out = Vec::new();
        push_opt(&mut out, "--daemon-url", self.daemon_url.as_deref());
        push_opt(&mut out, "--bot-token", self.bot_token.as_deref());
        push_opt(&mut out, "--app-token", self.app_token.as_deref());
        out.extend(self.rest.clone());
        out
    }
}

impl WhatsappArgs {
    pub fn to_legacy(&self) -> Vec<String> {
        let mut out = Vec::new();
        push_opt(&mut out, "--daemon-url", self.daemon_url.as_deref());
        push_opt(&mut out, "--deliver-bind", self.deliver_bind.as_deref());
        out.extend(self.rest.clone());
        out
    }
}

impl DoctorArgs {
    pub fn to_legacy(&self) -> Vec<String> {
        let mut out = Vec::new();
        push_flag(&mut out, "--config", self.config);
        push_flag(&mut out, "--json", self.json);
        push_flag(&mut out, "--local-engine", self.local_engine);
        out
    }
}

impl StopArgs {
    pub fn to_legacy(&self) -> Vec<String> {
        let mut out = Vec::new();
        push_flag(&mut out, "--local-engine", self.local_engine);
        push_flag(&mut out, "--all", self.all);
        out
    }
}

impl IdentityExportArgs {
    pub fn to_legacy(&self) -> Vec<String> {
        let mut out = Vec::new();
        push_opt(&mut out, "--user-id", self.user_id.as_deref());
        push_opt(&mut out, "--dir", self.dir.as_deref());
        out
    }
}

impl IdentityRememberArgs {
    pub fn to_legacy(&self) -> Vec<String> {
        let mut out = Vec::new();
        push_opt(&mut out, "--kind", Some(self.kind.as_str()));
        push_opt(&mut out, "--subject", Some(self.subject.as_str()));
        push_opt(&mut out, "--statement", Some(self.statement.as_str()));
        push_opt(&mut out, "--source", Some(self.source.as_str()));
        push_opt(&mut out, "--user-id", self.user_id.as_deref());
        push_opt(&mut out, "--attributes", self.attributes.as_deref());
        out
    }
}

impl SkillImportArgs {
    pub fn to_legacy(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(path) = &self.path {
            out.push(path.clone());
        }
        push_flag(&mut out, "--project", self.project);
        push_flag(&mut out, "--force", self.force);
        if self.no_extends {
            push_flag(&mut out, "--no-extends", true);
        } else {
            push_opt(&mut out, "--extends", Some(self.extends.as_str()));
        }
        push_flag(&mut out, "--from-hermes", self.from_hermes);
        push_flag(&mut out, "--from-openclaw", self.from_openclaw);
        push_flag(&mut out, "--from-cursor", self.from_cursor);
        out
    }
}

impl OpenshellProbeArgs {
    pub fn to_legacy(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(id) = &self.manuscript_id_pos {
            out.push(id.clone());
        }
        push_opt(&mut out, "--from", self.from.as_deref());
        push_opt(&mut out, "--policy", self.policy.as_deref());
        push_flag(&mut out, "--skip-grapheme", self.skip_grapheme);
        push_opt(&mut out, "--manuscript-id", self.manuscript_id.as_deref());
        push_opt(&mut out, "--script", self.script.as_deref());
        out
    }
}

impl PairArgs {
    pub fn to_legacy(&self) -> Vec<String> {
        let mut out = Vec::new();
        push_opt(&mut out, "--daemon-url", self.daemon_url.as_deref());
        match &self.command {
            None | Some(PairCommand::Status) | Some(PairCommand::List) => {
                out.insert(0, "status".into());
            }
            Some(PairCommand::Qr { term, open, full }) => {
                out.insert(0, "qr".into());
                push_flag(&mut out, "--term", *term);
                push_flag(&mut out, "--open", *open);
                push_flag(&mut out, "--full", *full);
            }
            Some(PairCommand::Remove {
                pairing_id,
                daemon_url,
            }) => {
                out.insert(0, "remove".into());
                out.push(pairing_id.clone());
                push_opt(&mut out, "--daemon-url", daemon_url.as_deref());
            }
            Some(PairCommand::Lan { action }) => {
                out.insert(0, "lan".into());
                out.push(match action {
                    PairLanAction::Status => "status".into(),
                    PairLanAction::On => "on".into(),
                    PairLanAction::Off => "off".into(),
                });
            }
        }
        out
    }
}

impl PeerArgs {
    pub fn to_legacy(&self) -> Vec<String> {
        let mut out = Vec::new();
        match &self.command {
            None => {
                // No subcommand → peer prints help (do not put flags first).
                return out;
            }
            Some(PeerCommand::Nearby { portal }) => {
                push_opt(&mut out, "--daemon-url", self.daemon_url.as_deref());
                out.insert(0, "nearby".into());
                push_flag(&mut out, "--portal", *portal);
            }
            Some(PeerCommand::Connect { daemon_url, name }) => {
                push_opt(&mut out, "--daemon-url", self.daemon_url.as_deref());
                out.insert(0, "connect".into());
                out.push(daemon_url.clone());
                push_opt(&mut out, "--name", name.as_deref());
            }
            Some(PeerCommand::List) => {
                push_opt(&mut out, "--daemon-url", self.daemon_url.as_deref());
                out.insert(0, "list".into());
            }
            Some(PeerCommand::Remove { id_or_label }) => {
                push_opt(&mut out, "--daemon-url", self.daemon_url.as_deref());
                out.insert(0, "remove".into());
                out.push(id_or_label.clone());
            }
            Some(PeerCommand::Send {
                id_or_label,
                message,
            }) => {
                push_opt(&mut out, "--daemon-url", self.daemon_url.as_deref());
                out.insert(0, "send".into());
                out.push(id_or_label.clone());
                out.extend(message.clone());
            }
            Some(PeerCommand::Inbox { unread }) => {
                push_opt(&mut out, "--daemon-url", self.daemon_url.as_deref());
                out.insert(0, "inbox".into());
                push_flag(&mut out, "--unread", *unread);
            }
            Some(PeerCommand::Read { message_id }) => {
                push_opt(&mut out, "--daemon-url", self.daemon_url.as_deref());
                out.insert(0, "read".into());
                out.push(message_id.clone());
            }
        }
        out
    }
}

impl PullArgs {
    pub fn to_legacy(&self) -> Vec<String> {
        let mut out = vec![self.name.clone()];
        push_flag(&mut out, "--json", self.json);
        out
    }
}

impl UpdateArgs {
    pub fn to_legacy(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(name) = &self.name {
            out.push(name.clone());
        }
        push_flag(&mut out, "--json", self.json);
        out
    }
}

impl PackagesArgs {
    pub fn to_legacy(&self) -> Vec<String> {
        let mut out = Vec::new();
        match self.command {
            None | Some(PackagesCommand::List) => out.push("list".into()),
            Some(PackagesCommand::Status) => out.push("status".into()),
        }
        push_flag(&mut out, "--json", self.json);
        out
    }
}

impl ModelsArgs {
    pub fn to_legacy(&self) -> Vec<String> {
        let mut out = self.rest.clone();
        push_opt(&mut out, "--daemon-url", self.daemon_url.as_deref());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn update_help_does_not_parse_as_update_all() {
        let err = Cli::try_parse_from(["medousa", "update", "--help"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayHelp);
    }

    #[test]
    fn unknown_flag_errors() {
        let err = Cli::try_parse_from(["medousa", "status", "--deamon-url", "x"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::UnknownArgument);
    }

    #[test]
    fn pair_daemon_url_without_subcommand_defaults_to_status() {
        let cli = Cli::try_parse_from(["medousa", "pair", "--daemon-url", "http://127.0.0.1:7419"])
            .expect("parse");
        match cli.command {
            Some(Commands::Pair(args)) => {
                assert!(args.command.is_none());
                let legacy = args.to_legacy();
                assert_eq!(legacy.first().map(String::as_str), Some("status"));
                assert!(legacy.iter().any(|a| a == "--daemon-url"));
            }
            other => panic!("expected Pair, got {other:?}"),
        }
    }

    #[test]
    fn packages_help_is_help_not_list() {
        let err = Cli::try_parse_from(["medousa", "packages", "--help"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayHelp);
    }

    #[test]
    fn credentials_subcommand_preserves_operator_arguments() {
        let cli = Cli::try_parse_from([
            "medousa",
            "credentials",
            "rotate",
            "medousa-cli",
            "--daemon-url",
            "http://127.0.0.1:7419",
        ])
        .expect("parse");
        match cli.command {
            Some(Commands::Credentials(args)) => {
                assert_eq!(
                    args.rest,
                    [
                        "rotate",
                        "medousa-cli",
                        "--daemon-url",
                        "http://127.0.0.1:7419"
                    ]
                );
            }
            other => panic!("expected Credentials, got {other:?}"),
        }
    }

    #[test]
    fn session_storage_is_dry_run_unless_apply_is_explicit() {
        let dry_run = Cli::try_parse_from(["medousa", "session-storage"]).unwrap();
        match dry_run.command {
            Some(Commands::SessionStorage(args)) => assert!(!args.apply),
            other => panic!("expected SessionStorage, got {other:?}"),
        }
        let apply =
            Cli::try_parse_from(["medousa", "session-storage", "--apply", "--json"]).unwrap();
        match apply.command {
            Some(Commands::SessionStorage(args)) => {
                assert!(args.apply);
                assert!(args.json);
            }
            other => panic!("expected SessionStorage, got {other:?}"),
        }
    }

    #[cfg(not(feature = "iroh-transport"))]
    #[test]
    fn iroh_absent_without_feature() {
        let err = Cli::try_parse_from(["medousa", "iroh", "--help"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::InvalidSubcommand);
    }
}
