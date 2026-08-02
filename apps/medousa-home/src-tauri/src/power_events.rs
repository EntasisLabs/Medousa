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
pub fn install(_app: &tauri::AppHandle) {}
