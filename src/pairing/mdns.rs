use std::collections::HashMap;

use anyhow::{Context, Result};
use mdns_sd::{ServiceDaemon, ServiceInfo};

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
        let service_type = "_medousa._tcp.local.";
        let service_info = ServiceInfo::new(
            service_type,
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
        let _ = self._daemon.unregister(&self._service.get_fullname());
    }
}
