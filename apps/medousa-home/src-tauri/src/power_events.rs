#[cfg(target_os = "macos")]
pub fn install(app: &tauri::AppHandle) {
    use std::sync::atomic::{AtomicBool, Ordering};

    static INSTALLED: AtomicBool = AtomicBool::new(false);
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }

    let _ = app.run_on_main_thread(|| {
        use block2::RcBlock;
        use objc2_app_kit::{NSWorkspace, NSWorkspaceWillSleepNotification};

        let center = NSWorkspace::sharedWorkspace().notificationCenter();
        let block = RcBlock::new(|_| {
            crate::workshop_runtime::request_local_brain_stop(
                crate::workshop_registry::PERSONAL_WORKSHOP_ID,
            );
        });
        let observer = unsafe {
            center.addObserverForName_object_queue_usingBlock(
                Some(NSWorkspaceWillSleepNotification),
                None,
                None,
                &block,
            )
        };
        // The workspace notification center owns the callback. Keep the token
        // for the app lifetime; dropping it would make lifecycle ownership
        // ambiguous across AppKit versions.
        std::mem::forget(observer);
    });
}

#[cfg(not(target_os = "macos"))]
#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub fn install(_app: &tauri::AppHandle) {}

#[cfg(target_os = "windows")]
pub fn install(_app: &tauri::AppHandle) {
    use std::ffi::c_void;

    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::Power::{
        DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS, PowerRegisterSuspendResumeNotification,
    };
    use windows::Win32::UI::WindowsAndMessaging::{DEVICE_NOTIFY_CALLBACK, PBT_APMSUSPEND};

    unsafe extern "system" fn callback(
        _context: *const c_void,
        event: u32,
        _setting: *const c_void,
    ) -> u32 {
        if event == PBT_APMSUSPEND {
            crate::workshop_runtime::request_local_brain_stop(
                crate::workshop_registry::PERSONAL_WORKSHOP_ID,
            );
        }
        0
    }

    let parameters = Box::new(DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS {
        Callback: Some(callback),
        Context: std::ptr::null_mut(),
    });
    let parameters = Box::into_raw(parameters);
    let mut registration = std::ptr::null_mut();
    let result = unsafe {
        PowerRegisterSuspendResumeNotification(
            DEVICE_NOTIFY_CALLBACK,
            HANDLE(parameters.cast()),
            &mut registration,
        )
    };
    if result.0 != 0 {
        eprintln!("[medousa-home] registering Windows power notifications failed: {result:?}");
        unsafe {
            drop(Box::from_raw(parameters));
        }
    }
    // On success Windows owns the callback subscription until process exit;
    // the parameter block must remain valid for that same lifetime.
}

#[cfg(target_os = "linux")]
pub fn install(_app: &tauri::AppHandle) {
    tauri::async_runtime::spawn(async {
        if let Err(error) = monitor_logind_sleep().await {
            eprintln!("[medousa-home] monitoring Linux sleep events failed: {error}");
        }
    });
}

#[cfg(target_os = "linux")]
async fn monitor_logind_sleep() -> zbus::Result<()> {
    use futures_util::StreamExt;

    let connection = zbus::Connection::system().await?;
    let proxy = zbus::Proxy::new(
        &connection,
        "org.freedesktop.login1",
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
    )
    .await?;
    let mut signals = proxy.receive_signal("PrepareForSleep").await?;
    while let Some(message) = signals.next().await {
        let (sleeping,): (bool,) = message.body()?;
        if sleeping {
            crate::workshop_runtime::request_local_brain_stop(
                crate::workshop_registry::PERSONAL_WORKSHOP_ID,
            );
        }
    }
    Ok(())
}
