//! One-sentence mood tags for memory recall and worker continuity.

use chrono::Timelike;
use serde::{Deserialize, Serialize};
use stasis::ports::outbound::memory::memory_models::MemoryAvecState;

use crate::daemon_api::TurnSurfaceContext;

use super::ambient_context::{
    AmbientContextInput, ChannelAmbientPolicy, build_ambient_context, operator_zoned_now,
    resolve_operator_timezone_label,
};

/// Serializable snapshot of host model avec for persisted worker handoff capsules.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct HandoffModelAvec {
    pub stability: f32,
    pub friction: f32,
    pub logic: f32,
    pub autonomy: f32,
}

impl From<MemoryAvecState> for HandoffModelAvec {
    fn from(value: MemoryAvecState) -> Self {
        Self {
            stability: value.stability,
            friction: value.friction,
            logic: value.logic,
            autonomy: value.autonomy,
        }
    }
}

impl From<HandoffModelAvec> for MemoryAvecState {
    fn from(value: HandoffModelAvec) -> Self {
        Self {
            stability: value.stability,
            friction: value.friction,
            logic: value.logic,
            autonomy: value.autonomy,
        }
    }
}

pub fn default_handoff_model_avec() -> MemoryAvecState {
    MemoryAvecState {
        stability: 0.88,
        friction: 0.24,
        logic: 0.94,
        autonomy: 0.84,
    }
}

fn daypart_label(hour: u32) -> &'static str {
    match hour {
        5..=11 => "Morning",
        12..=16 => "Afternoon",
        17..=21 => "Evening",
        _ => "Night",
    }
}

fn surface_tone_phrase(surface: Option<&TurnSurfaceContext>) -> &'static str {
    let Some(channel) = surface
        .and_then(|ctx| ctx.channel_surface.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return "focused operator-console";
    };

    match channel.to_ascii_lowercase().as_str() {
        "telegram" | "whatsapp" => "concise mobile",
        "discord" | "slack" => "threaded collaborative",
        "cli" => "scriptable terse",
        "tui" => "operator-console",
        "api" => "integration-neutral",
        "home" | "home-desktop" => "workshop desktop",
        "home-ios" | "home-android" => "workshop mobile",
        _ => "neutral channel",
    }
}

/// Derive a one-line vibe tag for STTP storage and worker handoff capsules.
pub fn derive_vibe_signature(
    session_id: &str,
    surface: Option<&TurnSurfaceContext>,
    channel_policy: Option<&ChannelAmbientPolicy>,
    model_avec: &MemoryAvecState,
) -> String {
    let _ambient = build_ambient_context(AmbientContextInput {
        session_id,
        surface,
        channel_policy,
    });
    let zoned = operator_zoned_now();
    let daypart = daypart_label(zoned.hour());
    let tone = surface_tone_phrase(surface);
    let tz = resolve_operator_timezone_label();

    let energy = if model_avec.friction < 0.28 && model_avec.stability > 0.82 {
        "warm builder momentum"
    } else if model_avec.logic > 0.92 {
        "precise diagnostic focus"
    } else {
        "steady partnership energy"
    };

    format!(
        "{daypart} {tone} ({tz}) — {energy}; stability {:.0}% logic {:.0}% autonomy {:.0}%",
        model_avec.stability * 100.0,
        model_avec.logic * 100.0,
        model_avec.autonomy * 100.0,
    )
}
