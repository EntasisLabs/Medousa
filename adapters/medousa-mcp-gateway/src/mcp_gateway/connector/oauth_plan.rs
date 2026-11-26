//! Pure OAuth parameter construction for a connector.
//!
//! The broker already speaks pre-registered clients, CIMD, and DCR. This module
//! decides which of those to ask for, which scopes to send, and when Google
//! needs `access_type=offline`. It never embeds a client secret in a URL.

use std::collections::HashMap;

use super::{ConnectorAuth, ConnectorPlatform, OAuthClientMode};

#[derive(Clone, Default, PartialEq, Eq)]
pub struct SuppliedOAuthClient {
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub client_metadata_url: Option<String>,
}

/// What Home and the gateway should pass into the OAuth broker.
#[derive(Clone, PartialEq, Eq)]
pub struct OAuthBeginPlan {
    pub mode: OAuthClientMode,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub client_metadata_url: Option<String>,
    pub scopes: Vec<String>,
    pub access_type_offline: bool,
    /// Public PKCE clients (Desktop, iOS, Android) use the native application type.
    pub application_type: &'static str,
    /// `None` on desktop: the host binds a loopback port. Mobile uses a fixed scheme.
    pub redirect_uri: Option<String>,
    pub google_agent_client: bool,
}

impl std::fmt::Debug for OAuthBeginPlan {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OAuthBeginPlan")
            .field("mode", &self.mode)
            .field("client_id", &self.client_id)
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| "<redacted>"),
            )
            .field("client_metadata_url", &self.client_metadata_url)
            .field("scopes", &self.scopes)
            .field("access_type_offline", &self.access_type_offline)
            .field("application_type", &self.application_type)
            .field("redirect_uri", &self.redirect_uri)
            .field("google_agent_client", &self.google_agent_client)
            .finish()
    }
}

pub fn plan_oauth_begin(
    auth: &ConnectorAuth,
    platform: ConnectorPlatform,
    supplied: &SuppliedOAuthClient,
) -> Result<OAuthBeginPlan, String> {
    if !auth.kind.eq_ignore_ascii_case("oauth") {
        return Err("this connector does not use OAuth".to_string());
    }
    let modes = parse_modes(&auth.clients)?;
    if modes.is_empty() {
        return Err("connector auth.clients is empty".to_string());
    }
    let selected = select_mode(auth, platform, supplied, &modes)?;
    let scopes =
        scopes_for_authorization(&auth.scopes, auth.offline_access, auth.access_type_offline);
    Ok(OAuthBeginPlan {
        mode: selected.mode,
        client_id: selected.client_id,
        client_secret: selected.client_secret,
        client_metadata_url: selected.client_metadata_url,
        scopes,
        access_type_offline: auth.access_type_offline,
        application_type: "native",
        redirect_uri: mobile_redirect_uri(platform).map(str::to_string),
        google_agent_client: auth.google_agent_client,
    })
}

struct SelectedMode {
    mode: OAuthClientMode,
    client_id: Option<String>,
    client_secret: Option<String>,
    client_metadata_url: Option<String>,
}

fn select_mode(
    auth: &ConnectorAuth,
    platform: ConnectorPlatform,
    supplied: &SuppliedOAuthClient,
    modes: &[OAuthClientMode],
) -> Result<SelectedMode, String> {
    let supplied_id = non_empty(supplied.client_id.clone());
    let allows_user_client =
        modes.contains(&OAuthClientMode::Byo) || modes.contains(&OAuthClientMode::Preregistered);
    if let Some(client_id) = supplied_id.clone()
        && allows_user_client
    {
        let mode = if modes.contains(&OAuthClientMode::Byo) {
            OAuthClientMode::Byo
        } else {
            OAuthClientMode::Preregistered
        };
        return Ok(SelectedMode {
            mode,
            client_id: Some(client_id),
            client_secret: non_empty(supplied.client_secret.clone()),
            client_metadata_url: None,
        });
    }

    if modes.contains(&OAuthClientMode::Preregistered)
        && let Some(client_id) = catalog_client_id(auth, platform)
    {
        return Ok(SelectedMode {
            mode: OAuthClientMode::Preregistered,
            client_id: Some(client_id),
            client_secret: None,
            client_metadata_url: None,
        });
    }

    let metadata = non_empty(supplied.client_metadata_url.clone())
        .or_else(|| non_empty(auth.client_metadata_url.clone()));
    if modes.contains(&OAuthClientMode::Cimd)
        && let Some(metadata) = metadata
    {
        return Ok(SelectedMode {
            mode: OAuthClientMode::Cimd,
            client_id: None,
            client_secret: None,
            client_metadata_url: Some(metadata),
        });
    }

    if modes.contains(&OAuthClientMode::Dcr) {
        return Ok(SelectedMode {
            mode: OAuthClientMode::Dcr,
            client_id: None,
            client_secret: None,
            client_metadata_url: None,
        });
    }

    if modes.contains(&OAuthClientMode::Byo) || modes.contains(&OAuthClientMode::Preregistered) {
        let hint = if auth.google_agent_client {
            " Paste a Desktop, iOS, or Android OAuth client id flagged as an AI-agent client. Medousa does not ship a Google client secret."
        } else {
            " This vendor does not allow anonymous registration. Paste the client id from your own app, or wait for an Entasis-registered client."
        };
        return Err(format!("this connector needs an OAuth client id.{hint}"));
    }

    Err("no usable OAuth client mode".to_string())
}

fn catalog_client_id(auth: &ConnectorAuth, platform: ConnectorPlatform) -> Option<String> {
    let platform_key = match platform {
        ConnectorPlatform::Desktop => "desktop",
        ConnectorPlatform::Ios => "ios",
        ConnectorPlatform::Android => "android",
    };
    non_empty(auth.client_ids.get(platform_key).cloned())
        .or_else(|| non_empty(auth.client_id.clone()))
}

pub fn parse_modes(raw: &[String]) -> Result<Vec<OAuthClientMode>, String> {
    raw.iter()
        .filter_map(|mode| {
            let trimmed = mode.trim();
            if trimmed.is_empty() {
                return None;
            }
            Some(match trimmed.to_ascii_lowercase().as_str() {
                "preregistered" => Ok(OAuthClientMode::Preregistered),
                "cimd" => Ok(OAuthClientMode::Cimd),
                "dcr" => Ok(OAuthClientMode::Dcr),
                "byo" => Ok(OAuthClientMode::Byo),
                other => Err(format!("unknown OAuth client mode '{other}'")),
            })
        })
        .collect()
}

/// Scopes sent to the broker.
///
/// An empty catalog list stays empty so dynamic registration can adopt the
/// authorization server's own scopes. `offline_access` is appended only when
/// the connector already named scopes, or when it is a Google-style client that
/// also sets `access_type=offline`. Sending `offline_access` as the only scope
/// would hide the server's real scopes.
pub fn scopes_for_authorization(
    scopes: &[String],
    offline_access: bool,
    access_type_offline: bool,
) -> Vec<String> {
    let mut scopes = scopes
        .iter()
        .map(|scope| scope.trim().to_string())
        .filter(|scope| !scope.is_empty())
        .collect::<Vec<_>>();
    let append = offline_access && (!scopes.is_empty() || access_type_offline);
    if append && !scopes.iter().any(|scope| scope == "offline_access") {
        scopes.push("offline_access".to_string());
    }
    scopes
}

pub fn mobile_redirect_uri(platform: ConnectorPlatform) -> Option<&'static str> {
    match platform {
        ConnectorPlatform::Desktop => None,
        ConnectorPlatform::Ios | ConnectorPlatform::Android => Some(MOBILE_OAUTH_REDIRECT),
    }
}

pub const MOBILE_OAUTH_REDIRECT: &str = "medousa://oauth/callback";

/// Google issues a refresh token when the authorization URL carries
/// `access_type=offline`. Desktop and mobile clients here are public PKCE
/// clients; the extra parameter is still required for Google's web and
/// installed-app token behavior.
pub fn with_access_type_offline(authorization_url: &str) -> String {
    let Ok(mut url) = reqwest::Url::parse(authorization_url) else {
        return authorization_url.to_string();
    };
    if url.query_pairs().any(|(key, _)| key == "access_type") {
        return url.to_string();
    }
    url.query_pairs_mut().append_pair("access_type", "offline");
    url.to_string()
}

pub fn platform_client_ids(ids: HashMap<String, String>) -> HashMap<String, String> {
    ids.into_iter()
        .filter_map(|(key, value)| {
            let key = key.trim().to_ascii_lowercase();
            let value = value.trim().to_string();
            if value.is_empty() {
                None
            } else {
                Some((key, value))
            }
        })
        .collect()
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp_gateway::connector::{ConnectorAuth, ConnectorPlatform};

    fn google_auth() -> ConnectorAuth {
        ConnectorAuth {
            kind: "oauth".into(),
            clients: vec!["preregistered".into(), "byo".into()],
            scopes: vec![
                "https://www.googleapis.com/auth/gmail.readonly".into(),
                "https://www.googleapis.com/auth/gmail.compose".into(),
            ],
            client_metadata_url: None,
            client_id: None,
            client_ids: HashMap::new(),
            offline_access: true,
            access_type_offline: true,
            google_agent_client: true,
        }
    }

    fn dcr_auth() -> ConnectorAuth {
        ConnectorAuth {
            kind: "oauth".into(),
            clients: vec![
                "preregistered".into(),
                "cimd".into(),
                "dcr".into(),
                "byo".into(),
            ],
            scopes: Vec::new(),
            client_metadata_url: None,
            client_id: None,
            client_ids: HashMap::new(),
            offline_access: true,
            access_type_offline: false,
            google_agent_client: false,
        }
    }

    #[test]
    fn google_byo_client_requests_offline_access_and_hides_the_secret() {
        let plan = plan_oauth_begin(
            &google_auth(),
            ConnectorPlatform::Desktop,
            &SuppliedOAuthClient {
                client_id: Some("desktop-client.apps.googleusercontent.com".into()),
                client_secret: Some("super-secret".into()),
                client_metadata_url: None,
            },
        )
        .expect("plan");
        assert_eq!(plan.mode, OAuthClientMode::Byo);
        assert_eq!(
            plan.client_id.as_deref(),
            Some("desktop-client.apps.googleusercontent.com")
        );
        assert!(plan.access_type_offline);
        assert!(plan.scopes.iter().any(|scope| scope == "offline_access"));
        assert!(plan.scopes.len() > 1);
        assert_eq!(plan.application_type, "native");
        assert!(plan.redirect_uri.is_none());
        let rendered = format!("{plan:?}");
        assert!(rendered.contains("<redacted>"));
        assert!(!rendered.contains("super-secret"));
    }

    #[test]
    fn google_without_a_client_id_asks_the_user_instead_of_inventing_one() {
        let error = plan_oauth_begin(
            &google_auth(),
            ConnectorPlatform::Ios,
            &SuppliedOAuthClient::default(),
        )
        .expect_err("missing client");
        assert!(error.contains("client id"));
        assert!(error.to_ascii_lowercase().contains("ai-agent") || error.contains("AI-agent"));
    }

    #[test]
    fn dcr_vendor_keeps_scopes_empty_until_a_user_supplies_a_client() {
        let plan = plan_oauth_begin(
            &dcr_auth(),
            ConnectorPlatform::Android,
            &SuppliedOAuthClient::default(),
        )
        .expect("dcr");
        assert_eq!(plan.mode, OAuthClientMode::Dcr);
        assert!(plan.client_id.is_none());
        assert!(plan.scopes.is_empty());
        assert_eq!(
            plan.redirect_uri.as_deref(),
            Some("medousa://oauth/callback")
        );
    }

    #[test]
    fn preregistered_catalog_id_beats_dcr_and_cimd_follows_when_it_is_missing() {
        let mut auth = dcr_auth();
        auth.client_id = Some("entasis-linear".into());
        let plan = plan_oauth_begin(
            &auth,
            ConnectorPlatform::Desktop,
            &SuppliedOAuthClient::default(),
        )
        .expect("preregistered");
        assert_eq!(plan.mode, OAuthClientMode::Preregistered);
        assert_eq!(plan.client_id.as_deref(), Some("entasis-linear"));

        auth.client_id = None;
        auth.client_metadata_url = Some("https://entasis.example/medousa.json".into());
        let plan = plan_oauth_begin(
            &auth,
            ConnectorPlatform::Desktop,
            &SuppliedOAuthClient::default(),
        )
        .expect("cimd");
        assert_eq!(plan.mode, OAuthClientMode::Cimd);
        assert!(plan.client_id.is_none());
        assert_eq!(
            plan.client_metadata_url.as_deref(),
            Some("https://entasis.example/medousa.json")
        );
    }

    #[test]
    fn access_type_offline_is_added_once() {
        let url = with_access_type_offline(
            "https://accounts.google.com/o/oauth2/v2/auth?client_id=abc&code_challenge=xyz",
        );
        assert!(url.contains("access_type=offline"));
        assert_eq!(url.matches("access_type=").count(), 1);
        let again = with_access_type_offline(&url);
        assert_eq!(again.matches("access_type=").count(), 1);
    }

    #[test]
    fn platform_client_id_is_preferred_over_the_shared_id() {
        let mut auth = google_auth();
        auth.client_id = Some("shared".into());
        auth.client_ids.insert("ios".into(), "ios-client".into());
        let plan = plan_oauth_begin(
            &auth,
            ConnectorPlatform::Ios,
            &SuppliedOAuthClient::default(),
        )
        .expect("ios client");
        assert_eq!(plan.client_id.as_deref(), Some("ios-client"));
        assert_eq!(
            plan.redirect_uri.as_deref(),
            Some("medousa://oauth/callback")
        );
    }
}
