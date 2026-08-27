//! Stable entry points for compiled production system policy.

use super::prompt_policy::{
    SttpPolicyActor, SttpPolicyMode, SttpPolicySelection, compile_sttp_policy,
};

/// General host policy for lightweight channels, scheduled jobs, and CLI turns.
pub fn lightweight_channel_system_prompt() -> String {
    compile_sttp_policy(SttpPolicySelection::new(
        SttpPolicyMode::General,
        SttpPolicyActor::Host,
    ))
    .expect("built-in STTP general host policy must compile")
    .rendered
}
