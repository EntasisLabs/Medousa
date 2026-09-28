//! Principal-facing message effects emitted alongside assistant turns.

use serde::{Deserialize, Serialize};

/// Agent reactions initially target only the message that triggered the turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum MessageReactionTarget {
    CurrentUserMessage,
    MessageId(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ReactionIntent {
    pub target: MessageReactionTarget,
    pub emoji: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct MessageReaction {
    pub effect_id: String,
    pub target: MessageReactionTarget,
    pub emoji: String,
}

impl ReactionIntent {
    pub fn validate(&self) -> Result<(), &'static str> {
        let emoji = self.emoji.trim();
        if emoji.is_empty() {
            return Err("reaction emoji is empty");
        }
        if emoji.chars().any(char::is_whitespace) {
            return Err("reaction emoji must not contain whitespace");
        }
        if emoji.chars().count() > 8 {
            return Err("reaction emoji is too long");
        }
        Ok(())
    }
}

impl MessageReaction {
    pub fn from_intent(effect_id: impl Into<String>, intent: ReactionIntent) -> Option<Self> {
        intent.validate().ok()?;
        Some(Self {
            effect_id: effect_id.into(),
            target: intent.target,
            emoji: intent.emoji.trim().to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reaction_intents_normalize_and_reject_unsafe_labels() {
        let reaction = MessageReaction::from_intent(
            "turn:1:reaction:0",
            ReactionIntent {
                target: MessageReactionTarget::CurrentUserMessage,
                emoji: "  👍  ".to_string(),
            },
        )
        .expect("valid emoji");
        assert_eq!(reaction.emoji, "👍");
        assert!(
            ReactionIntent {
                target: MessageReactionTarget::CurrentUserMessage,
                emoji: "thumbs up".to_string(),
            }
            .validate()
            .is_err()
        );
    }
}
