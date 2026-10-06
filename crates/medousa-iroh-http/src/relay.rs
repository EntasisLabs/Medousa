use anyhow::{Result, bail};
use iroh::RelayUrl;

/// Parse an explicit production relay set. Errors never echo possibly sensitive
/// configuration input. An absent setting is handled by the caller, not here.
pub fn parse_relay_urls(raw: &str) -> Result<Vec<RelayUrl>> {
    let mut relays = Vec::new();
    for entry in raw.split(',') {
        let url =
            url::Url::parse(entry.trim()).map_err(|_| anyhow::anyhow!("invalid Iroh relay URL"))?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/"
        {
            bail!("Iroh relays require HTTPS root URLs without credentials, queries, or fragments");
        }
        let relay = super::normalize_relay_host(&RelayUrl::from(url))?;
        if !relays.contains(&relay) {
            relays.push(relay);
        }
        if relays.len() > 8 {
            bail!("at most eight Iroh relays may be configured");
        }
    }
    Ok(relays)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configured_relays_are_normalized_and_deduplicated() {
        let relays = parse_relay_urls(
            " https://relay.example.com./,https://relay.example.com,https://backup.example.com/ ",
        )
        .unwrap();
        assert_eq!(relays.len(), 2);
        assert_eq!(relays[0].as_str(), "https://relay.example.com/");
    }
    #[test]
    fn rejects_invalid_or_sensitive_configuration_without_echoing_it() {
        for input in [
            "",
            "http://relay.example.com",
            "https://user:secret@relay.example.com",
            "https://relay.example.com/?key=secret",
            "https://relay.example.com/#secret",
            "https://relay.example.com/path",
            "https://relay.example.com,",
        ] {
            let error = parse_relay_urls(input).unwrap_err().to_string();
            assert!(!error.contains("secret"));
        }
        let too_many = (0..9)
            .map(|index| format!("https://relay{index}.example.com"))
            .collect::<Vec<_>>()
            .join(",");
        assert!(parse_relay_urls(&too_many).is_err());
    }
}
