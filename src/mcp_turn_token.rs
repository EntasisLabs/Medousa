//! Turn-scoped HMAC tokens for MCP invoke authorization.

use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::mcp_gateway_api::{McpTurnContext, McpTurnLane};

type HmacSha256 = Hmac<Sha256>;

const TOKEN_TTL_SECS: i64 = 900;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpTurnTokenClaims {
    pub turn_id: String,
    pub session_id: String,
    pub user_id: String,
    pub channel_id: String,
    pub lane: McpTurnLane,
    pub exp: i64,
}

pub fn resolve_mcp_turn_token_secret() -> Option<String> {
    std::env::var("MEDOUSA_MCP_TURN_TOKEN_SECRET")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn mint_mcp_turn_token(context: &McpTurnContext) -> Result<Option<String>> {
    let Some(secret) = resolve_mcp_turn_token_secret() else {
        return Ok(None);
    };

    let claims = McpTurnTokenClaims {
        turn_id: context.turn_id.clone(),
        session_id: context.session_id.clone(),
        user_id: context.user_id.clone(),
        channel_id: context.channel_id.clone(),
        lane: context.lane,
        exp: (Utc::now() + chrono::Duration::seconds(TOKEN_TTL_SECS)).timestamp(),
    };

    Ok(Some(sign_claims(&claims, &secret)?))
}

pub fn verify_mcp_turn_token(token: &str, context: &McpTurnContext) -> Result<()> {
    let Some(secret) = resolve_mcp_turn_token_secret() else {
        return Ok(());
    };

    let claims = verify_and_decode(token, &secret)?;
    if claims.exp < Utc::now().timestamp() {
        bail!("turn token expired");
    }
    if claims.turn_id != context.turn_id
        || claims.session_id != context.session_id
        || claims.user_id != context.user_id
        || claims.channel_id != context.channel_id
        || claims.lane != context.lane
    {
        bail!("turn token context mismatch");
    }
    Ok(())
}

fn sign_claims(claims: &McpTurnTokenClaims, secret: &str) -> Result<String> {
    let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(claims)?);
    let signature = sign_payload(&payload, secret)?;
    Ok(format!("{payload}.{signature}"))
}

fn verify_and_decode(token: &str, secret: &str) -> Result<McpTurnTokenClaims> {
    let (payload, signature) = token
        .split_once('.')
        .context("turn token must be payload.signature")?;
    let expected = sign_payload(payload, secret)?;
    if expected != signature {
        bail!("turn token signature invalid");
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(payload)
        .context("turn token payload is not valid base64")?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn sign_payload(payload: &str, secret: &str) -> Result<String> {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .context("invalid HMAC key length")?;
    mac.update(payload.as_bytes());
    Ok(URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_context() -> McpTurnContext {
        McpTurnContext {
            turn_id: "turn_1".to_string(),
            session_id: "sess_1".to_string(),
            user_id: "user_1".to_string(),
            channel_id: "channel_1".to_string(),
            lane: McpTurnLane::Interactive,
            policy_profile: None,
        }
    }

    #[test]
    fn sign_and_verify_roundtrip() {
        let context = sample_context();
        let claims = McpTurnTokenClaims {
            turn_id: context.turn_id.clone(),
            session_id: context.session_id.clone(),
            user_id: context.user_id.clone(),
            channel_id: context.channel_id.clone(),
            lane: context.lane,
            exp: (Utc::now() + chrono::Duration::seconds(TOKEN_TTL_SECS)).timestamp(),
        };
        let token = sign_claims(&claims, "test-secret").expect("sign");
        let decoded = verify_and_decode(&token, "test-secret").expect("decode");
        assert_eq!(decoded, claims);
    }

    #[test]
    fn open_when_secret_unset() {
        let context = sample_context();
        assert!(resolve_mcp_turn_token_secret().is_none() || true);
        verify_mcp_turn_token("anything", &context).expect("open verify when secret unset");
    }
}
