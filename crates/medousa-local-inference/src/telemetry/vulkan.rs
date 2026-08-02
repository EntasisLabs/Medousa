use std::ffi::CStr;

use ash::{Entry, vk};
use medousa_types::{GpuBackend, LocalDeviceTelemetrySnapshot, LocalDeviceTelemetrySource};

use super::{empty_snapshot, finish_availability, populate_missing_fields};

const MEMORY_BUDGET_EXTENSION: &CStr = c"VK_EXT_memory_budget";

pub(super) fn try_collect() -> Option<Result<Vec<LocalDeviceTelemetrySnapshot>, String>> {
    // SAFETY: Entry::load only resolves Vulkan loader symbols. Failure means
    // Vulkan is not applicable on this system, not a collector failure.
    let entry = unsafe { Entry::load().ok()? };
    Some(collect(&entry))
}

fn collect(entry: &Entry) -> Result<Vec<LocalDeviceTelemetrySnapshot>, String> {
    let loader_version = unsafe { entry.try_enumerate_instance_version() }
        .map_err(|error| format!("Vulkan loader version query failed: {error}"))?
        .unwrap_or(vk::API_VERSION_1_0);
    if loader_version < vk::API_VERSION_1_1 {
        return Err("Vulkan 1.1 is required for memory-budget collection".to_string());
    }

    let application = vk::ApplicationInfo::default()
        .application_name(c"Medousa telemetry")
        .application_version(1)
        .engine_name(c"Medousa")
        .engine_version(1)
        .api_version(vk::API_VERSION_1_1);
    let create_info = vk::InstanceCreateInfo::default().application_info(&application);
    // SAFETY: The create info and all chained structures live through the call.
    let instance = unsafe { entry.create_instance(&create_info, None) }
        .map_err(|error| format!("Vulkan instance creation failed: {error}"))?;
    let result = collect_instance(&instance);
    // SAFETY: The instance is no longer used after this point and no child
    // Vulkan objects were created.
    unsafe { instance.destroy_instance(None) };
    result
}

fn collect_instance(instance: &ash::Instance) -> Result<Vec<LocalDeviceTelemetrySnapshot>, String> {
    // SAFETY: `instance` is live and enumeration allocates owned handles.
    let devices = unsafe { instance.enumerate_physical_devices() }
        .map_err(|error| format!("Vulkan physical-device enumeration failed: {error}"))?;
    let mut snapshots = Vec::new();
    for (index, device) in devices.into_iter().enumerate() {
        // SAFETY: The physical-device handle belongs to this live instance.
        let extensions = unsafe { instance.enumerate_device_extension_properties(device) }
            .map_err(|error| format!("Vulkan device-extension query failed: {error}"))?;
        let supports_budget = extensions.iter().any(|extension| {
            // SAFETY: Vulkan guarantees a NUL-terminated extension name array.
            (unsafe { CStr::from_ptr(extension.extension_name.as_ptr()) })
                == MEMORY_BUDGET_EXTENSION
        });
        if !supports_budget {
            continue;
        }

        let (memory_properties, heap_budget, heap_usage) = memory_properties(instance, device);
        let (device_properties, device_uuid) = device_properties(instance, device);

        let Some((total, process_budget, process_usage)) =
            summarize_device_local_heaps(&memory_properties, &heap_budget, &heap_usage)
        else {
            continue;
        };

        let mut snapshot =
            empty_snapshot(LocalDeviceTelemetrySource::VulkanBudget, GpuBackend::Vulkan);
        snapshot.device_index = u32::try_from(index).ok();
        snapshot.device_uuid = uuid_string(&device_uuid);
        // SAFETY: Vulkan guarantees a NUL-terminated device-name array.
        snapshot.device_name = unsafe { CStr::from_ptr(device_properties.device_name.as_ptr()) }
            .to_str()
            .ok()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        snapshot.runtime_version = Some(format_api_version(device_properties.api_version));
        snapshot.memory_total_mb = Some(bytes_to_mb(total));
        snapshot.memory_budget_mb = Some(bytes_to_mb(process_budget));
        snapshot.process_memory_used_mb = Some(bytes_to_mb(process_usage));
        snapshot.memory_used_mb = Some(bytes_to_mb(process_usage));
        snapshot.memory_free_mb = Some(bytes_to_mb(process_budget.saturating_sub(process_usage)));
        populate_missing_fields(&mut snapshot);
        finish_availability(&mut snapshot);
        snapshots.push(snapshot);
    }
    Ok(snapshots)
}

fn summarize_device_local_heaps(
    properties: &vk::PhysicalDeviceMemoryProperties,
    heap_budget: &[u64],
    heap_usage: &[u64],
) -> Option<(u64, u64, u64)> {
    let heap_count = properties.memory_heap_count as usize;
    let mut found_device_local = false;
    let mut total = 0_u64;
    let mut process_budget = 0_u64;
    let mut process_usage = 0_u64;
    for heap_index in 0..heap_count {
        let heap = properties.memory_heaps[heap_index];
        if !heap.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL) {
            continue;
        }
        found_device_local = true;
        total = total.saturating_add(heap.size);
        process_budget = process_budget.saturating_add(heap_budget[heap_index]);
        process_usage = process_usage.saturating_add(heap_usage[heap_index]);
    }
    found_device_local.then_some((total, process_budget, process_usage))
}

fn memory_properties(
    instance: &ash::Instance,
    device: vk::PhysicalDevice,
) -> (vk::PhysicalDeviceMemoryProperties, Vec<u64>, Vec<u64>) {
    let mut budget = vk::PhysicalDeviceMemoryBudgetPropertiesEXT::default();
    let properties = {
        let mut memory = vk::PhysicalDeviceMemoryProperties2::default().push_next(&mut budget);
        // SAFETY: The output structure and pNext chain remain live through the
        // query and are correctly typed for Vulkan 1.1 core properties.
        unsafe { instance.get_physical_device_memory_properties2(device, &mut memory) };
        memory.memory_properties
    };
    let heap_count = properties.memory_heap_count as usize;
    (
        properties,
        budget.heap_budget[..heap_count].to_vec(),
        budget.heap_usage[..heap_count].to_vec(),
    )
}

fn device_properties(
    instance: &ash::Instance,
    device: vk::PhysicalDevice,
) -> (vk::PhysicalDeviceProperties, [u8; vk::UUID_SIZE]) {
    let mut identity = vk::PhysicalDeviceIDProperties::default();
    let properties = {
        let mut properties = vk::PhysicalDeviceProperties2::default().push_next(&mut identity);
        // SAFETY: The output structure and pNext chain remain live through the
        // query and are correctly typed for Vulkan 1.1 core properties.
        unsafe { instance.get_physical_device_properties2(device, &mut properties) };
        properties.properties
    };
    (properties, identity.device_uuid)
}

fn uuid_string(uuid: &[u8; vk::UUID_SIZE]) -> Option<String> {
    if uuid.iter().all(|byte| *byte == 0) {
        return None;
    }
    Some(
        uuid.iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
    )
}

fn format_api_version(version: u32) -> String {
    format!(
        "Vulkan {}.{}.{}",
        vk::api_version_major(version),
        vk::api_version_minor(version),
        vk::api_version_patch(version)
    )
}

fn bytes_to_mb(bytes: u64) -> u64 {
    bytes / 1024 / 1024
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_uuid_stays_unknown() {
        assert_eq!(uuid_string(&[0; vk::UUID_SIZE]), None);
    }

    #[test]
    fn uuid_is_stable_lowercase_hex() {
        let mut uuid = [0; vk::UUID_SIZE];
        uuid[0] = 0xab;
        uuid[15] = 0xcd;
        assert_eq!(
            uuid_string(&uuid).as_deref(),
            Some("ab0000000000000000000000000000cd")
        );
    }

    #[test]
    fn api_version_is_explicit() {
        assert_eq!(
            format_api_version(vk::make_api_version(0, 1, 3, 7)),
            "Vulkan 1.3.7"
        );
    }

    #[test]
    fn zero_dynamic_budget_is_preserved_as_authoritative() {
        let mut memory_heaps = [vk::MemoryHeap::default(); vk::MAX_MEMORY_HEAPS];
        memory_heaps[0] = vk::MemoryHeap {
            size: 8 * 1024 * 1024,
            flags: vk::MemoryHeapFlags::DEVICE_LOCAL,
        };
        memory_heaps[1] = vk::MemoryHeap {
            size: 4 * 1024 * 1024,
            flags: vk::MemoryHeapFlags::empty(),
        };
        let properties = vk::PhysicalDeviceMemoryProperties {
            memory_heap_count: 2,
            memory_heaps,
            ..Default::default()
        };
        assert_eq!(
            summarize_device_local_heaps(&properties, &[0, 99], &[0, 88]),
            Some((8 * 1024 * 1024, 0, 0))
        );
    }
}
