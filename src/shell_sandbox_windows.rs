//! Windows Job Object hardening for Medousa shell runs.
//!
//! Provides process-tree containment (kill-on-close, memory/active-process caps,
//! UI lockdown, CREATE_NO_WINDOW). Full AppContainer FS ACLs remain a follow-up;
//! network deny uses job net-rate control when available.

#![cfg(windows)]

use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

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

pub fn probe_backend() -> (&'static str, bool, String) {
    (
        "job_object",
        true,
        "Windows Job Object (kill-on-close, memory/UI limits, no-window). \
         FS ACL jail (AppContainer) not yet applied."
            .to_string(),
    )
}

pub fn run_with_job(request: &ShellRunRequest, cwd: &Path) -> Result<ShellRunResult, String> {
    let job = WindowsJob::create(!request.profile.network)?;
    let warning = Some(
        "Windows Job Object active; filesystem ACL isolation (AppContainer) still pending"
            .to_string(),
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
        .env("SystemRoot", std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()))
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
            // Best-effort: clamp outbound bandwidth to 1 byte/s (effectively blocked).
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

        let ok = unsafe {
            SetInformationJobObject(
                self.handle,
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&info).cast(),
                std::mem::size_of_val(&info) as u32,
            )
        };
        ok.map_err(|err| format!("SetInformationJobObject(extended) failed: {err}"))
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
        let ok = unsafe {
            SetInformationJobObject(
                self.handle,
                JobObjectBasicUIRestrictions,
                std::ptr::from_ref(&info).cast(),
                std::mem::size_of_val(&info) as u32,
            )
        };
        ok.map_err(|err| format!("SetInformationJobObject(ui) failed: {err}"))
    }

    fn apply_network_clamp(&self) -> Result<(), String> {
        let info = JOBOBJECT_NET_RATE_CONTROL_INFORMATION {
            MaxBandwidth: 1,
            ControlFlags: JOB_OBJECT_NET_RATE_CONTROL_ENABLE
                | JOB_OBJECT_NET_RATE_CONTROL_MAX_BANDWIDTH,
            DscpTag: 0,
        };
        let ok = unsafe {
            SetInformationJobObject(
                self.handle,
                JobObjectNetRateControlInformation,
                std::ptr::from_ref(&info).cast(),
                std::mem::size_of_val(&info) as u32,
            )
        };
        ok.map_err(|err| format!("SetInformationJobObject(net) failed: {err}"))
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
                // KILL_ON_JOB_CLOSE: closing the last handle kills remaining children.
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

    // Keep job alive until process tree is done, then drop (kill leftovers).
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
