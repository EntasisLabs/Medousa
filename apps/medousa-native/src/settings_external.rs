//! External Agents: coding agents the workshop can run (from its agents
//! surface) and conversations hosted by other providers. Setting an agent up
//! (install, sign in, sign out) runs the vendor's own CLI, so it is only
//! offered when the workshop runs on this machine.
use medousa_types::{AgentRuntimeInfo, ExternalConversationView, ExternalProvider};

/// One agent medousa-home knows, in its order.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Agent {
    /// The runtime id the workshop reports.
    pub runtime: &'static str,
    /// The account id setup actions take.
    pub account: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    pub cli: &'static str,
    /// Medousa can install and sign in to this one; the rest are set up on
    /// the workshop host by hand.
    pub managed: bool,
    pub sign_out: Option<&'static str>,
}

const fn agent(
    runtime: &'static str,
    account: &'static str,
    label: &'static str,
    hint: &'static str,
    cli: &'static str,
    sign_out: Option<&'static str>,
) -> Agent {
    Agent {
        runtime,
        account,
        label,
        hint,
        cli,
        managed: sign_out.is_some(),
        sign_out,
    }
}

/// medousa-home's `EXTERNAL_AGENTS`.
pub(crate) const AGENTS: [Agent; 8] = [
    agent(
        "codex",
        "chatgpt",
        "Codex",
        "ChatGPT-backed agent + chat",
        "Codex CLI",
        Some("Sign out"),
    ),
    agent(
        "cursor",
        "cursor",
        "Cursor",
        "External Cursor agent",
        "Cursor Agent CLI",
        Some("Sign out"),
    ),
    agent(
        "hermes",
        "hermes",
        "Hermes",
        "External Hermes agent",
        "Hermes Agent CLI",
        Some("Reconfigure"),
    ),
    agent(
        "claude",
        "claude",
        "Claude Code",
        "Your Claude Code agent",
        "Claude Code CLI",
        None,
    ),
    agent(
        "devin",
        "devin",
        "Devin",
        "Your Devin CLI agent",
        "Devin CLI",
        None,
    ),
    agent("pi", "pi", "Pi", "Your Pi coding agent", "Pi CLI", None),
    agent(
        "muse_code",
        "muse_code",
        "Muse Code",
        "Your Muse Code agent",
        "Muse Code CLI",
        None,
    ),
    agent(
        "hades",
        "hades",
        "Hades",
        "Your Hades agent",
        "Hades CLI",
        None,
    ),
];

pub(crate) fn by_account(account: &str) -> Option<&'static Agent> {
    AGENTS.iter().find(|a| a.account == account)
}

/// What the workshop reports, and its hosted conversations.
#[derive(Clone, Debug, Default)]
pub(crate) struct Agents {
    pub runtimes: Vec<AgentRuntimeInfo>,
    pub conversations: Vec<ExternalConversationView>,
    /// Conversations couldn't be read; agents still show.
    pub conversations_error: Option<String>,
}

/// An agent's state as shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    /// The workshop has no such CLI.
    Missing,
    SignedIn,
    SignedOut,
    /// Installed; the CLI doesn't say.
    Unknown,
}

impl Agents {
    pub(crate) fn runtime(&self, agent: &Agent) -> Option<&AgentRuntimeInfo> {
        self.runtimes.iter().find(|r| r.runtime == agent.runtime)
    }

    pub(crate) fn status(&self, agent: &Agent) -> Status {
        let Some(info) = self.runtime(agent) else {
            return Status::Missing;
        };
        if !(info.binary_present || info.available) {
            return Status::Missing;
        }
        match info.auth_status.as_deref() {
            Some("signed_in") => Status::SignedIn,
            Some("signed_out") => Status::SignedOut,
            _ => Status::Unknown,
        }
    }

    /// Ready to pick in Chat.
    pub(crate) fn ready(&self, agent: &Agent) -> bool {
        self.runtime(agent).is_some_and(|r| r.available) && self.status(agent) != Status::SignedOut
    }

    pub(crate) fn detail(&self, agent: &Agent) -> String {
        self.runtime(agent)
            .and_then(|r| r.auth_detail.clone().or(r.detail.clone()))
            .unwrap_or_else(|| "This workshop does not report this runtime.".into())
    }

    pub(crate) fn of(&self, provider: ExternalProvider) -> Vec<&ExternalConversationView> {
        self.conversations
            .iter()
            .filter(|c| c.provider == provider)
            .collect()
    }
}

/// The providers medousa-home lists, with how each is described.
pub(crate) const PROVIDERS: [(ExternalProvider, &str, &str); 4] = [
    (ExternalProvider::Muse, "Muse", "WhatsApp sessions"),
    (
        ExternalProvider::Instinct,
        "Instinct Agent",
        "WhatsApp sessions",
    ),
    (ExternalProvider::Dots, "Dots", "Slack conversations"),
    (
        ExternalProvider::GrokBot,
        "Grok Bot",
        "Webhook bots and routines",
    ),
];

/// Where a conversation is, as medousa-home words it.
pub(crate) fn place(c: &ExternalConversationView) -> String {
    match c.provider {
        ExternalProvider::Muse => "Session".into(),
        ExternalProvider::Instinct => format!(
            "WhatsApp · +{}",
            c.target.split('@').next().unwrap_or_default()
        ),
        ExternalProvider::Dots => format!("Slack · {}", c.target),
        ExternalProvider::GrokBot => format!("Bot · {}", c.target),
    }
}

/// How the sign-in hint after an agent signs in reads.
pub(crate) fn use_hint(agent: &Agent) -> String {
    format!(
        "Ready. In Chat, pick {} from the runtime control under the composer.",
        agent.label
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime(id: &str, present: bool, auth: Option<&str>) -> AgentRuntimeInfo {
        serde_json::from_value(serde_json::json!({
            "runtime": id,
            "available": present,
            "binary_present": present,
            "auth_status": auth,
        }))
        .unwrap()
    }

    #[test]
    fn status_reads_what_the_workshop_reports() {
        let agents = Agents {
            runtimes: vec![
                runtime("codex", true, Some("signed_in")),
                runtime("cursor", true, Some("signed_out")),
                runtime("hermes", false, None),
                runtime("claude", true, None),
            ],
            ..Default::default()
        };
        let s = |account| agents.status(by_account(account).unwrap());
        assert_eq!(s("chatgpt"), Status::SignedIn);
        assert_eq!(s("cursor"), Status::SignedOut);
        assert_eq!(s("hermes"), Status::Missing);
        assert_eq!(s("claude"), Status::Unknown);
        assert_eq!(s("devin"), Status::Missing, "not reported is not installed");
        assert!(agents.ready(by_account("claude").unwrap()));
        assert!(!agents.ready(by_account("cursor").unwrap()));
    }
}
