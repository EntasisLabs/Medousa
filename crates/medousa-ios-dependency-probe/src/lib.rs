//! Non-published compile/link probe for the native iOS daemon dependency graph.
//!
//! Build each feature independently. In particular, `grapheme-portable` must not
//! be combined with `stasis-native`, because current Stasis releases enable the
//! Grapheme host surface and Cargo would unify those features.

/// Exercise native Stasis, Locus, Surreal, and Medousa engine symbols so an iOS
/// `staticlib` build validates more than manifest resolution.
#[cfg(feature = "stasis-native")]
pub fn native_runtime_fingerprint() -> String {
    use locus_core_rs::NodeQuery;
    use locus_sdk::domain::ai::AiCapability;
    use stasis::prelude::RuntimeBackend;

    let backend = RuntimeBackend::InMemory;
    let query = NodeQuery::default();
    let capability = AiCapability::SemanticEmbedding;
    let embedded = locus_surreal_adapter::is_embedded_endpoint("surrealkv://medousa-ios-probe");
    let envelope =
        medousa_engine::TurnEnvelope::new("ios-probe-turn", medousa_engine::Principal::operator());

    format!(
        "{backend:?}:{}:{capability:?}:{embedded}:{}",
        query.limit, envelope.turn_id
    )
}

/// Construct the lean SDK engine without Grapheme's `host` or `full` features.
#[cfg(feature = "grapheme-portable")]
pub fn portable_grapheme_engine() -> grapheme_sdk::GraphemeEngine {
    grapheme_sdk::GraphemeEngine::builder().build()
}

/// Link the strict OS-keyring diagnostic used by the simulator/device smoke.
#[cfg(feature = "keychain")]
pub fn verify_keychain_roundtrip() -> Result<(), String> {
    medousa_secrets::probe_client_keyring_roundtrip().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "stasis-native")]
    #[test]
    fn native_runtime_symbols_are_reachable() {
        let fingerprint = super::native_runtime_fingerprint();
        assert!(fingerprint.contains("ios-probe-turn"));
        assert!(fingerprint.contains("true"));
    }

    #[cfg(feature = "grapheme-portable")]
    #[test]
    fn portable_grapheme_constructs() {
        let _engine = super::portable_grapheme_engine();
    }
}
