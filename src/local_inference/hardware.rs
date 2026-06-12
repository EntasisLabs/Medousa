use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HardwareTier {
    A,
    B,
    C,
    D,
    E,
}

impl HardwareTier {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::B => "B",
            Self::C => "C",
            Self::D => "D",
            Self::E => "E",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::A => "Minimal",
            Self::B => "Everyday",
            Self::C => "Comfortable",
            Self::D => "Enthusiast",
            Self::E => "Workstation",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GpuBackend {
    None,
    Metal,
    Cuda,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareProbe {
    pub total_ram_mb: u64,
    pub available_ram_mb: u64,
    pub cpu_cores: usize,
    pub cpu_arch: String,
    pub gpu_backend: GpuBackend,
    pub free_disk_gb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareProfile {
    pub probed_at: DateTime<Utc>,
    pub tier: HardwareTier,
    pub tier_label: String,
    pub probe: HardwareProbe,
    pub recommended_model_id: String,
    pub recommended_display_name: String,
}

pub fn medousa_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa")
}

pub fn hardware_profile_path() -> PathBuf {
    medousa_data_dir().join("hardware-profile.json")
}

pub fn probe_hardware() -> HardwareProbe {
    let mut system = sysinfo::System::new_all();
    system.refresh_memory();
    system.refresh_cpu_all();

    let total_ram_mb = system.total_memory() / 1024 / 1024;
    let available_ram_mb = system.available_memory() / 1024 / 1024;
    let cpu_cores = system.cpus().len().max(1);
    let cpu_arch = std::env::consts::ARCH.to_string();
    let gpu_backend = detect_gpu_backend();
    let free_disk_gb = free_disk_gb_on_data_volume();

    HardwareProbe {
        total_ram_mb,
        available_ram_mb,
        cpu_cores,
        cpu_arch,
        gpu_backend,
        free_disk_gb,
    }
}

fn detect_gpu_backend() -> GpuBackend {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        GpuBackend::Metal
    }
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    {
        if std::env::var_os("CUDA_VISIBLE_DEVICES")
            .map(|value| !value.is_empty() && value != "-1")
            .unwrap_or(false)
        {
            GpuBackend::Cuda
        } else {
            GpuBackend::None
        }
    }
}

fn free_disk_gb_on_data_volume() -> u64 {
    let path = medousa_data_dir();
    if let Some(parent) = path.parent() {
        if let Ok(space) = fs2::available_space(parent) {
            return space / 1024 / 1024 / 1024;
        }
    }
    if let Ok(space) = fs2::available_space(&path) {
        return space / 1024 / 1024 / 1024;
    }
    0
}

pub fn score_tier(probe: &HardwareProbe) -> HardwareTier {
    let ram_gb = probe.total_ram_mb / 1024;
    let available_gb = probe.available_ram_mb / 1024;
    let has_accel = matches!(probe.gpu_backend, GpuBackend::Metal | GpuBackend::Cuda);

    let mut tier = HardwareTier::A;
    if ram_gb >= 8 && probe.free_disk_gb >= 4 {
        tier = HardwareTier::A;
    }
    if ram_gb >= 12 {
        tier = HardwareTier::B;
    }
    if ram_gb >= 16 && available_gb >= 8 {
        tier = HardwareTier::C;
    }
    if ram_gb >= 24 && has_accel {
        tier = HardwareTier::D;
    }
    if ram_gb >= 48 {
        tier = HardwareTier::E;
    }

    if tier >= HardwareTier::C && probe.available_ram_mb < 12_000 {
        tier = HardwareTier::B;
    }
    if tier >= HardwareTier::D && probe.available_ram_mb < 20_000 {
        tier = HardwareTier::C;
    }

    tier
}

impl PartialOrd for HardwareTier {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HardwareTier {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (*self as u8).cmp(&(*other as u8))
    }
}

pub fn build_hardware_profile(probe: HardwareProbe) -> HardwareProfile {
    let tier = score_tier(&probe);
    let (recommended_model_id, recommended_display_name) =
        super::catalog::recommended_model_for_tier(tier)
            .map(|entry| (entry.id.clone(), entry.display_name.clone()))
            .unwrap_or_else(|| ("gemma-4-e4b-it".to_string(), "Gemma 4 E4B — balanced".to_string()));

    HardwareProfile {
        probed_at: Utc::now(),
        tier,
        tier_label: tier.label().to_string(),
        probe,
        recommended_model_id,
        recommended_display_name,
    }
}

pub fn write_hardware_profile(profile: &HardwareProfile) -> Result<(), String> {
    let path = hardware_profile_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let json = serde_json::to_string_pretty(profile).map_err(|err| err.to_string())?;
    fs::write(path, json).map_err(|err| err.to_string())
}

pub fn read_hardware_profile() -> Option<HardwareProfile> {
    let path = hardware_profile_path();
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe_with_ram(total_gb: u64, available_gb: u64, disk_gb: u64) -> HardwareProbe {
        HardwareProbe {
            total_ram_mb: total_gb * 1024,
            available_ram_mb: available_gb * 1024,
            cpu_cores: 8,
            cpu_arch: "aarch64".to_string(),
            gpu_backend: GpuBackend::Metal,
            free_disk_gb: disk_gb,
        }
    }

    #[test]
    fn tier_a_on_8gb_mac() {
        let tier = score_tier(&probe_with_ram(8, 4, 20));
        assert_eq!(tier, HardwareTier::A);
    }

    #[test]
    fn tier_c_on_16gb_with_headroom() {
        let tier = score_tier(&probe_with_ram(16, 13, 40));
        assert_eq!(tier, HardwareTier::C);
    }

    #[test]
    fn tier_b_when_16gb_but_low_available() {
        let tier = score_tier(&probe_with_ram(16, 6, 40));
        assert_eq!(tier, HardwareTier::B);
    }

    #[test]
    fn tier_e_on_workstation() {
        let tier = score_tier(&probe_with_ram(64, 48, 200));
        assert_eq!(tier, HardwareTier::E);
    }
}
