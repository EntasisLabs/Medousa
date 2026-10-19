//! Native local-client credentials shared by the daemon and first-party clients.
//!
//! The bearer secret is stored in an owner-only file. Existing Keychain records
//! with a private mirror are migrated without rotating the token. Keychain-only
//! local tokens are reissued at daemon startup without unlocking Keychain.
//! The daemon retains a SHA-256 verifier
//! and compares digests in constant time on the request path.

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Context, Result, bail};
use arc_swap::ArcSwap;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

pub const HOME_LOCAL_NAME: &str = "home-local";
pub const CLI_LOCAL_NAME: &str = "medousa-cli";
pub const TUI_LOCAL_NAME: &str = "medousa-tui";
pub const FIRST_PARTY_LOCAL_NAMES: [&str; 3] = [HOME_LOCAL_NAME, CLI_LOCAL_NAME, TUI_LOCAL_NAME];
const RECORD_VERSION: u8 = 1;
const CREDENTIALS_DIR: &str = "credentials";

static LOADED_SECRET_CACHE: OnceLock<Mutex<HashMap<(PathBuf, String), CachedLocalCredential>>> =
    OnceLock::new();

use medousa_types::secrets::DaemonSecretPath;

#[derive(Clone)]
pub struct LocalCredentialVerifier {
    name: Arc<str>,
    credential_id: Arc<str>,
    digest: [u8; 32],
    generation: u64,
}

impl LocalCredentialVerifier {
    pub fn from_token(credential_id: impl Into<Arc<str>>, token: &str) -> Self {
        Self::from_token_with_generation(credential_id, token, 1)
    }

    pub fn from_token_with_generation(
        credential_id: impl Into<Arc<str>>,
        token: &str,
        generation: u64,
    ) -> Self {
        Self {
            name: Arc::from("test-local"),
            credential_id: credential_id.into(),
            digest: Sha256::digest(token.as_bytes()).into(),
            generation: generation.max(1),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn credential_id(&self) -> &str {
        &self.credential_id
    }

    pub fn credential_id_arc(&self) -> Arc<str> {
        self.credential_id.clone()
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn verify(&self, token: &str) -> bool {
        constant_time_eq(&self.digest, &Sha256::digest(token.as_bytes()))
    }
}

#[derive(Clone)]
pub struct LocalCredentialSet {
    verifiers: Arc<ArcSwap<Vec<LocalCredentialVerifier>>>,
}

impl LocalCredentialSet {
    pub fn new(verifiers: impl IntoIterator<Item = LocalCredentialVerifier>) -> Self {
        Self {
            verifiers: Arc::new(ArcSwap::from_pointee(verifiers.into_iter().collect())),
        }
    }

    /// Resolve a bearer to its stable credential id with one hash operation.
    pub fn resolve(&self, token: &str) -> Option<(Arc<str>, u64)> {
        let digest = Sha256::digest(token.as_bytes());
        let mut resolved = None;
        for verifier in self.verifiers.load().iter() {
            if constant_time_eq(&verifier.digest, &digest) {
                resolved = Some((verifier.credential_id_arc(), verifier.generation()));
            }
        }
        resolved
    }

    pub fn replace(&self, verifier: LocalCredentialVerifier) {
        let mut next = self.verifiers.load().as_ref().clone();
        next.retain(|current| current.name() != verifier.name());
        next.push(verifier);
        self.verifiers.store(Arc::new(next));
    }

    pub fn revoke(&self, name: &str) {
        let mut next = self.verifiers.load().as_ref().clone();
        next.retain(|current| current.name() != name);
        self.verifiers.store(Arc::new(next));
    }

    /// Register a process-owned credential without reading or writing persistent secrets.
    pub fn provision_process_credential(&self) -> LocalCredentialSecret {
        let mut secret = generate_secret();
        let mut verifier =
            LocalCredentialVerifier::from_token(secret.credential_id.clone(), &secret.token);
        verifier.name = Arc::from("daemon-self");
        self.replace(verifier);
        LocalCredentialSecret {
            credential_id: std::mem::take(&mut secret.credential_id),
            token: std::mem::take(&mut secret.token),
        }
    }
}

pub struct LocalCredentialSecret {
    credential_id: String,
    token: String,
}

impl LocalCredentialSecret {
    pub fn credential_id(&self) -> &str {
        &self.credential_id
    }

    pub fn token(&self) -> &str {
        &self.token
    }
}

impl Drop for LocalCredentialSecret {
    fn drop(&mut self) {
        self.token.zeroize();
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CredentialRecord {
    version: u8,
    name: String,
    credential_id: String,
    #[serde(default = "initial_generation")]
    generation: u64,
    #[serde(default)]
    revoked: bool,
    token_sha256: String,
    secret_store: SecretStore,
}

const fn initial_generation() -> u64 {
    1
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalCredentialSummary {
    pub name: String,
    pub credential_id: String,
    pub generation: u64,
    pub revoked: bool,
    pub secret_store: &'static str,
}

pub struct LocalCredentialRotation {
    pub verifier: LocalCredentialVerifier,
    pub revoked_generation: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum SecretStore {
    Keyring { account: String },
    OwnerOnlyFile { relative_path: String },
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SecretPayload {
    credential_id: String,
    token: String,
}

impl Drop for SecretPayload {
    fn drop(&mut self) {
        self.token.zeroize();
    }
}

struct CachedLocalCredential {
    credential_id: String,
    generation: u64,
    token_sha256: String,
    token: String,
}

impl Drop for CachedLocalCredential {
    fn drop(&mut self) {
        self.token.zeroize();
    }
}

/// Provision the stable Home credential if necessary and return its verifier.
///
/// Private mirrors retain their token. Keychain-only local capabilities are
/// reissued with their stable ID and a newer generation, without a Keychain read.
/// Malformed or mismatched material is rejected.
pub fn provision_home_local(data_dir: &Path) -> Result<LocalCredentialVerifier> {
    provision_named(data_dir, HOME_LOCAL_NAME)
}

/// Provision each independently revocable first-party local client credential.
pub fn provision_first_party(data_dir: &Path) -> Result<LocalCredentialSet> {
    let mut verifiers = Vec::new();
    for name in FIRST_PARTY_LOCAL_NAMES {
        let record_path = data_dir.join(CREDENTIALS_DIR).join(record_file(name));
        if record_path.is_file() {
            let record = read_record(&record_path)?;
            if record.revoked {
                continue;
            }
            // The record already contains the verifier digest. Daemon startup
            // must not unlock three client secrets merely to reconstruct the
            // hashes it uses on the request path.
            verifiers.push(
                if matches!(record.secret_store, SecretStore::Keyring { .. }) {
                    provision_named(data_dir, name)?
                } else {
                    verifier_from_record(&record, name)?
                },
            );
        } else {
            verifiers.push(provision_named(data_dir, name)?);
        }
    }
    Ok(LocalCredentialSet::new(verifiers))
}

pub fn provision_named(data_dir: &Path, name: &str) -> Result<LocalCredentialVerifier> {
    validate_name(name)?;
    let credentials_dir = data_dir.join(CREDENTIALS_DIR);
    create_private_dir(&credentials_dir)?;
    let record_path = credentials_dir.join(record_file(name));

    if record_path.is_file() {
        let record = read_record(&record_path)?;
        validate_record_shape(&record, name)?;
        if record.revoked {
            bail!("{name} local credential is revoked");
        }
        match load_secret_for_record(data_dir, &record) {
            Ok(secret) => return verifier_from_parts(&record, &secret, name),
            Err(error)
                if matches!(record.secret_store, SecretStore::Keyring { .. })
                    && secret_material_missing(&error) =>
            {
                // These are generated local capabilities, not external account
                // credentials. Reissue at the authority's startup instead of
                // blocking unattended boot on a Keychain import dialog.
                let mut secret = generate_secret();
                secret.credential_id = record.credential_id.clone();
                let next_generation = record
                    .generation
                    .checked_add(1)
                    .context("local credential generation exhausted")?;
                replace_secret(data_dir, name, &private_store(name), &secret)?;
                let mut next = record;
                next.generation = next_generation;
                next.token_sha256 = digest_hex(&Sha256::digest(secret.token.as_bytes()));
                next.secret_store = private_store(name);
                replace_record(&record_path, &next)?;
                clear_cached_secret(data_dir, name);
                return verifier_from_parts(&next, &secret, name);
            }
            Err(error) if secret_material_missing(&error) => {
                // Stale metadata after a failed keyring migration / denied
                // Keychain write — drop the record and re-issue rather than
                // blocking daemon boot.
                eprintln!(
                    "medousa-local-credential: re-provisioning {name} after missing secret material ({error:#})"
                );
                let _ = delete_secret(data_dir, name, &record.secret_store);
                let _ = fs::remove_file(&record_path);
            }
            Err(error) => return Err(error),
        }
    }

    let secret_path = credentials_dir.join(secret_file(name));
    if secret_path.is_file() {
        let secret = read_file_secret(&secret_path)?;
        let record = record_for_secret(
            name,
            secret.credential_id.clone(),
            &secret,
            SecretStore::OwnerOnlyFile {
                relative_path: format!("{CREDENTIALS_DIR}/{}", secret_file(name)),
            },
        );
        replace_record(&record_path, &record)?;
        return verifier_from_parts(&record, &secret, name);
    }

    let secret = generate_secret();
    replace_secret(data_dir, name, &private_store(name), &secret)?;
    let record = record_for_secret(
        name,
        secret.credential_id.clone(),
        &secret,
        private_store(name),
    );
    replace_record(&record_path, &record)?;
    verifier_from_parts(&record, &secret, name)
}

/// Load the Home bearer for a native first-party transport.
pub fn load_home_local_secret(data_dir: &Path) -> Result<LocalCredentialSecret> {
    load_named_secret(data_dir, HOME_LOCAL_NAME)
}

pub fn load_named_secret(data_dir: &Path, name: &str) -> Result<LocalCredentialSecret> {
    validate_name(name)?;
    let record_path = data_dir.join(CREDENTIALS_DIR).join(record_file(name));
    let record = read_record(&record_path)?;
    validate_record(&record, name)?;
    let cache_key = (data_dir.to_path_buf(), name.to_string());
    let mut cache = LOADED_SECRET_CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(cached) = cache.get(&cache_key)
        && cached.credential_id == record.credential_id
        && cached.generation == record.generation
        && cached.token_sha256 == record.token_sha256
    {
        return Ok(LocalCredentialSecret {
            credential_id: cached.credential_id.clone(),
            token: cached.token.clone(),
        });
    }
    cache.remove(&cache_key);
    let mut secret = load_secret_for_record(data_dir, &record)?;
    verifier_from_parts(&record, &secret, name)?;
    let credential_id = std::mem::take(&mut secret.credential_id);
    let token = std::mem::take(&mut secret.token);
    cache.insert(
        cache_key,
        CachedLocalCredential {
            credential_id: credential_id.clone(),
            generation: record.generation,
            token_sha256: record.token_sha256,
            token: token.clone(),
        },
    );
    Ok(LocalCredentialSecret {
        credential_id,
        token,
    })
}

fn clear_cached_secret(data_dir: &Path, name: &str) {
    let Some(cache) = LOADED_SECRET_CACHE.get() else {
        return;
    };
    cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(&(data_dir.to_path_buf(), name.to_string()));
}

/// Load a named secret when it has been provisioned by a compatible daemon.
/// A present but invalid record remains a hard error.
pub fn try_load_named_secret(data_dir: &Path, name: &str) -> Result<Option<LocalCredentialSecret>> {
    validate_name(name)?;
    let record_path = data_dir.join(CREDENTIALS_DIR).join(record_file(name));
    if !record_path.exists() {
        return Ok(None);
    }
    if read_record(&record_path)?.revoked {
        return Ok(None);
    }
    load_named_secret(data_dir, name).map(Some)
}

pub fn list_local_credentials(data_dir: &Path) -> Result<Vec<LocalCredentialSummary>> {
    let mut summaries = Vec::new();
    for name in FIRST_PARTY_LOCAL_NAMES {
        let path = data_dir.join(CREDENTIALS_DIR).join(record_file(name));
        if path.is_file() {
            summaries.push(summary(&read_record(&path)?));
        }
    }
    Ok(summaries)
}

pub fn rotate_named(data_dir: &Path, name: &str) -> Result<LocalCredentialRotation> {
    validate_name(name)?;
    let credentials_dir = data_dir.join(CREDENTIALS_DIR);
    create_private_dir(&credentials_dir)?;
    let record_path = credentials_dir.join(record_file(name));
    if !record_path.is_file() {
        let verifier = provision_named(data_dir, name)?;
        return Ok(LocalCredentialRotation {
            verifier,
            revoked_generation: None,
        });
    }

    let old = read_record(&record_path)?;
    validate_record_shape(&old, name)?;
    let next_generation = old
        .generation
        .checked_add(1)
        .context("local credential generation exhausted")?;
    let previous_secret = if old.revoked {
        None
    } else {
        Some(load_secret_for_record(data_dir, &old)?)
    };
    let mut secret = generate_secret();
    secret.credential_id = old.credential_id.clone();
    replace_secret(data_dir, name, &old.secret_store, &secret)?;
    let old_generation = old.generation;
    let old_was_active = !old.revoked;
    let next_store = private_store(name);
    let next = CredentialRecord {
        version: RECORD_VERSION,
        name: name.to_string(),
        credential_id: old.credential_id.clone(),
        generation: next_generation,
        revoked: false,
        token_sha256: digest_hex(&Sha256::digest(secret.token.as_bytes())),
        secret_store: next_store,
    };
    if let Err(error) = replace_record(&record_path, &next) {
        let rollback = match previous_secret {
            Some(ref previous) => replace_secret(data_dir, name, &next.secret_store, previous),
            None => delete_secret(data_dir, name, &next.secret_store),
        };
        return match rollback {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(error.context(format!(
                "credential metadata replacement failed and secret rollback also failed: {rollback_error:#}"
            ))),
        };
    }
    let verifier = verifier_from_parts(&next, &secret, name)?;
    clear_cached_secret(data_dir, name);
    Ok(LocalCredentialRotation {
        verifier,
        revoked_generation: old_was_active.then_some(old_generation),
    })
}

pub fn revoke_named(data_dir: &Path, name: &str) -> Result<LocalCredentialSummary> {
    validate_name(name)?;
    let record_path = data_dir.join(CREDENTIALS_DIR).join(record_file(name));
    let mut record = read_record(&record_path)?;
    validate_record_shape(&record, name)?;
    if !record.revoked {
        delete_secret(data_dir, name, &record.secret_store)?;
        record.revoked = true;
        replace_record(&record_path, &record)?;
    }
    clear_cached_secret(data_dir, name);
    Ok(summary(&record))
}

fn generate_secret() -> SecretPayload {
    let mut token = [0u8; 32];
    OsRng.fill_bytes(&mut token);
    SecretPayload {
        credential_id: uuid::Uuid::new_v4().to_string(),
        token: URL_SAFE_NO_PAD.encode(token),
    }
}

fn record_for_secret(
    name: &str,
    credential_id: String,
    secret: &SecretPayload,
    secret_store: SecretStore,
) -> CredentialRecord {
    CredentialRecord {
        version: RECORD_VERSION,
        name: name.to_string(),
        credential_id,
        generation: 1,
        revoked: false,
        token_sha256: digest_hex(&Sha256::digest(secret.token.as_bytes())),
        secret_store,
    }
}

fn verifier_from_parts(
    record: &CredentialRecord,
    secret: &SecretPayload,
    expected_name: &str,
) -> Result<LocalCredentialVerifier> {
    let verifier = verifier_from_record(record, expected_name)?;
    if secret.credential_id != record.credential_id {
        bail!("local credential identifier does not match its verifier record");
    }
    if !verifier.verify(&secret.token) {
        bail!("local credential secret does not match its verifier record");
    }
    Ok(verifier)
}

fn verifier_from_record(
    record: &CredentialRecord,
    expected_name: &str,
) -> Result<LocalCredentialVerifier> {
    validate_record(record, expected_name)?;
    Ok(LocalCredentialVerifier {
        name: Arc::from(record.name.as_str()),
        credential_id: Arc::from(record.credential_id.as_str()),
        digest: decode_digest(&record.token_sha256)?,
        generation: record.generation,
    })
}

fn validate_record(record: &CredentialRecord, expected_name: &str) -> Result<()> {
    validate_record_shape(record, expected_name)?;
    if record.revoked {
        bail!("{expected_name} local credential is revoked");
    }
    Ok(())
}

fn validate_record_shape(record: &CredentialRecord, expected_name: &str) -> Result<()> {
    if record.version != RECORD_VERSION {
        bail!(
            "unsupported local credential record version {}",
            record.version
        );
    }
    validate_name(expected_name)?;
    if record.name != expected_name || record.credential_id.trim().is_empty() {
        bail!("invalid {expected_name} local credential record");
    }
    if record.generation == 0 {
        bail!("invalid {expected_name} local credential generation");
    }
    Ok(())
}

fn summary(record: &CredentialRecord) -> LocalCredentialSummary {
    LocalCredentialSummary {
        name: record.name.clone(),
        credential_id: record.credential_id.clone(),
        generation: record.generation,
        revoked: record.revoked,
        secret_store: match &record.secret_store {
            SecretStore::Keyring { .. } => "platform_keyring",
            SecretStore::OwnerOnlyFile { .. } => "owner_only_file",
        },
    }
}

fn load_secret_for_record(data_dir: &Path, record: &CredentialRecord) -> Result<SecretPayload> {
    match &record.secret_store {
        SecretStore::Keyring { account } => {
            let mirror = data_dir
                .join(CREDENTIALS_DIR)
                .join(secret_file(&record.name));
            if mirror.is_file() {
                let secret = read_file_secret(&mirror)?;
                return migrate_local_secret(data_dir, record, secret);
            }
            if let Ok(path @ DaemonSecretPath::LocalAuth { .. }) = DaemonSecretPath::parse(account)
            {
                let private = data_dir.join("secrets").join(path.storage_key().as_str());
                if private.is_file() {
                    return migrate_local_secret(data_dir, record, read_file_secret(&private)?);
                }
            }
            bail!(
                "{} local credential secret material is missing; start the daemon to provision private storage",
                record.name
            )
        }
        SecretStore::OwnerOnlyFile { relative_path } => {
            read_file_secret(&checked_secret_path(data_dir, relative_path)?)
        }
    }
}

fn private_store(name: &str) -> SecretStore {
    SecretStore::OwnerOnlyFile {
        relative_path: format!("{CREDENTIALS_DIR}/{}", secret_file(name)),
    }
}

fn migrate_local_secret(
    data_dir: &Path,
    record: &CredentialRecord,
    secret: SecretPayload,
) -> Result<SecretPayload> {
    verifier_from_parts(record, &secret, &record.name)?;
    create_private_dir(&data_dir.join(CREDENTIALS_DIR))?;
    let mirror = data_dir
        .join(CREDENTIALS_DIR)
        .join(secret_file(&record.name));
    let mut encoded = serde_json::to_vec(&secret)?;
    let created = create_private_file(&mirror, &encoded);
    encoded.zeroize();
    match created {
        Ok(()) => {}
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|io| io.kind() == ErrorKind::AlreadyExists) =>
        {
            let existing = read_file_secret(&mirror)?;
            verifier_from_parts(record, &existing, &record.name)?;
        }
        Err(error) => return Err(error),
    }
    let record_path = data_dir
        .join(CREDENTIALS_DIR)
        .join(record_file(&record.name));
    let mut current = read_record(&record_path)?;
    if current.credential_id != record.credential_id
        || current.generation != record.generation
        || current.token_sha256 != record.token_sha256
        || current.revoked
    {
        bail!("local credential changed during storage migration");
    }
    current.secret_store = private_store(&record.name);
    replace_record(&record_path, &current)?;
    Ok(secret)
}

fn secret_material_missing(error: &anyhow::Error) -> bool {
    let text = error.to_string();
    text.contains("local credential secret material is missing")
        || text.contains("No such file")
        || text.contains("os error 2")
}

fn read_record(path: &Path) -> Result<CredentialRecord> {
    let raw = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_slice(&raw).with_context(|| format!("parse {}", path.display()))
}

#[allow(dead_code)] // test fixtures (`provision_file_fixture`)
fn create_record(path: &Path, record: &CredentialRecord) -> Result<()> {
    let encoded = serde_json::to_vec_pretty(record).context("serialize local credential record")?;
    match create_private_file(path, &encoded) {
        Ok(()) => Ok(()),
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|io| io.kind() == ErrorKind::AlreadyExists) =>
        {
            Ok(())
        }
        Err(error) => Err(error),
    }
}

fn replace_record(path: &Path, record: &CredentialRecord) -> Result<()> {
    let encoded = serde_json::to_vec_pretty(record).context("serialize local credential record")?;
    replace_private_file(path, &encoded)
}

fn replace_secret(
    data_dir: &Path,
    name: &str,
    store: &SecretStore,
    secret: &SecretPayload,
) -> Result<()> {
    match store {
        SecretStore::Keyring { .. } => replace_secret(data_dir, name, &private_store(name), secret),
        SecretStore::OwnerOnlyFile { relative_path } => {
            let path = checked_secret_path(data_dir, relative_path)?;
            let mut encoded = serde_json::to_vec(secret)?;
            let result = replace_private_file(&path, &encoded);
            encoded.zeroize();
            result
        }
    }
}

fn delete_secret(data_dir: &Path, name: &str, store: &SecretStore) -> Result<()> {
    match store {
        SecretStore::Keyring { .. } => delete_secret(data_dir, name, &private_store(name)),
        SecretStore::OwnerOnlyFile { relative_path } => {
            let path = checked_secret_path(data_dir, relative_path)?;
            match fs::remove_file(&path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error).with_context(|| format!("delete {}", path.display())),
            }
        }
    }
}

fn checked_secret_path(data_dir: &Path, relative_path: &str) -> Result<std::path::PathBuf> {
    let relative = Path::new(relative_path);
    if relative.is_absolute()
        || relative
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        bail!("invalid local credential secret path");
    }
    Ok(data_dir.join(relative))
}

fn read_file_secret(path: &Path) -> Result<SecretPayload> {
    let mut raw = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let result = serde_json::from_slice(&raw).with_context(|| format!("parse {}", path.display()));
    raw.zeroize();
    result
}

fn validate_name(name: &str) -> Result<()> {
    if FIRST_PARTY_LOCAL_NAMES.contains(&name) {
        Ok(())
    } else {
        bail!("unsupported local credential name")
    }
}

fn record_file(name: &str) -> String {
    format!("{name}.json")
}

fn secret_file(name: &str) -> String {
    format!("{name}.secret")
}

fn create_private_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).with_context(|| format!("create {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        // A daemon launched as root still shares these credentials with the
        // owner of its data directory (Home/CLI run as that user).
        if let Some(parent) = path.parent() {
            let owner = fs::metadata(parent)?.uid();
            if fs::metadata(path)?.uid() != owner {
                std::os::unix::fs::chown(path, Some(owner), None)?;
            }
        }
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .with_context(|| format!("restrict {}", path.display()))?;
    }
    Ok(())
}

fn create_private_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .with_context(|| format!("create {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let Some(parent) = path.parent() {
            let owner = fs::metadata(parent)?.uid();
            if file.metadata()?.uid() != owner {
                std::os::unix::fs::chown(path, Some(owner), None)?;
            }
        }
    }
    file.write_all(bytes)
        .with_context(|| format!("write {}", path.display()))?;
    file.sync_all()
        .with_context(|| format!("sync {}", path.display()))?;
    Ok(())
}

fn replace_private_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("local credential path has no file name")?;
    let temporary = path.with_file_name(format!(".{file_name}.{}.tmp", uuid::Uuid::new_v4()));
    create_private_file(&temporary, bytes)?;
    replace_from_temporary(path, &temporary)
}

#[cfg(not(windows))]
fn replace_from_temporary(path: &Path, temporary: &Path) -> Result<()> {
    if let Err(error) = fs::rename(temporary, path) {
        let _ = fs::remove_file(temporary);
        return Err(error).with_context(|| format!("replace {}", path.display()));
    }
    Ok(())
}

#[cfg(windows)]
fn replace_from_temporary(path: &Path, temporary: &Path) -> Result<()> {
    if !path.exists() {
        return fs::rename(temporary, path).with_context(|| format!("replace {}", path.display()));
    }
    let backup = path.with_extension(format!("backup-{}", uuid::Uuid::new_v4()));
    fs::rename(path, &backup).with_context(|| format!("stage replacement {}", path.display()))?;
    if let Err(error) = fs::rename(temporary, path) {
        let _ = fs::rename(&backup, path);
        let _ = fs::remove_file(temporary);
        return Err(error).with_context(|| format!("replace {}", path.display()));
    }
    let _ = fs::remove_file(backup);
    Ok(())
}

fn decode_digest(value: &str) -> Result<[u8; 32]> {
    if value.len() != 64 {
        bail!("invalid local credential verifier length");
    }
    let mut digest = [0u8; 32];
    for (index, pair) in value.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        let pair = std::str::from_utf8(pair)?;
        digest[index] =
            u8::from_str_radix(pair, 16).context("invalid local credential verifier")?;
    }
    Ok(digest)
}

fn digest_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

fn constant_time_eq(left: &[u8; 32], right: &[u8]) -> bool {
    if right.len() != left.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0u8, |difference, (left, right)| difference | (left ^ right))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_types::secrets::LocalClientKind;

    #[test]
    fn fresh_credentials_use_private_files_across_cold_starts() -> Result<()> {
        let data_dir = test_dir("private-default");
        let set = provision_first_party(&data_dir)?;
        for name in FIRST_PARTY_LOCAL_NAMES {
            let secret = load_named_secret(&data_dir, name)?;
            assert!(set.resolve(secret.token()).is_some());
            assert_eq!(
                summary(&read_record(
                    &data_dir.join(CREDENTIALS_DIR).join(record_file(name))
                )?)
                .secret_store,
                "owner_only_file"
            );
            clear_cached_secret(&data_dir, name);
            assert_eq!(load_named_secret(&data_dir, name)?.token(), secret.token());
        }
        fs::remove_dir_all(data_dir)?;
        Ok(())
    }

    #[test]
    fn bootstrap_reissues_keychain_only_tokens_without_unlocking_keychain() -> Result<()> {
        let data_dir = test_dir("keychain-only-bootstrap");
        let old = provision_file_fixture(&data_dir, TUI_LOCAL_NAME)?;
        let path = data_dir
            .join(CREDENTIALS_DIR)
            .join(record_file(TUI_LOCAL_NAME));
        let mut record = read_record(&path)?;
        record.secret_store = SecretStore::Keyring {
            account: "never-query-the-host-keychain".into(),
        };
        record.generation = 4;
        replace_record(&path, &record)?;
        fs::remove_file(
            data_dir
                .join(CREDENTIALS_DIR)
                .join(secret_file(TUI_LOCAL_NAME)),
        )?;
        clear_cached_secret(&data_dir, TUI_LOCAL_NAME);
        let set = provision_first_party(&data_dir)?;
        let current = load_named_secret(&data_dir, TUI_LOCAL_NAME)?;
        assert_eq!(current.credential_id(), old.credential_id());
        assert_ne!(current.token(), old.token());
        assert!(set.resolve(old.token()).is_none());
        assert_eq!(set.resolve(current.token()).unwrap().1, 5);
        assert_eq!(
            summary(&read_record(&path)?).secret_store,
            "owner_only_file"
        );
        assert_eq!(
            provision_first_party(&data_dir)?
                .resolve(current.token())
                .unwrap()
                .1,
            5
        );
        fs::remove_dir_all(data_dir)?;
        Ok(())
    }

    #[test]
    fn legacy_record_migrates_without_changing_credential_identity_or_token() -> Result<()> {
        let data_dir = test_dir("legacy-to-private");
        let secret = provision_file_fixture(&data_dir, CLI_LOCAL_NAME)?;
        let record_path = data_dir
            .join(CREDENTIALS_DIR)
            .join(record_file(CLI_LOCAL_NAME));
        let mut record = read_record(&record_path)?;
        record.generation = 7;
        record.secret_store = SecretStore::Keyring {
            account: "legacy-account".into(),
        };
        replace_record(&record_path, &record)?;
        clear_cached_secret(&data_dir, CLI_LOCAL_NAME);
        let migrated = load_named_secret(&data_dir, CLI_LOCAL_NAME)?;
        assert_eq!(migrated.token(), secret.token());
        assert_eq!(migrated.credential_id(), secret.credential_id());
        let next = read_record(&record_path)?;
        assert_eq!(next.generation, 7);
        assert_eq!(summary(&next).secret_store, "owner_only_file");
        let rotation = rotate_named(&data_dir, CLI_LOCAL_NAME)?;
        assert_eq!(rotation.verifier.generation(), 8);
        assert!(!rotation.verifier.verify(secret.token()));
        revoke_named(&data_dir, CLI_LOCAL_NAME)?;
        assert!(try_load_named_secret(&data_dir, CLI_LOCAL_NAME)?.is_none());
        fs::remove_dir_all(data_dir)?;
        Ok(())
    }
    use std::path::PathBuf;

    fn test_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "medousa-local-credential-{name}-{}",
            uuid::Uuid::new_v4()
        ))
    }

    fn provision_file_fixture(data_dir: &Path, name: &str) -> Result<LocalCredentialSecret> {
        let credentials = data_dir.join(CREDENTIALS_DIR);
        create_private_dir(&credentials)?;
        let secret = generate_secret();
        let mut encoded = serde_json::to_vec(&secret)?;
        create_private_file(&credentials.join(secret_file(name)), &encoded)?;
        encoded.zeroize();
        let record = record_for_secret(
            name,
            secret.credential_id.clone(),
            &secret,
            SecretStore::OwnerOnlyFile {
                relative_path: format!("{CREDENTIALS_DIR}/{}", secret_file(name)),
            },
        );
        create_record(&credentials.join(record_file(name)), &record)?;
        load_named_secret(data_dir, name)
    }

    #[test]
    fn file_secret_is_stable_and_verifies_without_string_allocation() -> Result<()> {
        let data_dir = test_dir("stable");
        let secret = provision_file_fixture(&data_dir, HOME_LOCAL_NAME)?;
        let first = provision_home_local(&data_dir)?;
        let second = provision_home_local(&data_dir)?;
        assert_eq!(first.credential_id(), second.credential_id());
        assert_eq!(first.credential_id(), secret.credential_id());
        assert!(first.verify(secret.token()));
        assert!(!first.verify("wrong-token"));
        fs::remove_dir_all(data_dir)?;
        Ok(())
    }

    #[test]
    fn stale_keyring_record_without_secret_is_reprovisioned() -> Result<()> {
        unsafe {
            std::env::set_var("MEDOUSA_TEST_HERMETIC", "1");
        }
        let data_dir = test_dir("stale-keyring");
        create_private_dir(&data_dir.join(CREDENTIALS_DIR))?;
        let installation = medousa_secrets::ensure_installation_id(&data_dir)?;
        let credential_id = uuid::Uuid::new_v4().to_string();
        let path = DaemonSecretPath::LocalAuth {
            installation_id: installation,
            client_kind: LocalClientKind::HomeLocal,
            client_id: credential_id.clone(),
        };
        let dangling = CredentialRecord {
            version: RECORD_VERSION,
            name: HOME_LOCAL_NAME.to_string(),
            credential_id,
            generation: 1,
            revoked: false,
            token_sha256: digest_hex(&[0u8; 32]),
            secret_store: SecretStore::Keyring {
                account: path.account(),
            },
        };
        replace_record(
            &data_dir
                .join(CREDENTIALS_DIR)
                .join(record_file(HOME_LOCAL_NAME)),
            &dangling,
        )?;

        let verifier = provision_home_local(&data_dir)?;
        let secret = load_home_local_secret(&data_dir)?;
        assert_eq!(verifier.credential_id(), secret.credential_id());
        assert!(verifier.verify(secret.token()));
        fs::remove_dir_all(data_dir)?;
        unsafe {
            std::env::remove_var("MEDOUSA_TEST_HERMETIC");
        }
        Ok(())
    }

    #[test]
    fn generated_secret_has_at_least_256_bits() {
        let secret = generate_secret();
        assert_eq!(URL_SAFE_NO_PAD.decode(&secret.token).unwrap().len(), 32);
    }

    #[test]
    fn mismatched_secret_fails_closed() -> Result<()> {
        let data_dir = test_dir("mismatch");
        let secret = provision_file_fixture(&data_dir, HOME_LOCAL_NAME)?;
        let path = data_dir
            .join(CREDENTIALS_DIR)
            .join(secret_file(HOME_LOCAL_NAME));
        let replacement = SecretPayload {
            credential_id: secret.credential_id().to_string(),
            token: "replacement".to_string(),
        };
        fs::write(&path, serde_json::to_vec(&replacement)?)?;
        assert!(provision_home_local(&data_dir).is_err());
        fs::remove_dir_all(data_dir)?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn file_fallback_is_owner_only() -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        let data_dir = test_dir("permissions");
        let _ = provision_file_fixture(&data_dir, HOME_LOCAL_NAME)?;
        let credentials = data_dir.join(CREDENTIALS_DIR);
        assert_eq!(
            fs::metadata(&credentials)?.permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(credentials.join(secret_file(HOME_LOCAL_NAME)))?
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(credentials.join(record_file(HOME_LOCAL_NAME)))?
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        fs::remove_dir_all(data_dir)?;
        Ok(())
    }

    #[test]
    fn named_credentials_are_distinct_and_resolve_to_their_own_ids() -> Result<()> {
        let data_dir = test_dir("named");
        let cli = provision_file_fixture(&data_dir, CLI_LOCAL_NAME)?;
        let tui = provision_file_fixture(&data_dir, TUI_LOCAL_NAME)?;
        let set = LocalCredentialSet::new([
            provision_named(&data_dir, CLI_LOCAL_NAME)?,
            provision_named(&data_dir, TUI_LOCAL_NAME)?,
        ]);

        assert_ne!(cli.credential_id(), tui.credential_id());
        assert_ne!(cli.token(), tui.token());
        assert_eq!(
            set.resolve(cli.token()).as_ref().map(|(id, _)| id.as_ref()),
            Some(cli.credential_id())
        );
        assert_eq!(
            set.resolve(tui.token()).as_ref().map(|(id, _)| id.as_ref()),
            Some(tui.credential_id())
        );
        assert!(set.resolve("wrong-token").is_none());
        fs::remove_dir_all(data_dir)?;
        Ok(())
    }

    #[test]
    fn first_party_bootstrap_uses_persisted_verifiers_without_secret_reads() -> Result<()> {
        let data_dir = test_dir("verifier-only-bootstrap");
        let home = provision_file_fixture(&data_dir, HOME_LOCAL_NAME)?;
        let cli = provision_file_fixture(&data_dir, CLI_LOCAL_NAME)?;
        let tui = provision_file_fixture(&data_dir, TUI_LOCAL_NAME)?;
        for name in FIRST_PARTY_LOCAL_NAMES {
            fs::remove_file(data_dir.join(CREDENTIALS_DIR).join(secret_file(name)))?;
        }

        let set = provision_first_party(&data_dir)?;

        assert!(set.resolve(home.token()).is_some());
        assert!(set.resolve(cli.token()).is_some());
        assert!(set.resolve(tui.token()).is_some());
        fs::remove_dir_all(data_dir)?;
        Ok(())
    }

    #[test]
    fn client_secret_cache_is_invalidated_by_record_generation() -> Result<()> {
        let data_dir = test_dir("client-cache-generation");
        let first = provision_file_fixture(&data_dir, HOME_LOCAL_NAME)?;
        fs::remove_file(
            data_dir
                .join(CREDENTIALS_DIR)
                .join(secret_file(HOME_LOCAL_NAME)),
        )?;

        let cached = load_home_local_secret(&data_dir)?;
        assert_eq!(cached.token(), first.token());

        let record_path = data_dir
            .join(CREDENTIALS_DIR)
            .join(record_file(HOME_LOCAL_NAME));
        let mut record = read_record(&record_path)?;
        record.generation += 1;
        replace_record(&record_path, &record)?;
        assert!(load_home_local_secret(&data_dir).is_err());

        fs::remove_dir_all(data_dir)?;
        Ok(())
    }

    #[test]
    fn named_record_cannot_be_substituted_for_another_client() -> Result<()> {
        let data_dir = test_dir("substitution");
        let _ = provision_file_fixture(&data_dir, CLI_LOCAL_NAME)?;
        fs::copy(
            data_dir
                .join(CREDENTIALS_DIR)
                .join(record_file(CLI_LOCAL_NAME)),
            data_dir
                .join(CREDENTIALS_DIR)
                .join(record_file(TUI_LOCAL_NAME)),
        )?;
        assert!(load_named_secret(&data_dir, TUI_LOCAL_NAME).is_err());
        fs::remove_dir_all(data_dir)?;
        Ok(())
    }

    #[test]
    fn arbitrary_credential_names_are_rejected() {
        let data_dir = test_dir("invalid-name");
        assert!(provision_named(&data_dir, "../escape").is_err());
        assert!(load_named_secret(&data_dir, "unknown").is_err());
    }

    #[test]
    fn rotation_invalidates_old_secret_and_advances_generation() -> Result<()> {
        let data_dir = test_dir("rotation");
        let old_secret = provision_file_fixture(&data_dir, CLI_LOCAL_NAME)?;
        let set = LocalCredentialSet::new([provision_named(&data_dir, CLI_LOCAL_NAME)?]);
        let rotation = rotate_named(&data_dir, CLI_LOCAL_NAME)?;
        assert_eq!(rotation.revoked_generation, Some(1));
        assert_eq!(rotation.verifier.generation(), 2);
        set.replace(rotation.verifier);
        assert!(set.resolve(old_secret.token()).is_none());
        let new_secret = load_named_secret(&data_dir, CLI_LOCAL_NAME)?;
        assert_eq!(set.resolve(new_secret.token()).unwrap().1, 2);
        fs::remove_dir_all(data_dir)?;
        Ok(())
    }

    #[test]
    fn exhausted_generation_fails_before_replacing_secret() -> Result<()> {
        let data_dir = test_dir("generation-exhausted");
        let old_secret = provision_file_fixture(&data_dir, CLI_LOCAL_NAME)?;
        let record_path = data_dir
            .join(CREDENTIALS_DIR)
            .join(record_file(CLI_LOCAL_NAME));
        let mut record = read_record(&record_path)?;
        record.generation = u64::MAX;
        replace_record(&record_path, &record)?;

        assert!(rotate_named(&data_dir, CLI_LOCAL_NAME).is_err());
        let unchanged = load_named_secret(&data_dir, CLI_LOCAL_NAME)?;
        assert_eq!(unchanged.token(), old_secret.token());
        fs::remove_dir_all(data_dir)?;
        Ok(())
    }

    #[test]
    fn revoked_credential_stays_revoked_across_provisioning() -> Result<()> {
        let data_dir = test_dir("revoke");
        let _ = provision_file_fixture(&data_dir, HOME_LOCAL_NAME)?;
        let _ = provision_file_fixture(&data_dir, CLI_LOCAL_NAME)?;
        let _ = provision_file_fixture(&data_dir, TUI_LOCAL_NAME)?;
        let summary = revoke_named(&data_dir, TUI_LOCAL_NAME)?;
        assert!(summary.revoked);
        assert!(try_load_named_secret(&data_dir, TUI_LOCAL_NAME)?.is_none());
        let set = provision_first_party(&data_dir)?;
        assert!(set.resolve("anything").is_none());
        assert!(
            list_local_credentials(&data_dir)?
                .iter()
                .any(|entry| entry.name == TUI_LOCAL_NAME && entry.revoked)
        );
        fs::remove_dir_all(data_dir)?;
        Ok(())
    }
}
