use std::ffi::{CStr, c_char, c_uint, c_void};
use std::path::PathBuf;

use libloading::{Library, Symbol};
use medousa_types::{GpuBackend, LocalDeviceTelemetrySnapshot, LocalDeviceTelemetrySource};

use super::{empty_snapshot, finish_availability, populate_missing_fields};

type NvmlReturn = i32;
type NvmlDevice = *mut c_void;
const NVML_SUCCESS: NvmlReturn = 0;
const NVML_ERROR_INSUFFICIENT_SIZE: NvmlReturn = 7;
const NVML_VALUE_NOT_AVAILABLE: u64 = u64::MAX;
const STRING_BUFFER_LEN: usize = 96;
const PROCESS_QUERY_ATTEMPTS: usize = 3;

#[repr(C)]
struct NvmlMemory {
    total: u64,
    free: u64,
    used: u64,
}

#[repr(C)]
struct NvmlUtilization {
    gpu: u32,
    memory: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct NvmlProcessInfo {
    pid: u32,
    used_gpu_memory: u64,
    gpu_instance_id: u32,
    compute_instance_id: u32,
}

type Init = unsafe extern "C" fn() -> NvmlReturn;
type Shutdown = unsafe extern "C" fn() -> NvmlReturn;
type DeviceCount = unsafe extern "C" fn(*mut c_uint) -> NvmlReturn;
type DeviceByIndex = unsafe extern "C" fn(c_uint, *mut NvmlDevice) -> NvmlReturn;
type DeviceString = unsafe extern "C" fn(NvmlDevice, *mut c_char, c_uint) -> NvmlReturn;
type DeviceMemory = unsafe extern "C" fn(NvmlDevice, *mut NvmlMemory) -> NvmlReturn;
type DeviceProcesses =
    unsafe extern "C" fn(NvmlDevice, *mut c_uint, *mut NvmlProcessInfo) -> NvmlReturn;
type DeviceU32 = unsafe extern "C" fn(NvmlDevice, *mut c_uint) -> NvmlReturn;
type DeviceU64 = unsafe extern "C" fn(NvmlDevice, *mut u64) -> NvmlReturn;
type DeviceEnumU32 = unsafe extern "C" fn(NvmlDevice, c_uint, *mut c_uint) -> NvmlReturn;
type DeviceUtilization = unsafe extern "C" fn(NvmlDevice, *mut NvmlUtilization) -> NvmlReturn;
type SystemString = unsafe extern "C" fn(*mut c_char, c_uint) -> NvmlReturn;

pub(super) fn try_collect() -> Option<Result<Vec<LocalDeviceTelemetrySnapshot>, String>> {
    let library = load_library()?;
    Some(collect(&library))
}

fn load_library() -> Option<Library> {
    for candidate in library_candidates() {
        // SAFETY: Loading a driver-owned dynamic library does not call into it;
        // every symbol is checked before use and remains scoped to the library.
        if let Ok(library) = unsafe { Library::new(&candidate) } {
            return Some(library);
        }
    }
    None
}

fn library_candidates() -> Vec<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let mut candidates = vec![PathBuf::from("nvml.dll")];
        if let Some(program_files) = std::env::var_os("ProgramW6432") {
            candidates.push(
                PathBuf::from(program_files)
                    .join("NVIDIA Corporation")
                    .join("NVSMI")
                    .join("nvml.dll"),
            );
        }
        if let Some(windows) = std::env::var_os("WINDIR") {
            candidates.push(PathBuf::from(windows).join("System32").join("nvml.dll"));
        }
        candidates
    }
    #[cfg(target_os = "linux")]
    {
        vec![
            PathBuf::from("libnvidia-ml.so.1"),
            PathBuf::from("libnvidia-ml.so"),
        ]
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        Vec::new()
    }
}

fn collect(library: &Library) -> Result<Vec<LocalDeviceTelemetrySnapshot>, String> {
    // SAFETY: Symbol signatures mirror the stable NVML C ABI. Symbols never
    // outlive `library`, output buffers are initialized and sized by the caller.
    unsafe {
        let init: Symbol<Init> = symbol(library, b"nvmlInit_v2\0")?;
        let shutdown: Symbol<Shutdown> = symbol(library, b"nvmlShutdown\0")?;
        nvml_ok(init(), "nvmlInit_v2")?;
        let result = collect_initialized(library);
        let shutdown_result = nvml_ok(shutdown(), "nvmlShutdown");
        match result {
            Err(error) => Err(error),
            Ok(snapshots) => shutdown_result.map(|()| snapshots),
        }
    }
}

unsafe fn collect_initialized(
    library: &Library,
) -> Result<Vec<LocalDeviceTelemetrySnapshot>, String> {
    // SAFETY: See `collect`; each requested symbol is validated before calling.
    let count_fn: Symbol<DeviceCount> = unsafe { symbol(library, b"nvmlDeviceGetCount_v2\0")? };
    let device_fn: Symbol<DeviceByIndex> =
        unsafe { symbol(library, b"nvmlDeviceGetHandleByIndex_v2\0")? };
    let uuid_fn: Symbol<DeviceString> = unsafe { symbol(library, b"nvmlDeviceGetUUID\0")? };
    let name_fn: Symbol<DeviceString> = unsafe { symbol(library, b"nvmlDeviceGetName\0")? };
    let memory_fn: Symbol<DeviceMemory> = unsafe { symbol(library, b"nvmlDeviceGetMemoryInfo\0")? };
    let processes_fn: Symbol<DeviceProcesses> =
        unsafe { symbol(library, b"nvmlDeviceGetComputeRunningProcesses_v3\0")? };
    let driver_fn: Symbol<SystemString> =
        unsafe { symbol(library, b"nvmlSystemGetDriverVersion\0")? };
    let driver_version = unsafe { system_string(&driver_fn) };
    let utilization_fn: Option<Symbol<DeviceUtilization>> =
        unsafe { optional_symbol(library, b"nvmlDeviceGetUtilizationRates\0") };
    let power_fn: Option<Symbol<DeviceU32>> =
        unsafe { optional_symbol(library, b"nvmlDeviceGetPowerUsage\0") };
    let temperature_fn: Option<Symbol<DeviceEnumU32>> =
        unsafe { optional_symbol(library, b"nvmlDeviceGetTemperature\0") };
    let clock_fn: Option<Symbol<DeviceEnumU32>> =
        unsafe { optional_symbol(library, b"nvmlDeviceGetClockInfo\0") };
    let throttle_fn: Option<Symbol<DeviceU64>> = unsafe {
        optional_symbol(library, b"nvmlDeviceGetCurrentClocksEventReasons\0")
            .or_else(|| optional_symbol(library, b"nvmlDeviceGetCurrentClocksThrottleReasons\0"))
    };

    let mut count = 0;
    nvml_ok(unsafe { count_fn(&mut count) }, "nvmlDeviceGetCount_v2")?;
    let mut snapshots = Vec::with_capacity(count as usize);
    for index in 0..count {
        let mut device = std::ptr::null_mut();
        nvml_ok(
            unsafe { device_fn(index, &mut device) },
            "nvmlDeviceGetHandleByIndex_v2",
        )?;
        let mut memory = NvmlMemory {
            total: 0,
            free: 0,
            used: 0,
        };
        nvml_ok(
            unsafe { memory_fn(device, &mut memory) },
            "nvmlDeviceGetMemoryInfo",
        )?;
        let mut snapshot = empty_snapshot(LocalDeviceTelemetrySource::Nvml, GpuBackend::Cuda);
        snapshot.device_index = Some(index);
        snapshot.device_uuid = unsafe { device_string(device, &uuid_fn) };
        snapshot.device_name = unsafe { device_string(device, &name_fn) };
        snapshot.driver_version.clone_from(&driver_version);
        snapshot.memory_total_mb = Some(bytes_to_mb(memory.total));
        snapshot.memory_used_mb = Some(bytes_to_mb(memory.used));
        snapshot.memory_free_mb = Some(bytes_to_mb(memory.free));
        snapshot.process_memory_used_mb = unsafe { current_process_memory(device, &processes_fn)? };
        snapshot.utilization_percent = unsafe { device_utilization(device, &utilization_fn) };
        snapshot.power_watts =
            unsafe { device_u32(device, &power_fn) }.map(|value| f64::from(value) / 1_000.0);
        snapshot.temperature_c =
            unsafe { device_enum_u32(device, 0, &temperature_fn) }.map(f64::from);
        snapshot.graphics_clock_mhz =
            unsafe { device_enum_u32(device, 1, &clock_fn) }.map(u64::from);
        snapshot.memory_clock_mhz = unsafe { device_enum_u32(device, 2, &clock_fn) }.map(u64::from);
        snapshot.throttle_reasons =
            unsafe { device_u64(device, &throttle_fn) }.map(decode_throttle_reasons);
        populate_missing_fields(&mut snapshot);
        finish_availability(&mut snapshot);
        snapshots.push(snapshot);
    }
    Ok(snapshots)
}

unsafe fn device_utilization(
    device: NvmlDevice,
    function: &Option<Symbol<DeviceUtilization>>,
) -> Option<f64> {
    let function = function.as_ref()?;
    let mut utilization = NvmlUtilization { gpu: 0, memory: 0 };
    (unsafe { function(device, &mut utilization) } == NVML_SUCCESS)
        .then_some(f64::from(utilization.gpu))
}

unsafe fn device_u32(device: NvmlDevice, function: &Option<Symbol<DeviceU32>>) -> Option<u32> {
    let function = function.as_ref()?;
    let mut value = 0;
    (unsafe { function(device, &mut value) } == NVML_SUCCESS).then_some(value)
}

unsafe fn device_u64(device: NvmlDevice, function: &Option<Symbol<DeviceU64>>) -> Option<u64> {
    let function = function.as_ref()?;
    let mut value = 0;
    (unsafe { function(device, &mut value) } == NVML_SUCCESS).then_some(value)
}

unsafe fn device_enum_u32(
    device: NvmlDevice,
    kind: u32,
    function: &Option<Symbol<DeviceEnumU32>>,
) -> Option<u32> {
    let function = function.as_ref()?;
    let mut value = 0;
    (unsafe { function(device, kind, &mut value) } == NVML_SUCCESS).then_some(value)
}

unsafe fn system_string(function: &SystemString) -> Option<String> {
    let mut buffer = [0 as c_char; STRING_BUFFER_LEN];
    if unsafe { function(buffer.as_mut_ptr(), buffer.len() as c_uint) } != NVML_SUCCESS {
        return None;
    }
    buffer_string(&buffer)
}

unsafe fn device_string(device: NvmlDevice, function: &DeviceString) -> Option<String> {
    let mut buffer = [0 as c_char; STRING_BUFFER_LEN];
    if unsafe { function(device, buffer.as_mut_ptr(), buffer.len() as c_uint) } != NVML_SUCCESS {
        return None;
    }
    buffer_string(&buffer)
}

fn buffer_string(buffer: &[c_char]) -> Option<String> {
    // SAFETY: Callers only pass fixed-size buffers that NVML has successfully
    // populated with a NUL-terminated string.
    unsafe { CStr::from_ptr(buffer.as_ptr()) }
        .to_str()
        .ok()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

unsafe fn current_process_memory(
    device: NvmlDevice,
    function: &DeviceProcesses,
) -> Result<Option<u64>, String> {
    let mut count = 0;
    let first = unsafe { function(device, &mut count, std::ptr::null_mut()) };
    if first == NVML_SUCCESS && count == 0 {
        return Ok(Some(0));
    }
    if first != NVML_ERROR_INSUFFICIENT_SIZE {
        return Err(format!(
            "nvmlDeviceGetComputeRunningProcesses_v3 failed with NVML status {first}"
        ));
    }
    for _ in 0..PROCESS_QUERY_ATTEMPTS {
        let mut processes = vec![NvmlProcessInfo::default(); count.saturating_add(8) as usize];
        count = processes.len() as c_uint;
        let status = unsafe { function(device, &mut count, processes.as_mut_ptr()) };
        if status == NVML_SUCCESS {
            processes.truncate(count as usize);
            return Ok(process_memory_for_pid(&processes, std::process::id()));
        }
        if status != NVML_ERROR_INSUFFICIENT_SIZE {
            return Err(format!(
                "nvmlDeviceGetComputeRunningProcesses_v3 failed with NVML status {status}"
            ));
        }
    }
    Err("nvmlDeviceGetComputeRunningProcesses_v3 process list kept changing".to_string())
}

fn process_memory_for_pid(processes: &[NvmlProcessInfo], pid: u32) -> Option<u64> {
    let matching: Vec<_> = processes
        .iter()
        .filter(|process| process.pid == pid)
        .collect();
    if matching
        .iter()
        .any(|process| process.used_gpu_memory == NVML_VALUE_NOT_AVAILABLE)
    {
        return None;
    }
    Some(bytes_to_mb(
        matching.into_iter().fold(0_u64, |total, process| {
            total.saturating_add(process.used_gpu_memory)
        }),
    ))
}

fn decode_throttle_reasons(mask: u64) -> Vec<String> {
    const REASONS: [(u64, &str); 9] = [
        (0x001, "gpuIdle"),
        (0x002, "applicationsClockSetting"),
        (0x004, "softwarePowerCap"),
        (0x008, "hardwareSlowdown"),
        (0x010, "syncBoost"),
        (0x020, "softwareThermalSlowdown"),
        (0x040, "hardwareThermalSlowdown"),
        (0x080, "hardwarePowerBrakeSlowdown"),
        (0x100, "displayClockSetting"),
    ];
    let mut reasons: Vec<_> = REASONS
        .iter()
        .filter(|(bit, _)| mask & bit != 0)
        .map(|(_, name)| (*name).to_string())
        .collect();
    let known = REASONS.iter().fold(0_u64, |mask, (bit, _)| mask | bit);
    let unknown = mask & !known;
    if unknown != 0 {
        reasons.push(format!("unknown:0x{unknown:x}"));
    }
    reasons
}

unsafe fn symbol<'library, T>(
    library: &'library Library,
    name: &[u8],
) -> Result<Symbol<'library, T>, String> {
    // SAFETY: Callers supply a C ABI signature documented by NVML.
    unsafe { library.get(name) }.map_err(|error| {
        format!(
            "NVML is installed but required symbol {} is unavailable: {error}",
            String::from_utf8_lossy(name).trim_end_matches('\0')
        )
    })
}

unsafe fn optional_symbol<'library, T>(
    library: &'library Library,
    name: &[u8],
) -> Option<Symbol<'library, T>> {
    // SAFETY: Callers supply a C ABI signature documented by NVML. Optional
    // diagnostic symbols may be absent on older drivers.
    unsafe { library.get(name) }.ok()
}

fn nvml_ok(status: NvmlReturn, operation: &str) -> Result<(), String> {
    if status == NVML_SUCCESS {
        Ok(())
    } else {
        Err(format!("{operation} failed with NVML status {status}"))
    }
}

fn bytes_to_mb(bytes: u64) -> u64 {
    bytes / 1024 / 1024
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_candidates_prefer_the_versioned_driver_soname() {
        #[cfg(target_os = "linux")]
        assert_eq!(library_candidates()[0], PathBuf::from("libnvidia-ml.so.1"));
    }

    #[test]
    fn nvml_bytes_use_binary_mebibytes() {
        assert_eq!(bytes_to_mb(3 * 1024 * 1024 + 512), 3);
    }

    #[test]
    fn process_memory_sums_current_process_contexts() {
        let processes = [
            NvmlProcessInfo {
                pid: 42,
                used_gpu_memory: 2 * 1024 * 1024,
                ..Default::default()
            },
            NvmlProcessInfo {
                pid: 42,
                used_gpu_memory: 3 * 1024 * 1024,
                ..Default::default()
            },
            NvmlProcessInfo {
                pid: 7,
                used_gpu_memory: 99 * 1024 * 1024,
                ..Default::default()
            },
        ];
        assert_eq!(process_memory_for_pid(&processes, 42), Some(5));
        assert_eq!(process_memory_for_pid(&processes, 100), Some(0));
    }

    #[test]
    fn unavailable_wddm_process_memory_stays_unknown() {
        let processes = [NvmlProcessInfo {
            pid: 42,
            used_gpu_memory: NVML_VALUE_NOT_AVAILABLE,
            ..Default::default()
        }];
        assert_eq!(process_memory_for_pid(&processes, 42), None);
    }

    #[test]
    fn throttle_reasons_preserve_known_and_future_bits() {
        assert!(decode_throttle_reasons(0).is_empty());
        assert_eq!(
            decode_throttle_reasons(0x004 | 0x040 | 0x400),
            vec![
                "softwarePowerCap".to_string(),
                "hardwareThermalSlowdown".to_string(),
                "unknown:0x400".to_string(),
            ]
        );
    }
}
