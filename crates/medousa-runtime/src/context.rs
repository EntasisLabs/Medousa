//! Shared, payload-free context contracts. Domain adapters keep their own storage,
//! retrieval, trust boundaries and rendering. This phase preserves legacy selection;
//! a capacity diagnostic is NOT a guarantee that a provider request fits.

use genai::chat::{ChatMessage, ContentPart};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContextOwner {
    Chat,
    Tool,
    UnitOfWork,
    Host,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Retention {
    Required,
    Protected,
    Flexible,
}

/// Attribution only: a host-rendered data block is not execution authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextProvenance {
    pub source: String,
    pub scope: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextCost {
    pub text_chars: usize,
    /// Binary, URI and other opaque content is never treated as free text.
    pub unknown_parts: usize,
}

impl ContextCost {
    pub fn text(text_chars: usize) -> Self {
        Self {
            text_chars,
            unknown_parts: 0,
        }
    }

    pub fn messages(messages: &[ChatMessage]) -> Self {
        let mut cost = Self::default();
        for part in messages.iter().flat_map(|message| message.content.parts()) {
            match part {
                ContentPart::Text(text) | ContentPart::ReasoningContent(text) => {
                    cost.text_chars = cost.text_chars.saturating_add(text.chars().count());
                }
                // Tool protocols and media have provider-specific rendering. Do
                // not estimate base64 bytes as tokens or silently count zero.
                _ => cost.unknown_parts = cost.unknown_parts.saturating_add(1),
            }
        }
        cost
    }

    pub fn saturating_add(self, other: Self) -> Self {
        Self {
            text_chars: self.text_chars.saturating_add(other.text_chars),
            unknown_parts: self.unknown_parts.saturating_add(other.unknown_parts),
        }
    }

    pub fn tokens_estimate(self) -> u32 {
        u32::try_from(self.text_chars.div_ceil(4)).unwrap_or(u32::MAX)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContextDecision {
    Included,
    Projected { source_text_chars: usize },
    Excluded { source_text_chars: usize },
    Unattributed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextContribution {
    pub id: String,
    pub owner: ContextOwner,
    pub provenance: ContextProvenance,
    pub retention: Retention,
    pub decision: ContextDecision,
    pub cost: ContextCost,
    /// Existing producer cap, not a newly inferred model allocation.
    pub budget_chars: Option<usize>,
}

impl ContextContribution {
    /// Attribute an existing projection without rerendering or changing roles.
    pub fn project_text(mut self, selected_chars: usize) -> Self {
        let offered = match self.decision {
            ContextDecision::Projected { source_text_chars }
            | ContextDecision::Excluded { source_text_chars } => source_text_chars,
            _ => self.cost.text_chars,
        };
        self.cost.text_chars = selected_chars;
        self.decision = if selected_chars == 0 && offered > 0 {
            ContextDecision::Excluded {
                source_text_chars: offered,
            }
        } else if selected_chars < offered {
            ContextDecision::Projected {
                source_text_chars: offered,
            }
        } else {
            self.decision
        };
        self
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextCapacity {
    pub input_tokens: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextDecisionReport {
    pub contributions: Vec<ContextContribution>,
    pub capacity: ContextCapacity,
    pub cost: ContextCost,
    pub text_tokens_estimate: u32,
    /// Framing and media are not measured by chars/4, even with known capacity.
    pub complete_token_cost_known: bool,
    pub text_estimate_exceeds_capacity: Option<bool>,
}

/// Concrete coordinator, intentionally not a producer trait hierarchy. Contributions
/// remain ordered and are never deduplicated by prose or promoted to system roles.
#[derive(Debug, Clone, Default)]
pub struct ContextCoordinator {
    pub contributions: Vec<ContextContribution>,
    history: Option<ContextWindowPolicy>,
}

impl ContextCoordinator {
    pub fn with_legacy_history_limit(limit: usize) -> Self {
        Self {
            contributions: Vec::new(),
            history: Some(ContextWindowPolicy::legacy_history(limit)),
        }
    }

    pub fn history_allocation(&self, source_cap: usize) -> usize {
        self.history
            .as_ref()
            .expect("history coordinator configured")
            .allocation(source_cap)
    }

    /// Preserve legacy selection consumption separately from actual included
    /// cost (for example a role that consumes the old pool but is not emitted).
    pub fn account_history_selection(&mut self, chars: usize) {
        self.history
            .as_mut()
            .expect("history coordinator configured")
            .account(chars);
    }

    pub fn include(&mut self, contribution: ContextContribution) {
        self.contributions.push(contribution);
    }

    pub fn report(self, capacity: ContextCapacity) -> ContextDecisionReport {
        let cost = self
            .contributions
            .iter()
            .fold(ContextCost::default(), |sum, item| {
                sum.saturating_add(item.cost)
            });
        let text_tokens_estimate = cost.tokens_estimate();
        ContextDecisionReport {
            contributions: self.contributions,
            capacity,
            cost,
            text_tokens_estimate,
            complete_token_cost_known: false,
            text_estimate_exceeds_capacity: capacity
                .input_tokens
                .filter(|limit| *limit > 0)
                .map(|limit| text_tokens_estimate > limit),
        }
    }
}

/// One sequential history/auxiliary pool. Preserves the previous hot → cold →
/// slices → scripts → learnings → hints allocation, including domain sub-caps.
#[derive(Debug, Clone)]
struct ContextWindowPolicy {
    limit: usize,
    used: usize,
}

impl ContextWindowPolicy {
    pub fn legacy_history(limit: usize) -> Self {
        Self { limit, used: 0 }
    }

    pub fn allocation(&self, source_cap: usize) -> usize {
        source_cap.min(self.limit.saturating_sub(self.used))
    }

    pub fn account(&mut self, chars: usize) {
        self.used = self.used.saturating_add(chars);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_pool_is_ordered_saturating_and_deterministic() {
        for _ in 0..2 {
            let mut policy = ContextWindowPolicy::legacy_history(24);
            assert_eq!(policy.allocation(14), 14);
            policy.account(13);
            assert_eq!(policy.allocation(8), 8);
            policy.account(8);
            assert_eq!(policy.allocation(14), 3);
            policy.account(4);
            assert_eq!(policy.allocation(14), 0);
        }
    }

    #[test]
    fn coordinator_auxiliary_allocations_match_legacy_formulas() {
        for limit in [0, 1, 12, 13, 100, usize::MAX] {
            for source_cap in [0, 5, 24, usize::MAX] {
                for consume_divisor in [1, 2, 3] {
                    let mut coordinator = ContextCoordinator::with_legacy_history_limit(limit);
                    let mut used = 0usize;
                    // hot, cold, slices, scripts, learnings, hints; native caps
                    // remain owned by those producers, one pool governs all.
                    for _ in 0..6 {
                        let expected = source_cap.min(limit.saturating_sub(used));
                        assert_eq!(coordinator.history_allocation(source_cap), expected);
                        let included = expected / consume_divisor;
                        used = used.saturating_add(included);
                        coordinator.account_history_selection(included);
                    }
                }
            }
        }
    }

    #[test]
    fn unknown_capacity_and_media_are_not_claimed_as_measured_or_fitting() {
        let messages = vec![ChatMessage::user(genai::chat::MessageContent::from_parts(
            vec![
                ContentPart::Text("é🦀abc".into()),
                ContentPart::from_binary_base64("image/png", "AAAA", None),
            ],
        ))];
        let cost = ContextCost::messages(&messages);
        assert_eq!(cost.text_chars, 5);
        assert_eq!(cost.unknown_parts, 1);
        let mut coordinator = ContextCoordinator::default();
        coordinator.include(ContextContribution {
            id: "chat".into(),
            owner: ContextOwner::Chat,
            provenance: ContextProvenance {
                source: "session".into(),
                scope: Some("a".into()),
            },
            retention: Retention::Protected,
            decision: ContextDecision::Included,
            cost,
            budget_chars: None,
        });
        let unknown = coordinator.clone().report(ContextCapacity::default());
        assert_eq!(unknown.text_estimate_exceeds_capacity, None);
        assert!(!unknown.complete_token_cost_known);
        let small = coordinator.clone().report(ContextCapacity {
            input_tokens: Some(1),
        });
        assert_eq!(small.text_estimate_exceeds_capacity, Some(true));
        assert_eq!(
            coordinator.clone().report(ContextCapacity::default()),
            unknown
        );
        assert_eq!(
            unknown.contributions[0].provenance.scope.as_deref(),
            Some("a")
        );
    }
}
