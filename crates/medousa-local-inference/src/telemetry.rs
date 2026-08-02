use std::io::Read;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use chrono::Utc;
use medousa_types::{
    GpuBackend, LocalDeviceTelemetryAvailability, LocalDeviceTelemetrySnapshot,
    LocalDeviceTelemetrySource,
};
use serde_json::Value;

const DEVICE_FIELDS: [&str; 15] = [
    "unifiedMemory",
    "memoryTotalMb",
    "memoryBudgetMb",
    "memoryUsedMb",
    "memoryFreeMb",
    "processMemoryUsedMb",
    "recommendedWorkingSetMb",
    "utilizationPercent",
    "powerWatts",
    "temperatureC",
    "graphicsClockMhz",
    "memoryClockMhz",
    "driverVersion",
    "runtimeVersion",
    "throttleReasons",
];

/// Collects device-native counters without making any unavailable value look
/// like zero. A source that is not installed is not applicable and is omitted;
/// a source that is present but fails produces an explicit unavailable record.
pub fn collect_device_telemetry() -> Vec<LocalDeviceTelemetrySnapshot> {
    let mut snapshots = Vec::new();

    #[cfg(all(target_os = "macos", feature = "telemetry-metal"))]
    snapshots.push(collect_metal());

    if let Some(result) = run_optional(
        "nvidia-smi",
        &[
            "--query-gpu=index,uuid,name,driver_version,memory.total,memory.used,memory.free,utilization.gpu,power.draw,temperature.gpu,clocks.sm,clocks.mem",
            "--format=csv,noheader,nounits",
        ],
    ) {
        match result {
            Ok(output) if output.status.success() => {
                let process_memory = collect_nvidia_process_memory();
                let parsed = parse_nvidia_csv(
                    &String::from_utf8_lossy(&output.stdout),
                    process_memory.as_ref().ok(),
                );
                if parsed.is_empty() {
                    snapshots.push(unavailable_snapshot(
                        LocalDeviceTelemetrySource::NvidiaSmi,
                        GpuBackend::Cuda,
                        "nvidia-smi returned no GPU rows",
                    ));
                } else {
                    snapshots.extend(parsed);
                }
            }
            Ok(output) => snapshots.push(unavailable_snapshot(
                LocalDeviceTelemetrySource::NvidiaSmi,
                GpuBackend::Cuda,
                &command_error("nvidia-smi", &output),
            )),
            Err(error) => snapshots.push(unavailable_snapshot(
                LocalDeviceTelemetrySource::NvidiaSmi,
                GpuBackend::Cuda,
                &error,
            )),
        }
    }

    if let Some(result) = run_optional("amd-smi", &["--json"]) {
        match result {
            Ok(output) if output.status.success() => match parse_amd_json(&output.stdout) {
                Ok(parsed) => snapshots.extend(parsed),
                Err(error) => snapshots.push(unavailable_snapshot(
                    LocalDeviceTelemetrySource::AmdSmi,
                    GpuBackend::Rocm,
                    &error,
                )),
            },
            Ok(output) => snapshots.push(unavailable_snapshot(
                LocalDeviceTelemetrySource::AmdSmi,
                GpuBackend::Rocm,
                &command_error("amd-smi", &output),
            )),
            Err(error) => snapshots.push(unavailable_snapshot(
                LocalDeviceTelemetrySource::AmdSmi,
                GpuBackend::Rocm,
                &error,
            )),
        }
    }

    snapshots
}

#[cfg(all(target_os = "macos", feature = "telemetry-metal"))]
fn collect_metal() -> LocalDeviceTelemetrySnapshot {
    let Some(device) = metal::Device::system_default() else {
        return unavailable_snapshot(
            LocalDeviceTelemetrySource::MetalApi,
            GpuBackend::Metal,
            "Metal reported no system default device",
        );
    };
    let used_mb = bytes_to_mb(device.current_allocated_size());
    let working_set_mb = bytes_to_mb(device.recommended_max_working_set_size());
    let mut snapshot = empty_snapshot(LocalDeviceTelemetrySource::MetalApi, GpuBackend::Metal);
    snapshot.device_index = Some(0);
    snapshot.device_name = Some(device.name().to_string());
    snapshot.unified_memory = Some(device.has_unified_memory());
    snapshot.memory_used_mb = Some(used_mb);
    snapshot.memory_free_mb = Some(working_set_mb.saturating_sub(used_mb));
    snapshot.process_memory_used_mb = Some(used_mb);
    snapshot.recommended_working_set_mb = Some(working_set_mb);
    snapshot.unavailable_fields = vec![
        "memoryTotalMb",
        "utilizationPercent",
        "powerWatts",
        "temperatureC",
        "graphicsClockMhz",
        "memoryClockMhz",
        "driverVersion",
        "runtimeVersion",
        "throttleReasons",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    finish_availability(&mut snapshot);
    snapshot
}

fn collect_nvidia_process_memory() -> Result<Vec<(String, u32, Option<u64>)>, String> {
    let Some(result) = run_optional(
        "nvidia-smi",
        &[
            "--query-compute-apps=gpu_uuid,pid,used_gpu_memory",
            "--format=csv,noheader,nounits",
        ],
    ) else {
        return Err("nvidia-smi disappeared while collecting process memory".to_string());
    };
    let output = result?;
    if !output.status.success() {
        return Err(command_error("nvidia-smi process query", &output));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split(',').map(str::trim).collect();
            if fields.len() != 3 {
                return None;
            }
            Some((
                fields[0].to_string(),
                fields[1].parse::<u32>().ok()?,
                parse_u64(fields[2]),
            ))
        })
        .collect())
}

fn parse_nvidia_csv(
    raw: &str,
    process_rows: Option<&Vec<(String, u32, Option<u64>)>>,
) -> Vec<LocalDeviceTelemetrySnapshot> {
    raw.lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| {
            let fields: Vec<_> = line.split(',').map(str::trim).collect();
            if fields.len() != 12 {
                return None;
            }
            let mut snapshot =
                empty_snapshot(LocalDeviceTelemetrySource::NvidiaSmi, GpuBackend::Cuda);
            snapshot.device_index = fields[0].parse().ok();
            snapshot.device_uuid = parse_text(fields[1]);
            snapshot.device_name = parse_text(fields[2]);
            snapshot.driver_version = parse_text(fields[3]);
            snapshot.memory_total_mb = parse_u64(fields[4]);
            snapshot.memory_used_mb = parse_u64(fields[5]);
            snapshot.memory_free_mb = parse_u64(fields[6]);
            snapshot.utilization_percent = parse_f64(fields[7]);
            snapshot.power_watts = parse_f64(fields[8]);
            snapshot.temperature_c = parse_f64(fields[9]);
            snapshot.graphics_clock_mhz = parse_u64(fields[10]);
            snapshot.memory_clock_mhz = parse_u64(fields[11]);
            snapshot.process_memory_used_mb = process_rows.and_then(|rows| {
                let uuid = snapshot.device_uuid.as_deref()?;
                let pid = std::process::id();
                let matching: Vec<_> = rows
                    .iter()
                    .filter(|(row_uuid, row_pid, _)| row_uuid == uuid && *row_pid == pid)
                    .collect();
                if matching.is_empty() {
                    return Some(0);
                }
                matching
                    .into_iter()
                    .try_fold(0_u64, |total, (_, _, used)| Some(total + *used.as_ref()?))
            });
            populate_missing_fields(&mut snapshot);
            finish_availability(&mut snapshot);
            Some(snapshot)
        })
        .collect()
}

fn parse_amd_json(raw: &[u8]) -> Result<Vec<LocalDeviceTelemetrySnapshot>, String> {
    let root: Value = serde_json::from_slice(raw)
        .map_err(|error| format!("amd-smi returned invalid JSON: {error}"))?;
    let global_driver = find_value(&root, &["driverversion", "amdgpuversion"]).and_then(value_text);
    let global_runtime = find_value(&root, &["rocmversion"]).and_then(value_text);
    let candidates = amd_device_candidates(&root);
    let mut snapshots = Vec::new();
    for (fallback_index, value) in candidates.into_iter().enumerate() {
        let mut snapshot = empty_snapshot(LocalDeviceTelemetrySource::AmdSmi, GpuBackend::Rocm);
        snapshot.device_index = find_value(value, &["gpu", "gpuid", "gpuindex", "id"])
            .and_then(value_u64)
            .and_then(|value| u32::try_from(value).ok())
            .or_else(|| u32::try_from(fallback_index).ok());
        snapshot.device_uuid = find_value(value, &["uuid"]).and_then(value_text);
        snapshot.device_name = find_value(
            value,
            &["marketname", "asicmarketname", "devicename", "name"],
        )
        .and_then(value_text);
        snapshot.driver_version = find_value(value, &["driverversion", "amdgpuversion"])
            .and_then(value_text)
            .or_else(|| global_driver.clone());
        snapshot.runtime_version = find_value(value, &["rocmversion"])
            .and_then(value_text)
            .or_else(|| global_runtime.clone());
        snapshot.memory_total_mb = find_value(
            value,
            &["vramtotal", "totalvram", "vramtotalmemory", "totalmemory"],
        )
        .and_then(value_mb);
        snapshot.memory_used_mb = find_value(
            value,
            &["vramused", "usedvram", "vrammemoryused", "usedmemory"],
        )
        .and_then(value_mb);
        snapshot.memory_free_mb = match (snapshot.memory_total_mb, snapshot.memory_used_mb) {
            (Some(total), Some(used)) => Some(total.saturating_sub(used)),
            _ => None,
        };
        snapshot.utilization_percent = find_value(
            value,
            &["gfxactivity", "gfxutilization", "gpuuse", "gpuutilization"],
        )
        .and_then(value_f64);
        snapshot.power_watts =
            find_value(value, &["power", "powerusage", "averagepower"]).and_then(value_f64);
        snapshot.temperature_c = find_value(
            value,
            &["temperature", "hotspottemperature", "junctiontemperature"],
        )
        .and_then(value_f64);
        snapshot.graphics_clock_mhz =
            find_value(value, &["gfxclock", "sclk", "graphicsclock"]).and_then(value_u64);
        snapshot.memory_clock_mhz = find_value(value, &["memoryclock", "mclk"]).and_then(value_u64);
        snapshot.process_memory_used_mb = amd_process_memory_mb(value, std::process::id());

        if snapshot.device_name.is_none()
            && snapshot.memory_used_mb.is_none()
            && snapshot.utilization_percent.is_none()
        {
            continue;
        }
        populate_missing_fields(&mut snapshot);
        finish_availability(&mut snapshot);
        snapshots.push(snapshot);
    }
    if snapshots.is_empty() {
        return Err("amd-smi JSON contained no recognizable GPU telemetry".to_string());
    }
    Ok(snapshots)
}

fn amd_device_candidates(root: &Value) -> Vec<&Value> {
    match root {
        Value::Array(values) => values.iter().collect(),
        Value::Object(values) => {
            let nested: Vec<_> = values
                .iter()
                .filter(|(key, value)| normalize_key(key).starts_with("gpu") && value.is_object())
                .map(|(_, value)| value)
                .collect();
            if nested.is_empty() {
                vec![root]
            } else {
                nested
            }
        }
        _ => vec![root],
    }
}

fn find_value<'a>(value: &'a Value, aliases: &[&str]) -> Option<&'a Value> {
    match value {
        Value::Object(values) => {
            for (key, value) in values {
                if key_matches(key, aliases) {
                    return Some(value);
                }
            }
            values.values().find_map(|value| find_value(value, aliases))
        }
        Value::Array(values) => values.iter().find_map(|value| find_value(value, aliases)),
        _ => None,
    }
}

fn amd_process_memory_mb(value: &Value, pid: u32) -> Option<u64> {
    match value {
        Value::Object(values) => {
            let is_target = values.iter().any(|(key, value)| {
                key_matches(key, &["pid", "processid"])
                    && value_u64(value).is_some_and(|value| value == u64::from(pid))
            });
            if is_target {
                let vram = find_value(value, &["vrammem", "vrammemory"]).and_then(value_mb);
                let gtt = find_value(value, &["gttmem", "gttmemory"]).and_then(value_mb);
                if vram.is_some() || gtt.is_some() {
                    return Some(vram.unwrap_or_default() + gtt.unwrap_or_default());
                }
                return find_value(value, &["mem", "memoryusage"]).and_then(value_mb);
            }
            values
                .values()
                .find_map(|value| amd_process_memory_mb(value, pid))
        }
        Value::Array(values) => values
            .iter()
            .find_map(|value| amd_process_memory_mb(value, pid)),
        _ => None,
    }
}

fn key_matches(key: &str, aliases: &[&str]) -> bool {
    let key = normalize_key(key);
    aliases.iter().any(|alias| {
        key == *alias || (alias.len() >= 4 && (key.starts_with(alias) || key.ends_with(alias)))
    })
}

fn empty_snapshot(
    source: LocalDeviceTelemetrySource,
    backend: GpuBackend,
) -> LocalDeviceTelemetrySnapshot {
    LocalDeviceTelemetrySnapshot {
        captured_at: Utc::now(),
        source,
        availability: LocalDeviceTelemetryAvailability::Unavailable,
        backend,
        device_index: None,
        device_uuid: None,
        device_name: None,
        driver_version: None,
        runtime_version: None,
        unified_memory: None,
        memory_total_mb: None,
        memory_budget_mb: None,
        memory_used_mb: None,
        memory_free_mb: None,
        process_memory_used_mb: None,
        recommended_working_set_mb: None,
        utilization_percent: None,
        power_watts: None,
        temperature_c: None,
        graphics_clock_mhz: None,
        memory_clock_mhz: None,
        throttle_reasons: None,
        unavailable_fields: Vec::new(),
        collector_error: None,
    }
}

fn unavailable_snapshot(
    source: LocalDeviceTelemetrySource,
    backend: GpuBackend,
    error: &str,
) -> LocalDeviceTelemetrySnapshot {
    let mut snapshot = empty_snapshot(source, backend);
    snapshot.unavailable_fields = DEVICE_FIELDS.into_iter().map(str::to_string).collect();
    snapshot.collector_error = Some(truncate_error(error));
    snapshot
}

fn populate_missing_fields(snapshot: &mut LocalDeviceTelemetrySnapshot) {
    let fields = [
        ("memoryTotalMb", snapshot.memory_total_mb.is_some()),
        ("memoryBudgetMb", snapshot.memory_budget_mb.is_some()),
        ("memoryUsedMb", snapshot.memory_used_mb.is_some()),
        ("memoryFreeMb", snapshot.memory_free_mb.is_some()),
        (
            "processMemoryUsedMb",
            snapshot.process_memory_used_mb.is_some(),
        ),
        (
            "recommendedWorkingSetMb",
            snapshot.recommended_working_set_mb.is_some(),
        ),
        ("utilizationPercent", snapshot.utilization_percent.is_some()),
        ("powerWatts", snapshot.power_watts.is_some()),
        ("temperatureC", snapshot.temperature_c.is_some()),
        ("graphicsClockMhz", snapshot.graphics_clock_mhz.is_some()),
        ("memoryClockMhz", snapshot.memory_clock_mhz.is_some()),
        ("driverVersion", snapshot.driver_version.is_some()),
        ("runtimeVersion", snapshot.runtime_version.is_some()),
        ("unifiedMemory", snapshot.unified_memory.is_some()),
        ("throttleReasons", snapshot.throttle_reasons.is_some()),
    ];
    snapshot.unavailable_fields = fields
        .into_iter()
        .filter(|(_, available)| !available)
        .map(|(name, _)| name.to_string())
        .collect();
}

fn finish_availability(snapshot: &mut LocalDeviceTelemetrySnapshot) {
    snapshot.availability = if snapshot.collector_error.is_some() {
        LocalDeviceTelemetryAvailability::Unavailable
    } else if snapshot.unavailable_fields.is_empty() {
        LocalDeviceTelemetryAvailability::Available
    } else {
        LocalDeviceTelemetryAvailability::Partial
    };
}

fn run_optional(command: &str, args: &[&str]) -> Option<Result<Output, String>> {
    let mut child = match Command::new(command)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => return Some(Err(format!("failed to execute {command}: {error}"))),
    };
    let stdout = child.stdout.take().expect("stdout configured as piped");
    let stderr = child.stderr.take().expect("stderr configured as piped");
    let stdout_reader = thread::spawn(move || read_pipe(stdout));
    let stderr_reader = thread::spawn(move || read_pipe(stderr));
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if started.elapsed() < Duration::from_secs(2) => {
                thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(format!("{command} telemetry timed out after 2 seconds"));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(format!("failed while waiting for {command}: {error}"));
            }
        }
    };
    let stdout = join_pipe(command, "stdout", stdout_reader);
    let stderr = join_pipe(command, "stderr", stderr_reader);
    Some(match (status, stdout, stderr) {
        (Ok(status), Ok(stdout), Ok(stderr)) => Ok(Output {
            status,
            stdout,
            stderr,
        }),
        (Err(error), _, _) | (_, Err(error), _) | (_, _, Err(error)) => Err(error),
    })
}

fn read_pipe(mut pipe: impl Read) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    pipe.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn join_pipe(
    command: &str,
    stream: &str,
    reader: thread::JoinHandle<std::io::Result<Vec<u8>>>,
) -> Result<Vec<u8>, String> {
    reader
        .join()
        .map_err(|_| format!("{command} {stream} reader panicked"))?
        .map_err(|error| format!("failed to read {command} {stream}: {error}"))
}

fn command_error(command: &str, output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    format!("{command} exited with {}: {}", output.status, stderr.trim())
}

fn truncate_error(error: &str) -> String {
    error.chars().take(512).collect()
}

fn parse_text(value: &str) -> Option<String> {
    (!is_unavailable(value)).then(|| value.trim().to_string())
}

fn parse_u64(value: &str) -> Option<u64> {
    if is_unavailable(value) {
        return None;
    }
    value.trim().parse::<f64>().ok().map(|value| value as u64)
}

fn parse_f64(value: &str) -> Option<f64> {
    if is_unavailable(value) {
        return None;
    }
    value.trim().parse().ok()
}

fn is_unavailable(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "" | "n/a" | "na" | "not supported" | "[not supported]"
    )
}

fn normalize_key(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn value_text(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => parse_text(value),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn value_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(value) => value.as_f64(),
        Value::String(value) => value
            .split_whitespace()
            .next()
            .and_then(|value| value.trim_end_matches('%').parse().ok()),
        _ => None,
    }
}

fn value_u64(value: &Value) -> Option<u64> {
    value_f64(value).map(|value| value as u64)
}

fn value_mb(value: &Value) -> Option<u64> {
    match value {
        Value::Number(value) => value.as_u64(),
        Value::String(value) => {
            let amount = value
                .split_whitespace()
                .next()
                .and_then(|value| value.parse::<f64>().ok())?;
            let unit = value.to_ascii_lowercase();
            if unit.contains("gib") || unit.contains(" gb") {
                Some((amount * 1024.0) as u64)
            } else if unit.contains("kib") || unit.contains(" kb") {
                Some((amount / 1024.0) as u64)
            } else if unit.contains("byte") || unit.ends_with(" b") {
                Some((amount / 1024.0 / 1024.0) as u64)
            } else {
                Some(amount as u64)
            }
        }
        _ => None,
    }
}

#[cfg(all(target_os = "macos", feature = "telemetry-metal"))]
fn bytes_to_mb(bytes: u64) -> u64 {
    bytes / 1024 / 1024
}

#[cfg(test)]
mod tests {
    use super::{amd_process_memory_mb, parse_amd_json, parse_nvidia_csv};
    use medousa_types::{LocalDeviceTelemetryAvailability, LocalDeviceTelemetrySource};

    #[test]
    fn parses_nvidia_csv_and_marks_unsupported_fields_missing() {
        let rows = vec![("GPU-abc".to_string(), std::process::id(), Some(512))];
        let snapshots = parse_nvidia_csv(
            "0, GPU-abc, RTX Test, 600.1, 24576, 4096, 20480, 91, 150.5, 66, 2505, N/A\n",
            Some(&rows),
        );
        let snapshot = &snapshots[0];
        assert_eq!(snapshot.source, LocalDeviceTelemetrySource::NvidiaSmi);
        assert_eq!(snapshot.process_memory_used_mb, Some(512));
        assert_eq!(snapshot.memory_clock_mhz, None);
        assert!(
            snapshot
                .unavailable_fields
                .contains(&"memoryClockMhz".to_string())
        );
        assert_eq!(
            snapshot.availability,
            LocalDeviceTelemetryAvailability::Partial
        );
    }

    #[test]
    fn parses_nested_amd_smi_json_without_inventing_process_memory() {
        let raw = br#"{
          "gpu_0": {
            "GPU": 0,
            "ASIC": {"MARKET_NAME": "Radeon Test"},
            "DRIVER_VERSION": "6.14",
            "ROCM_VERSION": "7.0",
            "VRAM_USAGE": {"VRAM_TOTAL": "16 GB", "VRAM_USED": "4 GB"},
            "GFX_ACTIVITY": "87 %",
            "POWER_USAGE": "120 W",
            "HOTSPOT_TEMPERATURE": "71 C",
            "GFX_CLOCK": "2400 MHz",
            "MEMORY_CLOCK": "1100 MHz"
          }
        }"#;
        let snapshots = parse_amd_json(raw).unwrap();
        let snapshot = &snapshots[0];
        assert_eq!(snapshot.source, LocalDeviceTelemetrySource::AmdSmi);
        assert_eq!(snapshot.device_name.as_deref(), Some("Radeon Test"));
        assert_eq!(snapshot.memory_total_mb, Some(16 * 1024));
        assert_eq!(snapshot.memory_used_mb, Some(4 * 1024));
        assert_eq!(snapshot.process_memory_used_mb, None);
        assert!(
            snapshot
                .unavailable_fields
                .contains(&"processMemoryUsedMb".to_string())
        );
    }

    #[test]
    fn parses_current_process_vram_and_gtt_from_amd_smi() {
        let pid = std::process::id();
        let value = serde_json::json!({
            "PROCESSES": [{
                "PID": pid,
                "MEMORY_USAGE": {"VRAM_MEM": "2 GB", "GTT_MEM": "512 MB"}
            }]
        });
        assert_eq!(amd_process_memory_mb(&value, pid), Some(2 * 1024 + 512));
    }

    #[cfg(all(target_os = "macos", feature = "telemetry-metal"))]
    #[test]
    fn metal_reports_process_allocation_and_working_set_without_loading_a_model() {
        let snapshot = super::collect_metal();
        assert_eq!(snapshot.source, LocalDeviceTelemetrySource::MetalApi);
        assert!(snapshot.process_memory_used_mb.is_some());
        assert!(snapshot.recommended_working_set_mb.is_some());
        assert!(snapshot.unified_memory.is_some());
    }
}
