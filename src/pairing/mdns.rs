use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result};
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use serde::{Deserialize, Serialize};

pub const MEDOUSA_SERVICE_TYPE: &str = "_medousa._tcp.local.";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredWorkshop {
    pub instance_name: String,
    pub host: String,
    pub port: u16,
    pub device_id: Option<String>,
    pub peer_name: Option<String>,
    pub protocol_version: Option<String>,
    pub capability_flags: Option<String>,
    pub auth_required: Option<bool>,
    pub model_descriptor: Option<String>,
    /// Canonical HTTP base, e.g. `http://192.168.1.12:7419`
    pub daemon_url: String,
}

pub struct MdnsAdvertiser {
    _daemon: ServiceDaemon,
    _service: ServiceInfo,
}

impl MdnsAdvertiser {
    pub fn register(
        instance_name: &str,
        host_name: &str,
        port: u16,
        properties: HashMap<String, String>,
    ) -> Result<Self> {
        let daemon = ServiceDaemon::new().context("create mDNS daemon")?;
        let service_info = ServiceInfo::new(
            MEDOUSA_SERVICE_TYPE,
            instance_name,
            host_name,
            "",
            port,
            properties,
        )
        .context("build mDNS service info")?;
        daemon
            .register(service_info.clone())
            .context("register mDNS service")?;
        Ok(Self {
            _daemon: daemon,
            _service: service_info,
        })
    }
}

impl Drop for MdnsAdvertiser {
    fn drop(&mut self) {
        let _ = self._daemon.unregister(self._service.get_fullname());
    }
}

/// Browse the LAN for Medousa workshops for up to `timeout`.
pub fn browse_workshops(timeout: Duration) -> Result<Vec<DiscoveredWorkshop>> {
    let daemon = ServiceDaemon::new().context("create mDNS browse daemon")?;
    let receiver = daemon
        .browse(MEDOUSA_SERVICE_TYPE)
        .context("start mDNS browse")?;

    let mut by_key: HashMap<String, DiscoveredWorkshop> = HashMap::new();
    let deadline = std::time::Instant::now() + timeout;

    while std::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        let wait = remaining.min(Duration::from_millis(250));
        match receiver.recv_timeout(wait) {
            Ok(event) => match event {
                ServiceEvent::ServiceResolved(info) => {
                    if let Some(workshop) = workshop_from_service_info(&info) {
                        by_key.insert(workshop.device_id.clone().unwrap_or_else(|| workshop.instance_name.clone()), workshop);
                    }
                }
                ServiceEvent::ServiceRemoved(name, _) => {
                    by_key.retain(|_, workshop| workshop.instance_name != name);
                }
                ServiceEvent::SearchStopped(_) => break,
                _ => {}
            },
            Err(_) => continue,
        }
    }

    let _ = daemon.stop_browse(MEDOUSA_SERVICE_TYPE);

    let mut workshops: Vec<DiscoveredWorkshop> = by_key.into_values().collect();
    workshops.sort_by(|left, right| {
        left.peer_name
            .as_deref()
            .unwrap_or(&left.instance_name)
            .cmp(right.peer_name.as_deref().unwrap_or(&right.instance_name))
    });
    Ok(workshops)
}

fn workshop_from_service_info(info: &ServiceInfo) -> Option<DiscoveredWorkshop> {
    let host = info
        .get_addresses()
        .iter()
        .find(|addr| addr.is_ipv4())
        .or_else(|| info.get_addresses().iter().next())
        .map(|addr| addr.to_string())?;
    let port = info.get_port();
    let device_id = info.get_property_val_str("dv").map(str::to_string);
    let peer_name = info.get_property_val_str("pn").map(str::to_string);
    let protocol_version = info.get_property_val_str("pv").map(str::to_string);
    let capability_flags = info.get_property_val_str("pf").map(str::to_string);
    let auth_required = info
        .get_property_val_str("ar")
        .map(|value| value == "1");
    let model_descriptor = info.get_property_val_str("md").map(str::to_string);
    let daemon_url = format!("http://{host}:{port}");

    Some(DiscoveredWorkshop {
        instance_name: info.get_fullname().to_string(),
        host,
        port,
        device_id,
        peer_name,
        protocol_version,
        capability_flags,
        auth_required,
        model_descriptor,
        daemon_url,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browse_workshops_times_out_cleanly() {
        let result = browse_workshops(Duration::from_millis(50));
        assert!(result.is_ok());
    }
}
