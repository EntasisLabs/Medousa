use std::ffi::{c_char, c_void};
use std::path::PathBuf;

use libloading::{Library, Symbol};
use medousa_types::{GpuBackend, LocalDeviceTelemetrySnapshot, LocalDeviceTelemetrySource};

use super::{empty_snapshot, finish_availability, populate_missing_fields};

type AmdSmiStatus = u32;
type ProcessorHandle = *mut c_void;
type SocketHandle = *mut c_void;

const AMDSMI_STATUS_SUCCESS: AmdSmiStatus = 0;
const AMDSMI_STATUS_OUT_OF_RESOURCES: AmdSmiStatus = 15;
const AMDSMI_INIT_AMD_GPUS: u64 = 1 << 1;
const SUPPORTED_ABI_MAJOR: u32 = 26;
const STRING_LENGTH: usize = 256;
const UUID_LENGTH: usize = 38;
const QUERY_ATTEMPTS: usize = 3;

#[repr(C)]
struct AmdSmiVersion {
    major: u32,
    minor: u32,
    release: u32,
    build: *const c_char,
}

#[repr(C)]
struct AmdSmiVramUsage {
    total_mb: u32,
    used_mb: u32,
    reserved: [u32; 2],
}

#[repr(C)]
struct AmdSmiEngineUsage {
    gfx_activity: u32,
    memory_activity: u32,
    multimedia_activity: u32,
    reserved: [u32; 13],
}

#[repr(C)]
struct AmdSmiPowerInfo {
    socket_power: u64,
    current_socket_power: u32,
    average_socket_power: u32,
    gfx_voltage: u64,
    soc_voltage: u64,
    memory_voltage: u64,
    power_limit: u32,
    reserved: [u64; 18],
}

#[repr(C)]
struct AmdSmiClockInfo {
    clock_mhz: u32,
    min_clock_mhz: u32,
    max_clock_mhz: u32,
    locked: u8,
    deep_sleep: u8,
    reserved: [u32; 4],
}

#[repr(C)]
struct AmdSmiDriverInfo {
    version: [c_char; STRING_LENGTH],
    date: [c_char; STRING_LENGTH],
    name: [c_char; STRING_LENGTH],
}

#[repr(C)]
struct AmdSmiAsicInfo {
    market_name: [c_char; STRING_LENGTH],
    vendor_id: u32,
    vendor_name: [c_char; STRING_LENGTH],
    subvendor_id: u32,
    device_id: u64,
    revision_id: u32,
    serial: [c_char; STRING_LENGTH],
    oam_id: u32,
    compute_units: u32,
    target_graphics_version: u64,
    subsystem_id: u32,
    reserved: [u32; 21],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct AmdSmiProcessEngineUsage {
    graphics_ns: u64,
    encode_ns: u64,
    reserved: [u32; 12],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct AmdSmiProcessMemoryUsage {
    gtt_bytes: u64,
    cpu_bytes: u64,
    vram_bytes: u64,
    reserved: [u32; 10],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct AmdSmiProcessInfo {
    name: [c_char; STRING_LENGTH],
    pid: u32,
    memory_bytes: u64,
    engine_usage: AmdSmiProcessEngineUsage,
    memory_usage: AmdSmiProcessMemoryUsage,
    container_name: [c_char; STRING_LENGTH],
    compute_units: u32,
    evicted_time_ms: u32,
    reserved: [u32; 10],
}

impl Default for AmdSmiProcessInfo {
    fn default() -> Self {
        // SAFETY: This C data structure contains only integers and character
        // arrays, for which all-zero is a valid initialization state.
        unsafe { std::mem::zeroed() }
    }
}

type GetVersion = unsafe extern "C" fn(*mut AmdSmiVersion) -> AmdSmiStatus;
type Init = unsafe extern "C" fn(u64) -> AmdSmiStatus;
type Shutdown = unsafe extern "C" fn() -> AmdSmiStatus;
type GetSockets = unsafe extern "C" fn(*mut u32, *mut SocketHandle) -> AmdSmiStatus;
type GetProcessors =
    unsafe extern "C" fn(SocketHandle, *mut u32, *mut ProcessorHandle) -> AmdSmiStatus;
type GetUuid = unsafe extern "C" fn(ProcessorHandle, *mut u32, *mut c_char) -> AmdSmiStatus;
type GetVram = unsafe extern "C" fn(ProcessorHandle, *mut AmdSmiVramUsage) -> AmdSmiStatus;
type GetActivity = unsafe extern "C" fn(ProcessorHandle, *mut AmdSmiEngineUsage) -> AmdSmiStatus;
type GetPower = unsafe extern "C" fn(ProcessorHandle, *mut AmdSmiPowerInfo) -> AmdSmiStatus;
type GetClock = unsafe extern "C" fn(ProcessorHandle, u32, *mut AmdSmiClockInfo) -> AmdSmiStatus;
type GetTemperature = unsafe extern "C" fn(ProcessorHandle, u32, u32, *mut i64) -> AmdSmiStatus;
type GetDriver = unsafe extern "C" fn(ProcessorHandle, *mut AmdSmiDriverInfo) -> AmdSmiStatus;
type GetAsic = unsafe extern "C" fn(ProcessorHandle, *mut AmdSmiAsicInfo) -> AmdSmiStatus;
type GetProcesses =
    unsafe extern "C" fn(ProcessorHandle, *mut u32, *mut AmdSmiProcessInfo) -> AmdSmiStatus;

pub(super) fn try_collect() -> Option<Result<Vec<LocalDeviceTelemetrySnapshot>, String>> {
    let library = load_library()?;
    Some(collect(&library))
}

fn load_library() -> Option<Library> {
    for candidate in library_candidates() {
        // SAFETY: Loading the driver companion library does not call it. Every
        // symbol is checked and remains bounded by the library lifetime.
        if let Ok(library) = unsafe { Library::new(candidate) } {
            return Some(library);
        }
    }
    None
}

fn library_candidates() -> Vec<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        let mut candidates = vec![
            PathBuf::from("libamd_smi.so.1"),
            PathBuf::from("libamd_smi.so"),
            PathBuf::from("/opt/rocm/lib/libamd_smi.so"),
            PathBuf::from("/opt/rocm/lib64/libamd_smi.so"),
        ];
        if let Ok(entries) = std::fs::read_dir("/opt/rocm") {
            for entry in entries.flatten() {
                if entry.file_name().to_string_lossy().starts_with("core-") {
                    candidates.push(entry.path().join("lib/libamd_smi.so"));
                    candidates.push(entry.path().join("lib64/libamd_smi.so"));
                }
            }
        }
        candidates
    }
    #[cfg(not(target_os = "linux"))]
    {
        Vec::new()
    }
}

fn collect(library: &Library) -> Result<Vec<LocalDeviceTelemetrySnapshot>, String> {
    // SAFETY: Function signatures and structures mirror AMD SMI ABI major 26.
    // The major is checked before initialization or any versioned structure use.
    unsafe {
        let get_version: Symbol<GetVersion> = symbol(library, b"amdsmi_get_lib_version\0")?;
        let mut version = AmdSmiVersion {
            major: 0,
            minor: 0,
            release: 0,
            build: std::ptr::null(),
        };
        amd_ok(get_version(&mut version), "amdsmi_get_lib_version")?;
        if version.major != SUPPORTED_ABI_MAJOR {
            return Err(format!(
                "AMD SMI ABI {}.{}.{} is unsupported; native collection requires major {}",
                version.major, version.minor, version.release, SUPPORTED_ABI_MAJOR
            ));
        }

        let init: Symbol<Init> = symbol(library, b"amdsmi_init\0")?;
        let shutdown: Symbol<Shutdown> = symbol(library, b"amdsmi_shut_down\0")?;
        amd_ok(init(AMDSMI_INIT_AMD_GPUS), "amdsmi_init")?;
        let result = collect_initialized(library);
        let shutdown_result = amd_ok(shutdown(), "amdsmi_shut_down");
        match result {
            Err(error) => Err(error),
            Ok(snapshots) => shutdown_result.map(|()| snapshots),
        }
    }
}

unsafe fn collect_initialized(
    library: &Library,
) -> Result<Vec<LocalDeviceTelemetrySnapshot>, String> {
    let sockets_fn: Symbol<GetSockets> =
        unsafe { symbol(library, b"amdsmi_get_socket_handles\0")? };
    let processors_fn: Symbol<GetProcessors> =
        unsafe { symbol(library, b"amdsmi_get_processor_handles\0")? };
    let uuid_fn: Symbol<GetUuid> = unsafe { symbol(library, b"amdsmi_get_gpu_device_uuid\0")? };
    let vram_fn: Symbol<GetVram> = unsafe { symbol(library, b"amdsmi_get_gpu_vram_usage\0")? };
    let activity_fn: Option<Symbol<GetActivity>> =
        unsafe { optional_symbol(library, b"amdsmi_get_gpu_activity\0") };
    let power_fn: Option<Symbol<GetPower>> =
        unsafe { optional_symbol(library, b"amdsmi_get_power_info\0") };
    let clock_fn: Option<Symbol<GetClock>> =
        unsafe { optional_symbol(library, b"amdsmi_get_clock_info\0") };
    let temperature_fn: Option<Symbol<GetTemperature>> =
        unsafe { optional_symbol(library, b"amdsmi_get_temp_metric\0") };
    let driver_fn: Option<Symbol<GetDriver>> =
        unsafe { optional_symbol(library, b"amdsmi_get_gpu_driver_info\0") };
    let asic_fn: Option<Symbol<GetAsic>> =
        unsafe { optional_symbol(library, b"amdsmi_get_gpu_asic_info\0") };
    let processes_fn: Option<Symbol<GetProcesses>> =
        unsafe { optional_symbol(library, b"amdsmi_get_gpu_process_list\0") };

    let sockets = unsafe { query_sockets(&sockets_fn)? };
    let mut snapshots = Vec::new();
    for socket in sockets {
        for processor in unsafe { query_processors(socket, &processors_fn)? } {
            let index = u32::try_from(snapshots.len())
                .map_err(|_| "AMD SMI returned too many devices".to_string())?;
            let mut vram = AmdSmiVramUsage {
                total_mb: 0,
                used_mb: 0,
                reserved: [0; 2],
            };
            amd_ok(
                unsafe { vram_fn(processor, &mut vram) },
                "amdsmi_get_gpu_vram_usage",
            )?;
            let mut snapshot =
                empty_snapshot(LocalDeviceTelemetrySource::AmdSmiLibrary, GpuBackend::Rocm);
            snapshot.device_index = Some(index);
            snapshot.device_uuid = unsafe { device_uuid(processor, &uuid_fn) };
            snapshot.device_name = unsafe { device_name(processor, &asic_fn) };
            snapshot.driver_version = unsafe { driver_version(processor, &driver_fn) };
            snapshot.memory_total_mb = Some(u64::from(vram.total_mb));
            snapshot.memory_used_mb = Some(u64::from(vram.used_mb));
            snapshot.memory_free_mb = Some(u64::from(vram.total_mb.saturating_sub(vram.used_mb)));
            snapshot.process_memory_used_mb =
                unsafe { current_process_memory(processor, &processes_fn) };
            snapshot.utilization_percent = unsafe { utilization(processor, &activity_fn) };
            snapshot.power_watts = unsafe { power(processor, &power_fn) };
            snapshot.temperature_c = unsafe { temperature(processor, &temperature_fn) };
            snapshot.graphics_clock_mhz = unsafe { clock(processor, 0, &clock_fn) };
            snapshot.memory_clock_mhz = unsafe { clock(processor, 4, &clock_fn) };
            populate_missing_fields(&mut snapshot);
            finish_availability(&mut snapshot);
            snapshots.push(snapshot);
        }
    }
    Ok(snapshots)
}

unsafe fn query_sockets(function: &GetSockets) -> Result<Vec<SocketHandle>, String> {
    let mut count = 0;
    amd_ok(
        unsafe { function(&mut count, std::ptr::null_mut()) },
        "amdsmi_get_socket_handles(count)",
    )?;
    let mut handles = vec![std::ptr::null_mut(); count as usize];
    amd_ok(
        unsafe { function(&mut count, handles.as_mut_ptr()) },
        "amdsmi_get_socket_handles",
    )?;
    handles.truncate(count as usize);
    Ok(handles)
}

unsafe fn query_processors(
    socket: SocketHandle,
    function: &GetProcessors,
) -> Result<Vec<ProcessorHandle>, String> {
    let mut count = 0;
    amd_ok(
        unsafe { function(socket, &mut count, std::ptr::null_mut()) },
        "amdsmi_get_processor_handles(count)",
    )?;
    let mut handles = vec![std::ptr::null_mut(); count as usize];
    amd_ok(
        unsafe { function(socket, &mut count, handles.as_mut_ptr()) },
        "amdsmi_get_processor_handles",
    )?;
    handles.truncate(count as usize);
    Ok(handles)
}

unsafe fn device_uuid(processor: ProcessorHandle, function: &GetUuid) -> Option<String> {
    let mut buffer = [0 as c_char; UUID_LENGTH];
    let mut length = buffer.len() as u32;
    (unsafe { function(processor, &mut length, buffer.as_mut_ptr()) } == AMDSMI_STATUS_SUCCESS)
        .then(|| c_buffer_string(&buffer))
        .flatten()
}

unsafe fn device_name(
    processor: ProcessorHandle,
    function: &Option<Symbol<GetAsic>>,
) -> Option<String> {
    let function = function.as_ref()?;
    // SAFETY: ABI major 26 guarantees this structure layout and zero is valid.
    let mut info: AmdSmiAsicInfo = unsafe { std::mem::zeroed() };
    (unsafe { function(processor, &mut info) } == AMDSMI_STATUS_SUCCESS)
        .then(|| c_buffer_string(&info.market_name))
        .flatten()
}

unsafe fn driver_version(
    processor: ProcessorHandle,
    function: &Option<Symbol<GetDriver>>,
) -> Option<String> {
    let function = function.as_ref()?;
    // SAFETY: ABI major 26 guarantees this structure layout and zero is valid.
    let mut info: AmdSmiDriverInfo = unsafe { std::mem::zeroed() };
    (unsafe { function(processor, &mut info) } == AMDSMI_STATUS_SUCCESS)
        .then(|| c_buffer_string(&info.version))
        .flatten()
}

unsafe fn utilization(
    processor: ProcessorHandle,
    function: &Option<Symbol<GetActivity>>,
) -> Option<f64> {
    let function = function.as_ref()?;
    let mut info = AmdSmiEngineUsage {
        gfx_activity: 0,
        memory_activity: 0,
        multimedia_activity: 0,
        reserved: [0; 13],
    };
    (unsafe { function(processor, &mut info) } == AMDSMI_STATUS_SUCCESS)
        .then_some(f64::from(info.gfx_activity))
}

unsafe fn power(processor: ProcessorHandle, function: &Option<Symbol<GetPower>>) -> Option<f64> {
    let function = function.as_ref()?;
    // SAFETY: ABI major 26 guarantees this structure layout and zero is valid.
    let mut info: AmdSmiPowerInfo = unsafe { std::mem::zeroed() };
    if unsafe { function(processor, &mut info) } != AMDSMI_STATUS_SUCCESS {
        return None;
    }
    if info.current_socket_power != u32::MAX {
        Some(f64::from(info.current_socket_power))
    } else if info.average_socket_power != u32::MAX {
        Some(f64::from(info.average_socket_power))
    } else if info.socket_power != u64::MAX {
        Some(info.socket_power as f64)
    } else {
        None
    }
}

unsafe fn temperature(
    processor: ProcessorHandle,
    function: &Option<Symbol<GetTemperature>>,
) -> Option<f64> {
    let function = function.as_ref()?;
    let mut value = 0_i64;
    (unsafe { function(processor, 0, 0, &mut value) } == AMDSMI_STATUS_SUCCESS)
        .then_some(value as f64)
}

unsafe fn clock(
    processor: ProcessorHandle,
    clock_type: u32,
    function: &Option<Symbol<GetClock>>,
) -> Option<u64> {
    let function = function.as_ref()?;
    // SAFETY: ABI major 26 guarantees this structure layout and zero is valid.
    let mut info: AmdSmiClockInfo = unsafe { std::mem::zeroed() };
    (unsafe { function(processor, clock_type, &mut info) } == AMDSMI_STATUS_SUCCESS)
        .then_some(u64::from(info.clock_mhz))
}

unsafe fn current_process_memory(
    processor: ProcessorHandle,
    function: &Option<Symbol<GetProcesses>>,
) -> Option<u64> {
    let function = function.as_ref()?;
    let mut count = 0;
    let first = unsafe { function(processor, &mut count, std::ptr::null_mut()) };
    if first == AMDSMI_STATUS_SUCCESS && count == 0 {
        return Some(0);
    }
    if first != AMDSMI_STATUS_SUCCESS && first != AMDSMI_STATUS_OUT_OF_RESOURCES {
        return None;
    }
    for _ in 0..QUERY_ATTEMPTS {
        let mut processes = vec![AmdSmiProcessInfo::default(); count.saturating_add(8) as usize];
        count = processes.len() as u32;
        let status = unsafe { function(processor, &mut count, processes.as_mut_ptr()) };
        if status == AMDSMI_STATUS_SUCCESS {
            processes.truncate(count as usize);
            return Some(process_memory_for_pid(&processes, std::process::id()));
        }
        if status != AMDSMI_STATUS_OUT_OF_RESOURCES {
            return None;
        }
    }
    None
}

fn process_memory_for_pid(processes: &[AmdSmiProcessInfo], pid: u32) -> u64 {
    let bytes =
        processes
            .iter()
            .filter(|process| process.pid == pid)
            .fold(0_u64, |total, process| {
                total
                    .saturating_add(process.memory_usage.vram_bytes)
                    .saturating_add(process.memory_usage.gtt_bytes)
            });
    bytes / 1024 / 1024
}

fn c_buffer_string(buffer: &[c_char]) -> Option<String> {
    let bytes: Vec<u8> = buffer
        .iter()
        .take_while(|value| **value != 0)
        .map(|value| *value as u8)
        .collect();
    std::str::from_utf8(&bytes)
        .ok()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

unsafe fn symbol<'library, T>(
    library: &'library Library,
    name: &[u8],
) -> Result<Symbol<'library, T>, String> {
    // SAFETY: Callers provide signatures from the ABI-major-26 AMD SMI header.
    unsafe { library.get(name) }.map_err(|error| {
        format!(
            "AMD SMI is installed but required symbol {} is unavailable: {error}",
            String::from_utf8_lossy(name).trim_end_matches('\0')
        )
    })
}

unsafe fn optional_symbol<'library, T>(
    library: &'library Library,
    name: &[u8],
) -> Option<Symbol<'library, T>> {
    // SAFETY: Callers provide signatures from the ABI-major-26 AMD SMI header.
    unsafe { library.get(name) }.ok()
}

fn amd_ok(status: AmdSmiStatus, operation: &str) -> Result<(), String> {
    if status == AMDSMI_STATUS_SUCCESS {
        Ok(())
    } else {
        Err(format!("{operation} failed with AMD SMI status {status}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_candidates_include_driver_and_rocm_locations() {
        #[cfg(target_os = "linux")]
        {
            let candidates = library_candidates();
            assert_eq!(candidates[0], PathBuf::from("libamd_smi.so.1"));
            assert!(candidates.contains(&PathBuf::from("/opt/rocm/lib/libamd_smi.so")));
        }
    }

    #[test]
    fn process_memory_combines_vram_and_gtt_for_target_pid() {
        let target = AmdSmiProcessInfo {
            pid: 42,
            memory_usage: AmdSmiProcessMemoryUsage {
                gtt_bytes: 2 * 1024 * 1024,
                cpu_bytes: 0,
                vram_bytes: 3 * 1024 * 1024,
                reserved: [0; 10],
            },
            ..Default::default()
        };
        let other = AmdSmiProcessInfo {
            pid: 7,
            memory_usage: AmdSmiProcessMemoryUsage {
                gtt_bytes: 0,
                cpu_bytes: 0,
                vram_bytes: 100 * 1024 * 1024,
                reserved: [0; 10],
            },
            ..Default::default()
        };
        assert_eq!(process_memory_for_pid(&[target, other], 42), 5);
        assert_eq!(process_memory_for_pid(&[target, other], 100), 0);
    }

    #[test]
    fn c_strings_work_with_signed_or_unsigned_c_char() {
        let mut buffer = [0 as c_char; 8];
        for (target, source) in buffer.iter_mut().zip(b"MI300\0") {
            *target = *source as c_char;
        }
        assert_eq!(c_buffer_string(&buffer).as_deref(), Some("MI300"));
    }
}
