//! Read-only selection of the configured local desktop connection.
//! No directory scanning, credential reads, registry writes, or process effects.
use crate::{workshop_registry::*, workshop_runtime::*};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn local_url(value: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(value).map_err(|_| "Invalid daemon URL")?;
    let local = url.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    });
    if !local
        || !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "This slice requires a loopback HTTP(S) daemon origin, without userinfo/path/query"
                .into(),
        );
    }
    Ok(url)
}

#[derive(Default)]
pub struct Overrides {
    pub data_dir: Option<PathBuf>,
    pub daemon: Option<String>,
}

pub struct LocalConnection {
    pub origin: String,
    pub data_dir: PathBuf,
    pub workshop: WorkshopServer,
    /// Explicit origins are attach-only: never spawn/restart a process at an arbitrary override.
    pub allow_bootstrap: bool,
}

/// `root` is the canonical environment/redirect/platform root, NOT an arbitrary search path.
/// An explicit data directory selects a separate local engine; it does not import a registry.
pub fn resolve_local(root: &Path, overrides: Overrides) -> Result<LocalConnection, String> {
    let mut workshop = if overrides.data_dir.is_some() {
        default_personal_workshop()
    } else {
        let path = root.join("workshops.json");
        let mut registry = match fs::read(&path) {
            Ok(raw) => serde_json::from_slice::<WorkshopRegistry>(&raw).map_err(|_| {
                "Invalid workshop registry; repair Connection settings in Medousa".to_string()
            })?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => default_registry(),
            Err(_) => {
                return Err(
                    "Cannot read workshop registry; check local configuration permissions".into(),
                );
            }
        };
        normalize_desktop_registry(&mut registry)?;
        registry
            .workshops
            .into_iter()
            .find(|w| w.id == registry.active_workshop_id)
            .ok_or("Active workshop not found")?
    };
    if workshop.kind != "local" {
        return Err("The active workshop is remote; RNDR supports local workshops only. Select a local workshop in Medousa or supply --data-dir and --daemon.".into());
    }
    // Personal always uses the canonical root; local secondary workshops own their own data.
    let data_dir = overrides.data_dir.unwrap_or_else(|| {
        workshop
            .data_dir
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| root.to_path_buf())
    });
    let allow_bootstrap = overrides.daemon.is_none();
    let origin = overrides
        .daemon
        .unwrap_or_else(|| resolve_workshop_url(&workshop));
    let origin = local_url(&origin)?.to_string();
    // Bind the credential root and bootstrap root to the same selected local engine.
    workshop.data_dir = Some(data_dir.to_string_lossy().into_owned());
    Ok(LocalConnection {
        origin,
        data_dir,
        workshop,
        allow_bootstrap,
    })
}

pub fn discover(overrides: Overrides) -> Result<LocalConnection, String> {
    resolve_local(&crate::paths::medousa_data_dir(), overrides)
}

impl LocalConnection {
    /// Called only by connected startup, before any authenticated SDK clients exist.
    pub async fn ensure_ready(&self) -> Result<(), String> {
        // Revalidate before any network/process effect, even for manually constructed plans.
        local_url(&self.origin)?;
        if self.allow_bootstrap {
            let expected = local_url(&resolve_workshop_url(&self.workshop))?;
            if expected.as_str() != self.origin
                || resolve_workshop_data_dir(&self.workshop) != self.data_dir
            {
                return Err("Local connection/bootstrap binding mismatch".into());
            }
            let result = ensure_local_engine(&self.workshop, false).await?;
            if !result.ok {
                return Err(
                    "Local daemon did not become ready; inspect the selected workshop engine log"
                        .into(),
                );
            }
        } else if !wait_engine_healthy(&self.origin, 45, 500).await?.0 {
            return Err("Explicit local daemon is unavailable; check --daemon and --data-dir (no process started)".into());
        }
        Ok(())
    }
}
