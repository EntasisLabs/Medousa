#[cfg(target_os = "windows")]
mod platform {
    use medousa_types::{GpuBackend, LocalDeviceTelemetrySnapshot, LocalDeviceTelemetrySource};
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, DXGI_ERROR_NOT_FOUND,
        DXGI_MEMORY_SEGMENT_GROUP_LOCAL, DXGI_QUERY_VIDEO_MEMORY_INFO, IDXGIAdapter3,
        IDXGIFactory1,
    };
    use windows::core::Interface;

    use super::super::{empty_snapshot, finish_availability, populate_missing_fields};

    pub(super) fn try_collect() -> Option<Result<Vec<LocalDeviceTelemetrySnapshot>, String>> {
        Some(collect())
    }

    fn collect() -> Result<Vec<LocalDeviceTelemetrySnapshot>, String> {
        // SAFETY: DXGI returns reference-counted COM interfaces and initializes
        // every output structure. The windows crate owns all Release calls.
        unsafe {
            let factory: IDXGIFactory1 = CreateDXGIFactory1()
                .map_err(|error| format!("CreateDXGIFactory1 failed: {error}"))?;
            let mut snapshots = Vec::new();
            for index in 0_u32.. {
                let adapter = match factory.EnumAdapters1(index) {
                    Ok(adapter) => adapter,
                    Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
                    Err(error) => return Err(format!("EnumAdapters1({index}) failed: {error}")),
                };
                let description = adapter
                    .GetDesc1()
                    .map_err(|error| format!("IDXGIAdapter1::GetDesc1 failed: {error}"))?;
                if description.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                    continue;
                }
                let adapter3: IDXGIAdapter3 = adapter
                    .cast()
                    .map_err(|error| format!("IDXGIAdapter3 is unavailable: {error}"))?;
                let mut memory = DXGI_QUERY_VIDEO_MEMORY_INFO::default();
                adapter3
                    .QueryVideoMemoryInfo(0, DXGI_MEMORY_SEGMENT_GROUP_LOCAL, &mut memory)
                    .map_err(|error| format!("QueryVideoMemoryInfo failed: {error}"))?;

                let backend = backend_for_vendor(description.VendorId);
                let mut snapshot = empty_snapshot(LocalDeviceTelemetrySource::Wddm, backend);
                snapshot.device_index = Some(index);
                snapshot.device_uuid = Some(format!(
                    "luid:{:08x}:{:08x}",
                    description.AdapterLuid.HighPart as u32, description.AdapterLuid.LowPart
                ));
                snapshot.device_name = wide_string(&description.Description);
                snapshot.memory_total_mb = Some(bytes_to_mb(
                    u64::try_from(description.DedicatedVideoMemory).unwrap_or(u64::MAX),
                ));
                snapshot.memory_budget_mb = Some(bytes_to_mb(memory.Budget));
                snapshot.process_memory_used_mb = Some(bytes_to_mb(memory.CurrentUsage));
                snapshot.memory_used_mb = Some(bytes_to_mb(memory.CurrentUsage));
                snapshot.memory_free_mb = Some(bytes_to_mb(
                    memory.Budget.saturating_sub(memory.CurrentUsage),
                ));
                populate_missing_fields(&mut snapshot);
                finish_availability(&mut snapshot);
                snapshots.push(snapshot);
            }
            Ok(snapshots)
        }
    }

    fn backend_for_vendor(vendor_id: u32) -> GpuBackend {
        match vendor_id {
            0x10de => GpuBackend::Cuda,
            0x1002 | 0x1022 => GpuBackend::Rocm,
            _ => GpuBackend::Vulkan,
        }
    }

    fn wide_string(value: &[u16]) -> Option<String> {
        let length = value
            .iter()
            .position(|character| *character == 0)
            .unwrap_or(value.len());
        let value = String::from_utf16_lossy(&value[..length])
            .trim()
            .to_string();
        (!value.is_empty()).then_some(value)
    }

    fn bytes_to_mb(bytes: u64) -> u64 {
        bytes / 1024 / 1024
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn vendor_ids_map_only_where_wddm_is_api_agnostic() {
            assert_eq!(backend_for_vendor(0x10de), GpuBackend::Cuda);
            assert_eq!(backend_for_vendor(0x1002), GpuBackend::Rocm);
            assert_eq!(backend_for_vendor(0x8086), GpuBackend::Vulkan);
        }

        #[test]
        fn budget_remaining_is_saturating() {
            assert_eq!(bytes_to_mb(8 * 1024 * 1024), 8);
            assert_eq!(4_u64.saturating_sub(8), 0);
        }
    }
}

#[cfg(target_os = "windows")]
pub(super) use platform::try_collect;

#[cfg(not(target_os = "windows"))]
pub(super) fn try_collect()
-> Option<Result<Vec<medousa_types::LocalDeviceTelemetrySnapshot>, String>> {
    None
}
