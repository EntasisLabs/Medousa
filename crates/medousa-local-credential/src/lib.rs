//! Native local-client credentials shared by the daemon and first-party clients.
//!
//! The bearer secret is stored in the platform credential store when possible,
//! with an owner-only file fallback. The daemon retains only a SHA-256 verifier
//! and compares digests in constant time on the request path.

use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use arc_swap::ArcSwap;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use medousa_secrets::{SecretBackend, SecretStore as PlatformSecretStore};
use medousa_types::{DaemonSecretPath, LocalClientId, LocalClientKind};
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
const LEGACY_KEYRING_SERVICE: &str = "com.entasislabs.medousa.local-credentials";
const CREDENTIALS_DIR: &str = "credentials";

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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

/// Provision the stable Home credential if necessary and return its verifier.
///
/// This validates that the verifier and secret agree on every startup. Missing
/// or mismatched material fails closed instead of silently rotating authority.
pub fn provision_home_local(data_dir: &Path) -> Result<LocalCredentialVerifier> {
    provision_named(data_dir, HOME_LOCAL_NAME)
}

/// Provision each independently revocable first-party local client credential.
pub fn provision_first_party(data_dir: &Path) -> Result<LocalCredentialSet> {
    let mut verifiers = Vec::new();
    for name in FIRST_PARTY_LOCAL_NAMES {
        let record_path = data_dir.join(CREDENTIALS_DIR).join(record_file(name));
        if record_path.is_file() && read_record(&record_path)?.revoked {
            continue;
        }
        verifiers.push(provision_named(data_dir, name)?);
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
        if record.revoked {
            bail!("{name} local credential is revoked");
        }
        let secret = load_secret_for_record(data_dir, &record)?;
        return verifier_from_parts(&record, &secret, name);
    }

    if let Some(secret) = load_unrecorded_legacy_secret(data_dir, name)? {
        let secret_store = persist_secret(data_dir, name, &secret)?;
        let record = record_for_secret(name, secret.credential_id.clone(), &secret, secret_store);
        create_record(&record_path, &record)?;
        return load_verifier(data_dir, &record_path, name);
    }

    let secret = generate_secret();
    let secret_store = persist_secret(data_dir, name, &secret)?;
    let record = record_for_secret(name, secret.credential_id.clone(), &secret, secret_store);
    create_record(&record_path, &record)?;
    load_verifier(data_dir, &record_path, name)
}

/// Load the Home bearer for a native first-party transport.
pub fn load_home_local_secret(data_dir: &Path) -> Result<LocalCredentialSecret> {
    load_named_secret(data_dir, HOME_LOCAL_NAME)
}

pub fn load_named_secret(data_dir: &Path, name: &str) -> Result<LocalCredentialSecret> {
    validate_name(name)?;
    let record_path = data_dir.join(CREDENTIALS_DIR).join(record_file(name));
    let record = read_record(&record_path)?;
    let mut secret = load_secret_for_record(data_dir, &record)?;
    verifier_from_parts(&record, &secret, name)?;
    Ok(LocalCredentialSecret {
        credential_id: std::mem::take(&mut secret.credential_id),
        token: std::mem::take(&mut secret.token),
    })
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
    let secret_store = persist_secret(data_dir, name, &secret)?;
    let old_generation = old.generation;
    let old_was_active = !old.revoked;
    let next = CredentialRecord {
        version: RECORD_VERSION,
        name: name.to_string(),
        credential_id: old.credential_id.clone(),
        generation: next_generation,
        revoked: false,
        token_sha256: digest_hex(&Sha256::digest(secret.token.as_bytes())),
        secret_store,
    };
    if let Err(error) = replace_record(&record_path, &next) {
        let rollback = match previous_secret {
            Some(ref previous) => persist_secret(data_dir, name, previous).map(|_| ()),
            None => delete_secret(data_dir, name, &next.secret_store, &next.credential_id),
        };
        return match rollback {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(error.context(format!(
                "credential metadata replacement failed and secret rollback also failed: {rollback_error:#}"
            ))),
        };
    }
    if old.secret_store != next.secret_store {
        let _ = delete_secret(data_dir, name, &old.secret_store, &old.credential_id);
    }
    let verifier = verifier_from_parts(&next, &secret, name)?;
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
        delete_secret(data_dir, name, &record.secret_store, &record.credential_id)?;
        record.revoked = true;
        replace_record(&record_path, &record)?;
    }
    Ok(summary(&record))
}

fn load_verifier(
    data_dir: &Path,
    record_path: &Path,
    expected_name: &str,
) -> Result<LocalCredentialVerifier> {
    let record = read_record(record_path)?;
    let secret = load_secret_for_record(data_dir, &record)?;
    verifier_from_parts(&record, &secret, expected_name)
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
    validate_record(record, expected_name)?;
    if secret.credential_id != record.credential_id {
        bail!("local credential identifier does not match its verifier record");
    }
    let digest = decode_digest(&record.token_sha256)?;
    if !constant_time_eq(&digest, &Sha256::digest(secret.token.as_bytes())) {
        bail!("local credential secret does not match its verifier record");
    }
    Ok(LocalCredentialVerifier {
        name: Arc::from(record.name.as_str()),
        credential_id: Arc::from(record.credential_id.as_str()),
        digest,
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
            if let Some(secret) = load_token_for_account(data_dir, &record.name, &record.credential_id, account)?
            {
                return Ok(secret);
            }
            bail!(
                "{} local credential is missing from the platform credential store",
                record.name
            )
        }
        SecretStore::OwnerOnlyFile { relative_path } => {
            let mut secret = read_file_secret(&checked_secret_path(data_dir, relative_path)?)?;
            if secret.credential_id.is_empty() {
                secret.credential_id = record.credential_id.clone();
            }
            Ok(secret)
        }
    }
}

fn persist_secret(data_dir: &Path, name: &str, secret: &SecretPayload) -> Result<SecretStore> {
    let store = PlatformSecretStore::new(data_dir);
    let path = daemon_secret_path(data_dir, name, &secret.credential_id)?;
    let backend = store.set_daemon(&path, Some(&secret.token))?;
    Ok(match backend {
        SecretBackend::Keyring => SecretStore::Keyring {
            account: path.keyring_account(),
        },
        SecretBackend::OwnerOnlyFile => SecretStore::OwnerOnlyFile {
            relative_path: format!("secrets/{}", path.storage_key().as_str()),
        },
    })
}

fn daemon_secret_path(
    data_dir: &Path,
    name: &str,
    credential_id: &str,
) -> Result<DaemonSecretPath> {
    let installation = PlatformSecretStore::new(data_dir).ensure_installation_id()?;
    Ok(DaemonSecretPath::local_auth(
        installation,
        LocalClientKind::parse(name).map_err(|err| anyhow::anyhow!(err))?,
        LocalClientId::parse(credential_id).map_err(|err| anyhow::anyhow!(err))?,
    ))
}

fn load_token_for_account(
    data_dir: &Path,
    name: &str,
    credential_id: &str,
    account: &str,
) -> Result<Option<SecretPayload>> {
    if let Ok(path) = DaemonSecretPath::parse(account)
        && let Some(token) = PlatformSecretStore::new(data_dir).get_daemon(&path)
    {
        return Ok(Some(SecretPayload {
            credential_id: credential_id.to_string(),
            token,
        }));
    }
    if let Ok(path) = daemon_secret_path(data_dir, name, credential_id)
        && let Some(token) = PlatformSecretStore::new(data_dir).get_daemon(&path)
    {
        return Ok(Some(SecretPayload {
            credential_id: credential_id.to_string(),
            token,
        }));
    }
    if let Some(secret) = read_legacy_keyring_secret(account)? {
        return Ok(Some(secret));
    }
    Ok(None)
}

fn load_unrecorded_legacy_secret(data_dir: &Path, name: &str) -> Result<Option<SecretPayload>> {
    if let Ok(account) = legacy_keyring_account(data_dir, name)
        && let Some(secret) = read_legacy_keyring_secret(&account)?
    {
        return Ok(Some(secret));
    }
    let secret_path = data_dir.join(CREDENTIALS_DIR).join(secret_file(name));
    if secret_path.is_file() {
        return Ok(Some(read_file_secret(&secret_path)?));
    }
    Ok(None)
}

fn read_record(path: &Path) -> Result<CredentialRecord> {
    let raw = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_slice(&raw).with_context(|| format!("parse {}", path.display()))
}

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

#[allow(dead_code)]
fn replace_secret(
    data_dir: &Path,
    name: &str,
    store: &SecretStore,
    secret: &SecretPayload,
) -> Result<()> {
    match store {
        SecretStore::Keyring { account } => {
            if let Ok(path) = DaemonSecretPath::parse(account) {
                PlatformSecretStore::new(data_dir).set_daemon(&path, Some(&secret.token))?;
                return Ok(());
            }
            persist_secret(data_dir, name, secret).map(|_| ())
        }
        SecretStore::OwnerOnlyFile { relative_path } => {
            write_token_file(&checked_secret_path(data_dir, relative_path)?, secret)
        }
    }
}

fn delete_secret(
    data_dir: &Path,
    name: &str,
    store: &SecretStore,
    credential_id: &str,
) -> Result<()> {
    match store {
        SecretStore::Keyring { account } => {
            if let Ok(path) = DaemonSecretPath::parse(account) {
                PlatformSecretStore::new(data_dir).set_daemon(&path, None)?;
            } else if let Ok(path) = daemon_secret_path(data_dir, name, credential_id) {
                PlatformSecretStore::new(data_dir).set_daemon(&path, None)?;
            }
            let _ = PlatformSecretStore::new(data_dir)
                .delete_legacy_entry(LEGACY_KEYRING_SERVICE, account);
            Ok(())
        }
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
    let text = String::from_utf8_lossy(&raw).to_string();
    raw.zeroize();
    decode_secret_blob(&text, "")
}

fn write_token_file(path: &Path, secret: &SecretPayload) -> Result<()> {
    if path
        .extension()
        .is_some_and(|ext| ext == "secret")
        || path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().ends_with(".secret"))
    {
        let mut encoded = serde_json::to_vec(secret)?;
        let result = replace_private_file(path, &encoded);
        encoded.zeroize();
        return result;
    }
    replace_private_file(path, secret.token.as_bytes())
}

fn decode_secret_blob(raw: &str, credential_id: &str) -> Result<SecretPayload> {
    if let Ok(payload) = serde_json::from_str::<SecretPayload>(raw) {
        return Ok(payload);
    }
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("empty local credential secret");
    }
    Ok(SecretPayload {
        credential_id: credential_id.to_string(),
        token: trimmed.to_string(),
    })
}

fn read_legacy_keyring_secret(account: &str) -> Result<Option<SecretPayload>> {
    match medousa_secrets::read_legacy_keyring(LEGACY_KEYRING_SERVICE, account) {
        Some(raw) => Ok(Some(decode_secret_blob(&raw, "")?)),
        None => Ok(None),
    }
}

fn legacy_keyring_account(data_dir: &Path, name: &str) -> Result<String> {
    validate_name(name)?;
    let absolute = if data_dir.is_absolute() {
        data_dir.to_path_buf()
    } else {
        std::env::current_dir()?.join(data_dir)
    };
    let digest = Sha256::digest(absolute.to_string_lossy().as_bytes());
    Ok(format!("{name}-{}", &digest_hex(&digest)[..24]))
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
        use std::os::unix::fs::PermissionsExt;
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
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
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
