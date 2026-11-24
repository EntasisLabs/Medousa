//! Host-only composite identities. A local item id is never a routing key.
//! These handles stay in retained UI state/caches; only `wire` crosses the API.
use serde::{Deserialize, Serialize};

const PREFIX: &str = "ses_native_";
fn identities() -> &'static std::sync::RwLock<std::collections::HashMap<String, SessionKey>> {
    static KEYS: std::sync::OnceLock<
        std::sync::RwLock<std::collections::HashMap<String, SessionKey>>,
    > = std::sync::OnceLock::new();
    KEYS.get_or_init(Default::default)
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionKey {
    pub authority: String,
    pub session: String,
}
impl SessionKey {
    pub fn new(authority: &str, session: &str) -> Self {
        Self {
            authority: authority.into(),
            session: session.into(),
        }
    }
    pub fn handle(&self) -> String {
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(serde_json::to_vec(self).expect("session key serializes"));
        let handle = format!("{PREFIX}{hash:x}");
        let mut keys = identities().write().unwrap();
        if let Some(existing) = keys.get(&handle) {
            assert_eq!(existing, self, "composite identity digest collision");
        } else {
            keys.insert(handle.clone(), self.clone());
        }
        handle
    }
    pub fn parse(handle: &str) -> Option<Self> {
        identities().read().unwrap().get(handle).cloned()
    }
}
pub fn qualify(authority: &str, raw: &str) -> String {
    if authority.is_empty() {
        raw.into()
    } else {
        SessionKey::new(authority, raw).handle()
    }
}
pub fn wire_owned(handle: &str) -> String {
    SessionKey::parse(handle)
        .map(|k| k.session)
        .unwrap_or_else(|| handle.into())
}
pub fn checked_wire(authority: &str, handle: &str) -> Result<String, String> {
    match SessionKey::parse(handle) {
        Some(key) if key.authority == authority => Ok(key.session),
        Some(_) => Err("Conversation belongs to a different workshop".into()),
        None if authority.is_empty() => Ok(handle.into()),
        None => Err("Conversation has no owning workshop".into()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub authority: String,
    pub epoch: u64,
}
impl Binding {
    pub fn new(authority: String) -> Self {
        Self {
            authority,
            epoch: 0,
        }
    }
    pub fn select(&mut self, authority: String) {
        if self.authority != authority {
            self.authority = authority;
            self.epoch = self.epoch.checked_add(1).expect("view epoch overflow");
        }
    }
    pub fn accepts(&self, captured: &Self) -> bool {
        self == captured
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn colliding_sessions_and_delimiters_cannot_alias() {
        let a = qualify("auth-a", "same");
        let b = qualify("auth-b", "same");
        assert_ne!(a, b);
        assert!(medousa_types::SessionId::parse(&a).is_ok());
        assert!(medousa_types::SessionId::parse(qualify("auth-a", &"s".repeat(128))).is_ok());
        assert_eq!(checked_wire("auth-a", &a).unwrap(), "same");
        assert!(checked_wire("auth-b", &a).is_err());
        let odd = SessionKey::new("a:\"/", "b:\"/native-session:{}");
        assert_eq!(SessionKey::parse(&odd.handle()), Some(odd));
    }
    #[test]
    fn view_round_trip_rejects_old_response() {
        let mut notes = Binding::new("a".into());
        let canvas = Binding::new("b".into());
        let pending = notes.clone();
        notes.select("b".into());
        notes.select("a".into());
        assert!(!notes.accepts(&pending));
        assert_eq!(canvas.authority, "b");
        assert_eq!(canvas.epoch, 0);
    }
}
