use super::hardware::{GpuBackend, HardwareProbe};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InferenceDevice {
    Cpu,
    Metal,
    Cuda,
}

impl InferenceDevice {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Metal => "metal",
            Self::Cuda => "cuda",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Cpu => "CPU",
            Self::Metal => "Apple Metal",
            Self::Cuda => "NVIDIA CUDA",
        }
    }
}

/// Feature flags compiled into this `medousa_daemon` binary.
pub fn compiled_backends() -> Vec<&'static str> {
    let backends = vec!["cpu"];
    #[cfg(feature = "embedded-inference-metal")]
    {
        backends.push("metal");
    }
    #[cfg(feature = "embedded-inference-cuda")]
    {
        backends.push("cuda");
    }
    backends
}

fn force_cpu_from_env() -> bool {
    std::env::var("MEDOUSA_LOCAL_ENGINE_CPU")
        .ok()
        .is_some_and(|value| matches!(value.trim(), "1" | "true" | "yes"))
}

#[cfg(feature = "embedded-inference-cuda")]
fn force_cuda_from_env() -> bool {
    std::env::var("MEDOUSA_LOCAL_ENGINE_CUDA")
        .ok()
        .is_some_and(|value| matches!(value.trim(), "1" | "true" | "yes"))
}

#[cfg(feature = "embedded-inference-cuda")]
pub fn cuda_device_present() -> bool {
    if std::env::var_os("CUDA_VISIBLE_DEVICES")
        .is_some_and(|value| value.is_empty() || value == "-1")
    {
        return false;
    }

    #[cfg(unix)]
    if std::path::Path::new("/dev/nvidia0").exists() {
        return true;
    }

    std::process::Command::new("nvidia-smi")
        .arg("-L")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(not(feature = "embedded-inference-cuda"))]
pub fn cuda_device_present() -> bool {
    false
}

pub fn detect_gpu_backend() -> GpuBackend {
    #[cfg(all(target_os = "macos", feature = "embedded-inference-metal"))]
    return GpuBackend::Metal;

    #[cfg(not(all(target_os = "macos", feature = "embedded-inference-metal")))]
    return detect_non_metal_gpu_backend();
}

#[cfg(not(all(target_os = "macos", feature = "embedded-inference-metal")))]
fn detect_non_metal_gpu_backend() -> GpuBackend {
    #[cfg(feature = "embedded-inference-cuda")]
    if cuda_device_present() || force_cuda_from_env() {
        return GpuBackend::Cuda;
    }

    GpuBackend::None
}

pub fn resolve_inference_device(_probe: &HardwareProbe) -> InferenceDevice {
    if force_cpu_from_env() {
        return InferenceDevice::Cpu;
    }

    #[cfg(feature = "embedded-inference-metal")]
    if matches!(probe.gpu_backend, GpuBackend::Metal) {
        return InferenceDevice::Metal;
    }

    #[cfg(feature = "embedded-inference-cuda")]
    if matches!(probe.gpu_backend, GpuBackend::Cuda) {
        return InferenceDevice::Cuda;
    }

    InferenceDevice::Cpu
}

pub fn resolve_cpu_only(probe: &HardwareProbe) -> bool {
    resolve_inference_device(probe) == InferenceDevice::Cpu
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::HardwareProbe;

    #[test]
    fn cpu_only_when_no_gpu_backend() {
        let probe = HardwareProbe {
            total_ram_mb: 16_384,
            available_ram_mb: 12_000,
            cpu_cores: 8,
            cpu_arch: "x86_64".to_string(),
            gpu_backend: GpuBackend::None,
            free_disk_gb: 100,
        };
        assert!(resolve_cpu_only(&probe));
        assert_eq!(resolve_inference_device(&probe), InferenceDevice::Cpu);
    }

    #[test]
    fn compiled_backends_always_include_cpu() {
        assert!(compiled_backends().contains(&"cpu"));
    }
}
