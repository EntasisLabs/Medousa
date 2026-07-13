//! Windows sandbox backends for Medousa shell runs.
//!
//! Preferred: AppContainer (rappct) with package-SID ACL grants on workshop paths
//! + Job limits. Fallback: Job Object only (process-tree containment, no FS ACL jail).

#![cfg(windows)]

use std::ffi::OsString;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use rappct::acl::{AccessMask, ResourcePath, grant_to_package};
use rappct::{
    AppContainerProfile, JobLimits, KnownCapability, LaunchOptions, SecurityCapabilitiesBuilder,
    StdioConfig, launch_in_container_with_io,
};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOBOBJECT_BASIC_UI_RESTRICTIONS,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOBOBJECT_NET_RATE_CONTROL_INFORMATION,
    JOB_OBJECT_LIMIT_ACTIVE_PROCESS, JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_LIMIT_PROCESS_MEMORY,
    JOB_OBJECT_NET_RATE_CONTROL_ENABLE, JOB_OBJECT_NET_RATE_CONTROL_MAX_BANDWIDTH,
    JOB_OBJECT_UILIMIT_DESKTOP, JOB_OBJECT_UILIMIT_DISPLAYSETTINGS,
    JOB_OBJECT_UILIMIT_EXITWINDOWS, JOB_OBJECT_UILIMIT_HANDLES, JOB_OBJECT_UILIMIT_READCLIPBOARD,
    JOB_OBJECT_UILIMIT_SYSTEMPARAMETERS, JOB_OBJECT_UILIMIT_WRITECLIPBOARD,
    JobObjectBasicUIRestrictions, JobObjectExtendedLimitInformation,
    JobObjectNetRateControlInformation, SetInformationJobObject, TerminateJobObject,
};
use windows::core::PCWSTR;

use super::{
    ShellPermissionProfile, ShellRunRequest, ShellRunResult, default_path, read_limited,
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const DEFAULT_PROCESS_MEMORY_BYTES: usize = 512 * 1024 * 1024;
const DEFAULT_ACTIVE_PROCESS_LIMIT: u32 = 32;
const PROFILE_NAME: &str = "medousa.shell";
const PROFILE_DISPLAY: &str = "Medousa Shell";

pub fn probe_backend() -> (&'static str, bool, String) {
    match AppContainerProfile::ensure(PROFILE_NAME, PROFILE_DISPLAY, Some("Medousa agent shell")) {
        Ok(_) => (
            "appcontainer",
            true,
            "Windows AppContainer + Job limits (package-SID ACL grants on workshop paths)"
                .to_string(),
        ),
        Err(err) => (
            "job_object",
            true,
            format!(
                "AppContainer unavailable ({err}); Job Object fallback (no FS ACL jail)"
            ),
        ),
    }
}

/// Prefer AppContainer; fall back to Job Object containment.
pub fn run_with_job(request: &ShellRunRequest, cwd: &Path) -> Result<ShellRunResult, String> {
    match run_with_appcontainer(request, cwd) {
        Ok(result) => Ok(result),
        Err(ac_err) => {
            tracing::warn!(error = %ac_err, "shell AppContainer launch failed; falling back to Job Object");
            let mut result = run_with_job_object(request, cwd)?;
            let fallback = format!("AppContainer unavailable ({ac_err}); used Job Object");
            result.warning = Some(match result.warning.take() {
                Some(existing) => format!("{fallback}; {existing}"),
                None => fallback,
            });
            Ok(result)
        }
    }
}

fn run_with_appcontainer(
    request: &ShellRunRequest,
    cwd: &Path,
) -> Result<ShellRunResult, String> {
    let profile = AppContainerProfile::ensure(
        PROFILE_NAME,
        PROFILE_DISPLAY,
        Some("Medousa agent shell sandbox"),
    )
    .map_err(|err| format!("AppContainerProfile::ensure failed: {err}"))?;

    grant_profile_paths(&profile, request, cwd)?;

    let mut cap_builder = SecurityCapabilitiesBuilder::new(&profile.sid);
    if request.profile.network {
        cap_builder = cap_builder.with_known(&[
            KnownCapability::InternetClient,
            KnownCapability::PrivateNetworkClientServer,
        ]);
    }
    let caps = cap_builder
        .build()
        .map_err(|err| format!("SecurityCapabilities build failed: {err}"))?;

    let exe = PathBuf::from(&request.argv[0]);
    let cmdline = Some(format!(" {}", windows_cmdline(&request.argv)));

    let profile_dir = profile
        .folder_path()
        .unwrap_or_else(|_| std::env::temp_dir().join("medousa-shell-ac"));

    let env = Some(vec![
        (OsString::from("PATH"), OsString::from(default_path())),
        (
            OsString::from("SystemRoot"),
            OsString::from(
                std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string()),
            ),
        ),
        (OsString::from("TMP"), profile_dir.as_os_str().to_owned()),
        (OsString::from("TEMP"), profile_dir.as_os_str().to_owned()),
        (
            OsString::from("LOCALAPPDATA"),
            profile_dir.as_os_str().to_owned(),
        ),
        (
            OsString::from("USERPROFILE"),
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from(r"C:\"))
                .into_os_string(),
        ),
    ]);

    let opts = LaunchOptions {
        exe,
        cmdline,
        cwd: Some(cwd.to_path_buf()),
        env,
        stdio: StdioConfig::Pipe,
        suspended: false,
        join_job: Some(JobLimits {
            memory_bytes: Some(DEFAULT_PROCESS_MEMORY_BYTES),
            cpu_rate_percent: None,
            kill_on_job_close: true,
        }),
        startup_timeout: Some(Duration::from_secs(15)),
    };

    let started = Instant::now();
    let mut launched = launch_in_container_with_io(&caps, &opts)
        .map_err(|err| format!("launch_in_container_with_io failed: {err}"))?;

    let max_output = request.profile.max_output_bytes;
    let stdout_thread = launched.stdout.take().map(|pipe| {
        std::thread::spawn(move || read_limited(pipe, max_output))
    });
    let stderr_thread = launched.stderr.take().map(|pipe| {
        std::thread::spawn(move || read_limited(pipe, max_output))
    });
    // Drop stdin so the child isn't blocked waiting for input.
    drop(launched.stdin.take());

    let timeout = Duration::from_millis(request.profile.timeout_ms);
    let wait_result = launched.wait(Some(timeout));
    let (exit_code, timed_out) = match wait_result {
        Ok(code) => (code as i32, false),
        Err(_) => (-1, true),
    };

    let stdout = stdout_thread
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    let stderr = if timed_out {
        let mut msg = format!(
            "shell.run timed out after {}ms",
            request.profile.timeout_ms
        );
        let captured = stderr_thread
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default();
        if !captured.is_empty() {
            msg.push('\n');
            msg.push_str(&captured);
        }
        msg
    } else {
        stderr_thread
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default()
    };

    Ok(ShellRunResult {
        exit_code,
        stdout,
        stderr,
        backend: "appcontainer".to_string(),
        sandboxed: true,
        timed_out,
        duration_ms: started.elapsed().as_millis() as u64,
        warning: None,
    })
}

fn grant_profile_paths(
    profile: &AppContainerProfile,
    request: &ShellRunRequest,
    cwd: &Path,
) -> Result<(), String> {
    let rw = AccessMask(AccessMask::FILE_GENERIC_READ.0 | AccessMask::FILE_GENERIC_WRITE.0);
    let ro = AccessMask::FILE_GENERIC_READ;

    grant_dir(profile, cwd, rw)?;
    for root in &request.profile.writable_roots {
        grant_dir(profile, root, rw)?;
    }
    for root in &request.profile.readonly_roots {
        grant_dir(profile, root, ro)?;
    }

    // Allow reading/executing the target binary when it lives outside System32.
    let exe = Path::new(&request.argv[0]);
    if exe.is_file() {
        let _ = grant_to_package(
            ResourcePath::File(exe.to_path_buf()),
            &profile.sid,
            AccessMask::GENERIC_ALL,
        );
    }

    if let Ok(profile_dir) = profile.folder_path() {
        grant_dir(profile, &profile_dir, rw)?;
    }

    Ok(())
}

fn grant_dir(
    profile: &AppContainerProfile,
    path: &Path,
    access: AccessMask,
) -> Result<(), String> {
    let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if !canon.exists() {
        return Ok(());
    }
    let target = if canon.is_dir() {
        ResourcePath::Directory(canon)
    } else {
        ResourcePath::File(canon)
    };
    grant_to_package(target, &profile.sid, access)
        .map_err(|err| format!("grant_to_package failed for {}: {err}", path.display()))
}

fn windows_cmdline(argv: &[String]) -> String {
    argv.iter()
        .map(|arg| {
            if arg.is_empty() {
                "\"\"".to_string()
            } else if arg.chars().any(|c| c.is_whitespace() || c == '"') {
                format!("\"{}\"", arg.replace('"', "\\\""))
            } else {
                arg.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn run_with_job_object(
    request: &ShellRunRequest,
    cwd: &Path,
) -> Result<ShellRunResult, String> {
    let job = WindowsJob::create(!request.profile.network)?;
    let warning = Some(
        "Windows Job Object active (no AppContainer FS ACL jail)".to_string(),
    );

    let mut cmd = Command::new(&request.argv[0]);
    if request.argv.len() > 1 {
        cmd.args(&request.argv[1..]);
    }
    cmd.current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear()
        .env("PATH", default_path())
        .env(
            "USERPROFILE",
            dirs::home_dir().unwrap_or_else(|| PathBuf::from(r"C:\")),
        )
        .env("TMP", std::env::temp_dir())
        .env("TEMP", std::env::temp_dir())
        .env(
            "SystemRoot",
            std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()),
        )
        .creation_flags(CREATE_NO_WINDOW);

    wait_with_job(cmd, &request.profile, job, warning)
}

struct WindowsJob {
    handle: HANDLE,
}

impl WindowsJob {
    fn create(deny_network: bool) -> Result<Self, String> {
        let handle = unsafe { CreateJobObjectW(None, PCWSTR::null()) }
            .map_err(|err| format!("CreateJobObjectW failed: {err}"))?;

        let job = Self { handle };
        job.apply_extended_limits()?;
        job.apply_ui_restrictions()?;
        if deny_network {
            if let Err(err) = job.apply_network_clamp() {
                tracing::warn!(error = %err, "shell job net-rate clamp unavailable");
            }
        }
        Ok(job)
    }

    fn apply_extended_limits(&self) -> Result<(), String> {
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION
            | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
            | JOB_OBJECT_LIMIT_PROCESS_MEMORY;
        info.BasicLimitInformation.ActiveProcessLimit = DEFAULT_ACTIVE_PROCESS_LIMIT;
        info.ProcessMemoryLimit = DEFAULT_PROCESS_MEMORY_BYTES;

        unsafe {
            SetInformationJobObject(
                self.handle,
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&info).cast(),
                std::mem::size_of_val(&info) as u32,
            )
        }
        .map_err(|err| format!("SetInformationJobObject(extended) failed: {err}"))
    }

    fn apply_ui_restrictions(&self) -> Result<(), String> {
        let info = JOBOBJECT_BASIC_UI_RESTRICTIONS {
            UIRestrictionsClass: JOB_OBJECT_UILIMIT_DESKTOP
                | JOB_OBJECT_UILIMIT_DISPLAYSETTINGS
                | JOB_OBJECT_UILIMIT_EXITWINDOWS
                | JOB_OBJECT_UILIMIT_READCLIPBOARD
                | JOB_OBJECT_UILIMIT_WRITECLIPBOARD
                | JOB_OBJECT_UILIMIT_SYSTEMPARAMETERS
                | JOB_OBJECT_UILIMIT_HANDLES,
        };
        unsafe {
            SetInformationJobObject(
                self.handle,
                JobObjectBasicUIRestrictions,
                std::ptr::from_ref(&info).cast(),
                std::mem::size_of_val(&info) as u32,
            )
        }
        .map_err(|err| format!("SetInformationJobObject(ui) failed: {err}"))
    }

    fn apply_network_clamp(&self) -> Result<(), String> {
        let info = JOBOBJECT_NET_RATE_CONTROL_INFORMATION {
            MaxBandwidth: 1,
            ControlFlags: JOB_OBJECT_NET_RATE_CONTROL_ENABLE
                | JOB_OBJECT_NET_RATE_CONTROL_MAX_BANDWIDTH,
            DscpTag: 0,
        };
        unsafe {
            SetInformationJobObject(
                self.handle,
                JobObjectNetRateControlInformation,
                std::ptr::from_ref(&info).cast(),
                std::mem::size_of_val(&info) as u32,
            )
        }
        .map_err(|err| format!("SetInformationJobObject(net) failed: {err}"))
    }

    fn assign_process(&self, process: HANDLE) -> Result<(), String> {
        unsafe { AssignProcessToJobObject(self.handle, process) }
            .map_err(|err| format!("AssignProcessToJobObject failed: {err}"))
    }

    fn terminate(&self) {
        let _ = unsafe { TerminateJobObject(self.handle, 1) };
    }
}

impl Drop for WindowsJob {
    fn drop(&mut self) {
        if !self.handle.is_invalid() {
            unsafe {
                let _ = CloseHandle(self.handle);
            }
            self.handle = HANDLE::default();
        }
    }
}

fn wait_with_job(
    mut cmd: Command,
    profile: &ShellPermissionProfile,
    job: WindowsJob,
    warning: Option<String>,
) -> Result<ShellRunResult, String> {
    let started = Instant::now();
    let max_output = profile.max_output_bytes;
    let mut child = cmd
        .spawn()
        .map_err(|err| format!("failed to spawn Windows shell job: {err}"))?;

    let process = HANDLE(child.as_raw_handle());
    if let Err(err) = job.assign_process(process) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(err);
    }

    let stdout_thread = child
        .stdout
        .take()
        .map(|pipe| std::thread::spawn(move || read_limited(pipe, max_output)));
    let stderr_thread = child
        .stderr
        .take()
        .map(|pipe| std::thread::spawn(move || read_limited(pipe, max_output)));

    let timeout = Duration::from_millis(profile.timeout_ms);
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if started.elapsed() >= timeout {
                    job.terminate();
                    let _ = child.kill();
                    timed_out = true;
                    break child.wait().unwrap_or_default();
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(err) => return Err(format!("shell.run wait failed: {err}")),
        }
    };

    drop(job);

    let stdout = stdout_thread
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    let stderr = if timed_out {
        let mut msg = format!("shell.run timed out after {}ms", profile.timeout_ms);
        let captured = stderr_thread
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default();
        if !captured.is_empty() {
            msg.push('\n');
            msg.push_str(&captured);
        }
        msg
    } else {
        stderr_thread
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default()
    };

    Ok(ShellRunResult {
        exit_code: if timed_out {
            -1
        } else {
            status.code().unwrap_or(-1)
        },
        stdout,
        stderr,
        backend: "job_object".to_string(),
        sandboxed: true,
        timed_out,
        duration_ms: started.elapsed().as_millis() as u64,
        warning,
    })
}