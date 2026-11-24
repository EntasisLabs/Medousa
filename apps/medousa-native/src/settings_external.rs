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

/// WhatsApp's linked-devices pairing, its QR drawn for showing.
#[derive(Clone, Debug)]
pub(crate) struct Pairing {
    pub state: medousa_types::ExternalWhatsAppPairingState,
    /// Width, height and RGBA, only while the QR is current.
    pub qr: Option<(u32, u32, Vec<u8>)>,
}

/// QR side in pixels, drawn on a white mat.
pub(crate) const QR_SIZE: u32 = 220;

pub(crate) fn pairing(status: medousa_types::ExternalWhatsAppPairingStatus) -> Pairing {
    let current = status.expires_at.is_some_and(|at| at > chrono::Utc::now());
    let qr = status
        .qr_svg
        .filter(|_| current && status.state == medousa_types::ExternalWhatsAppPairingState::QrReady)
        .and_then(|svg| rndr_render::decode_svg(svg.as_bytes(), QR_SIZE, QR_SIZE).ok())
        .map(|r| (r.width, r.height, r.rgba));
    Pairing {
        state: status.state,
        qr,
    }
}

/// One read of the WhatsApp pairing, and whether starting the adapter
/// failed.
#[derive(Clone, Debug)]
pub(crate) struct PairingRead {
    pub pairing: Result<Pairing, String>,
    pub adapter_error: Option<String>,
}

/// The Add form for one provider.
#[derive(Clone, Debug)]
pub(crate) struct ConvForm {
    pub provider: ExternalProvider,
    pub pairing: Option<PairingRead>,
    pub pairing_reading: Option<u64>,
    pub discovery: Option<medousa_types::ExternalMuseDiscoveryStatus>,
    pub discovery_reading: Option<u64>,
    pub saving: Option<u64>,
    pub note: String,
    pub next_poll: std::time::Instant,
}

impl ConvForm {
    pub(crate) fn new(provider: ExternalProvider) -> Self {
        Self {
            provider,
            pairing: None,
            pairing_reading: None,
            discovery: None,
            discovery_reading: None,
            saving: None,
            note: String::new(),
            next_poll: std::time::Instant::now(),
        }
    }

    /// Muse and Instinct reach Medousa through the linked WhatsApp.
    pub(crate) fn whatsapp(&self) -> bool {
        matches!(
            self.provider,
            ExternalProvider::Muse | ExternalProvider::Instinct
        )
    }
}

pub(crate) const CONV_FIELDS: [&str; 6] = [
    "conv.label",
    "conv.target",
    "conv.webhook_url",
    "conv.webhook_key",
    "conv.dot_user",
    "conv.slack_token",
];

/// medousa-home's `create`: what each provider needs before it connects.
pub(crate) fn request(
    form: &ConvForm,
    drafts: &std::collections::HashMap<&'static str, String>,
) -> Result<medousa_types::CreateExternalConversationRequest, String> {
    let d = |k: &str| {
        drafts
            .get(k)
            .map(|v| v.trim().to_string())
            .unwrap_or_default()
    };
    let label = d("conv.label");
    if label.is_empty() {
        return Err("Give it a name.".into());
    }
    let required = |value: String, what: &str| {
        if value.is_empty() {
            Err(format!("Enter {what}."))
        } else {
            Ok(value)
        }
    };
    let mut req = medousa_types::CreateExternalConversationRequest {
        provider: form.provider,
        label,
        target: String::new(),
        webhook_url: None,
        webhook_key: None,
        slack_user_token: None,
        dot_user_id: None,
    };
    match form.provider {
        ExternalProvider::Muse => {
            req.target = form
                .discovery
                .as_ref()
                .and_then(|d| d.observed_chat_jid.clone())
                .ok_or("Find the Muse chat first.")?;
        }
        ExternalProvider::Instinct => {
            req.target = required(d("conv.target"), "Instinct’s WhatsApp number")?;
        }
        ExternalProvider::Dots => {
            req.target = required(d("conv.target"), "the Slack channel ID")?;
            req.dot_user_id = Some(required(d("conv.dot_user"), "the dot’s Slack ID")?);
            req.slack_user_token = Some(required(d("conv.slack_token"), "your Slack user token")?);
        }
        ExternalProvider::GrokBot => {
            req.target = required(d("conv.target"), "the bot name")?;
            req.webhook_url = Some(required(d("conv.webhook_url"), "the webhook URL")?);
            req.webhook_key = Some(required(d("conv.webhook_key"), "the webhook key")?);
        }
    }
    Ok(req)
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

    #[test]
    fn a_conversation_needs_what_medousa_home_asks_for() {
        let mut drafts = std::collections::HashMap::new();
        let grok = ConvForm::new(ExternalProvider::GrokBot);
        assert_eq!(request(&grok, &drafts).unwrap_err(), "Give it a name.");
        drafts.insert("conv.label", " Ops ".to_string());
        drafts.insert("conv.target", "ops-bot".to_string());
        drafts.insert("conv.webhook_url", "https://x.ai/hook".to_string());
        assert_eq!(
            request(&grok, &drafts).unwrap_err(),
            "Enter the webhook key."
        );
        drafts.insert("conv.webhook_key", "k".to_string());
        let r = request(&grok, &drafts).unwrap();
        assert_eq!((r.label.as_str(), r.target.as_str()), ("Ops", "ops-bot"));
        assert_eq!(r.webhook_key.as_deref(), Some("k"));
        assert!(r.slack_user_token.is_none());

        let mut muse = ConvForm::new(ExternalProvider::Muse);
        assert!(muse.whatsapp());
        assert_eq!(
            request(&muse, &drafts).unwrap_err(),
            "Find the Muse chat first."
        );
        muse.discovery = Some(medousa_types::ExternalMuseDiscoveryStatus {
            challenge: "MDS-1".into(),
            observed_chat_jid: Some("123@g.us".into()),
            expires_at: chrono::Utc::now(),
        });
        assert_eq!(request(&muse, &drafts).unwrap().target, "123@g.us");
    }

    #[test]
    fn an_expired_or_missing_qr_is_not_drawn() {
        use medousa_types::{ExternalWhatsAppPairingState as S, ExternalWhatsAppPairingStatus};
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 2 2"><rect width="1" height="1"/></svg>"#;
        let fresh = pairing(ExternalWhatsAppPairingStatus {
            state: S::QrReady,
            qr_svg: Some(svg.into()),
            expires_at: Some(chrono::Utc::now() + chrono::Duration::seconds(30)),
        });
        let (w, h, rgba) = fresh.qr.unwrap();
        assert_eq!(
            (w, h, rgba.len()),
            (QR_SIZE, QR_SIZE, (QR_SIZE * QR_SIZE * 4) as usize)
        );
        let stale = pairing(ExternalWhatsAppPairingStatus {
            state: S::QrReady,
            qr_svg: Some(svg.into()),
            expires_at: Some(chrono::Utc::now() - chrono::Duration::seconds(1)),
        });
        assert!(stale.qr.is_none());
    }
}
