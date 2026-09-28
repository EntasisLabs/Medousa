//! Replace the wasm32 `std::time` panic stubs.
//!
//! `Instant` on this target is a `Duration` written through an sret pointer:
//! `secs: u64` at offset 0 and `nanos: u32` at offset 8. `now` is `(out: i32)`,
//! and `elapsed` is `(out: i32, self: i32)`. `wasm-ld --wrap` sends every call
//! at the libstd panic stubs to these writers.

use wasm_bindgen::prelude::*;

use crate::clock_parts::{duration_parts_from_ms, saturating_between};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = performance, js_name = now)]
    fn performance_now() -> f64;
}

fn write_parts(out: *mut u8, parts: (u64, u32)) {
    // SAFETY: callers pass the sret pointer rustc uses for `Instant` / `SystemTime`,
    // which is 16 bytes (`u64` secs, `u32` nanos).
    unsafe {
        std::ptr::write_unaligned(out as *mut u64, parts.0);
        std::ptr::write_unaligned(out.add(8) as *mut u32, parts.1);
    }
}

fn read_parts(ptr: *const u8) -> (u64, u32) {
    // SAFETY: `this` is the same `Instant` layout written by `write_parts`.
    unsafe {
        (
            std::ptr::read_unaligned(ptr as *const u64),
            std::ptr::read_unaligned(ptr.add(8) as *const u32),
        )
    }
}

pub fn write_instant_now(out: *mut u8) {
    write_parts(out, duration_parts_from_ms(performance_now()));
}

pub fn write_system_time_now(out: *mut u8) {
    write_parts(out, duration_parts_from_ms(js_sys::Date::now()));
}

pub fn write_instant_elapsed(out: *mut u8, this: *const u8) {
    let mut now = [0u8; 16];
    write_instant_now(now.as_mut_ptr());
    let elapsed = saturating_between(read_parts(now.as_ptr()), read_parts(this));
    write_parts(out, elapsed);
}
