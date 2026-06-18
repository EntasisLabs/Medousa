use ed25519_dalek::SigningKey;
use iroh::SecretKey;
use sha2::{Digest, Sha256};

/// Domain-separated derivation — never reuse the raw Ed25519 pairing seed as an Iroh secret.
const IROH_WORKSHOP_DOMAIN: &[u8] = b"medousa-iroh-workshop-v1";

/// Derive a stable Iroh [`SecretKey`] from the long-term pairing identity seed.
pub fn secret_key_from_pairing_identity(signing_key: &SigningKey) -> SecretKey {
    let mut hasher = Sha256::new();
    hasher.update(IROH_WORKSHOP_DOMAIN);
    hasher.update(signing_key.to_bytes());
    let digest: [u8; 32] = hasher.finalize().into();
    SecretKey::from_bytes(&digest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;

    #[test]
    fn derived_iroh_key_is_stable() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let first = secret_key_from_pairing_identity(&signing_key);
        let second = secret_key_from_pairing_identity(&signing_key);
        assert_eq!(first.public().as_bytes(), second.public().as_bytes());
    }

    #[test]
    fn derived_iroh_key_differs_from_pairing_seed() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let derived = secret_key_from_pairing_identity(&signing_key);
        assert_ne!(derived.to_bytes(), signing_key.to_bytes());
    }
}
