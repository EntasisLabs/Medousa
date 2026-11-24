//! Explicit connection plans for concurrent clients. Unlike `connection::discover`,
//! this catalog never consults or changes the legacy active-workshop selection.
//! Resolving a plan has no credential, network, registry-write or process effects.
use crate::workshop_registry::{WorkshopRegistry, WorkshopServer};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub enum Credentials {
    Local {
        data_dir: PathBuf,
    },
    Paired {
        workshop_id: String,
        device_id: String,
    },
}

#[derive(Debug, Clone)]
pub struct ConnectionPlan {
    pub workshop: WorkshopServer,
    pub origin: String,
    pub credentials: Credentials,
}

/// Resolve one exact saved identity. Never substitute Personal or another daemon
/// when a record is missing, offline, or has incomplete pairing metadata.
pub fn resolve(
    registry: &WorkshopRegistry,
    id: &str,
    root: &Path,
) -> Result<ConnectionPlan, String> {
    if registry.version != crate::workshop_registry::REGISTRY_VERSION {
        return Err("Unsupported workshop registry version".into());
    }
    let mut matches = registry.workshops.iter().filter(|w| w.id == id);
    let workshop = matches.next().ok_or("Workshop is no longer saved")?.clone();
    if matches.next().is_some() {
        return Err("Duplicate saved workshop identity".into());
    }
    let origin = crate::workshop_runtime::resolve_workshop_url(&workshop);
    let credentials = match workshop.kind.as_str() {
        "local" => {
            crate::connection::local_url(&origin)?;
            let data_dir = workshop
                .data_dir
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(PathBuf::from)
                .or_else(|| {
                    (id == crate::workshop_registry::PERSONAL_WORKSHOP_ID)
                        .then(|| root.to_path_buf())
                })
                .ok_or("Local workshop has no credential/data directory")?;
            if !data_dir.is_absolute() {
                return Err("Local workshop data directory must be absolute".into());
            }
            Credentials::Local { data_dir }
        }
        "portal" | "paired" => {
            let pairing = workshop
                .pairing
                .as_ref()
                .ok_or("Workshop needs trusted pairing")?;
            if pairing.workshop_device_id.trim().is_empty() || pairing.pairing_id.trim().is_empty()
            {
                return Err("Workshop pairing identity is incomplete".into());
            }
            let url = reqwest::Url::parse(&origin).map_err(|_| "Invalid workshop URL")?;
            if !matches!(url.scheme(), "http" | "https")
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.path() != "/"
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err("Workshop address must be an HTTP(S) origin".into());
            }
            Credentials::Paired {
                workshop_id: id.into(),
                device_id: pairing.workshop_device_id.clone(),
            }
        }
        "peer" => return Err("Peer connections are sharing-only, not client workspaces".into()),
        _ => return Err("Unsupported saved workshop kind".into()),
    };
    Ok(ConnectionPlan {
        workshop,
        origin: origin.trim_end_matches('/').into(),
        credentials,
    })
}

/// Keep each record's error local. One broken record must not hide healthy
/// connections, and the obsolete active id is deliberately irrelevant.
pub fn catalog(
    registry: &WorkshopRegistry,
    root: &Path,
) -> Vec<(WorkshopServer, Result<ConnectionPlan, String>)> {
    registry
        .workshops
        .iter()
        .filter(|w| w.kind != "peer")
        .map(|w| (w.clone(), resolve(registry, &w.id, root)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workshop_registry::*;

    #[test]
    fn concurrent_catalog_ignores_legacy_selection_and_isolates_errors() {
        let root = tempfile::tempdir().unwrap();
        let mut registry = default_registry();
        registry.active_workshop_id = "obsolete-and-missing".into();
        let mut other = default_personal_workshop();
        other.id = "second".into();
        other.url = "http://127.0.0.1:7420".into();
        other.bind = Some("127.0.0.1:7420".into());
        other.data_dir = Some(root.path().join("second").display().to_string());
        registry.workshops.push(other);
        let mut broken = default_personal_workshop();
        broken.id = "broken".into();
        broken.kind = "portal".into();
        registry.workshops.push(broken);
        let rows = catalog(&registry, root.path());
        assert_eq!(rows.len(), 3);
        assert!(rows[0].1.is_ok());
        assert!(rows[1].1.is_ok());
        assert!(rows[2].1.is_err());
        let Credentials::Local { data_dir } = &rows[1].1.as_ref().unwrap().credentials else {
            panic!()
        };
        assert_eq!(data_dir, &root.path().join("second"));
        assert_eq!(registry.active_workshop_id, "obsolete-and-missing");
    }

    #[test]
    fn missing_or_ambiguous_identity_never_falls_back() {
        let root = tempfile::tempdir().unwrap();
        let mut registry = default_registry();
        assert!(resolve(&registry, "missing", root.path()).is_err());
        registry.workshops.push(registry.workshops[0].clone());
        assert!(resolve(&registry, "personal", root.path()).is_err());
    }

    #[test]
    fn secondary_local_never_borrows_personal_credentials() {
        let root = tempfile::tempdir().unwrap();
        let mut registry = default_registry();
        let mut other = default_personal_workshop();
        other.id = "other".into();
        registry.workshops.push(other);
        assert!(resolve(&registry, "other", root.path()).is_err());
    }
}
