#[cfg(all(target_os = "ios", live_activity_native))]
mod ios_push_setup {
    use std::os::raw::c_void;

    extern "C" {
        fn medousa_ios_push_setup();
    }

    pub fn install() {
        unsafe {
            medousa_ios_push_setup();
        }
        let _ = std::ptr::null::<c_void>();
    }
}

#[cfg(not(all(target_os = "ios", live_activity_native)))]
mod ios_push_setup {
    pub fn install() {}
}

pub fn install_ios_push_background_handler() {
    ios_push_setup::install();
}
