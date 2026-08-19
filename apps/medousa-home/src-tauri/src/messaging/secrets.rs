//! Legacy Home keyring I/O retired. Daemon-owned secrets move through generated
//! `/v1/integrations*` ops; pairing uses `medousa-secrets` client service.

#[cfg(test)]
mod tests {
    use medousa_types::authority_id::ProviderId;

    #[test]
    fn provider_ids_stay_opaque_for_file_fallbacks() {
        let provider = ProviderId::parse("openai.compatible").unwrap();
        assert!(provider.storage_key().as_str().starts_with("pv1-"));
        assert!(!provider.storage_key().as_str().contains("openai"));
        assert!(ProviderId::parse("../../outside").is_err());
    }
}
