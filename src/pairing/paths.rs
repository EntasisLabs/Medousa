use std::path::PathBuf;

pub fn medousa_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa")
}

pub fn identity_dir() -> PathBuf {
    medousa_data_dir().join("identity")
}

pub fn identity_secret_path() -> PathBuf {
    identity_dir().join("ed25519_sk")
}

pub fn pairings_dir() -> PathBuf {
    medousa_data_dir().join("pairings")
}

pub fn revoked_pairings_path() -> PathBuf {
    pairings_dir().join("revoked.json")
}
