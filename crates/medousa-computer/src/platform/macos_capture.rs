use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use block2::RcBlock;
use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder};
use medousa_computer_bridge::{
    ComputerRect, MAX_COMPUTER_SCREENSHOT_BYTES, MAX_COMPUTER_SCREENSHOT_HEIGHT,
    MAX_COMPUTER_SCREENSHOT_PIXELS,
};
use objc2::AnyThread;
use objc2::rc::autoreleasepool;
use objc2_core_graphics::{CGDataProvider, CGImage};
use objc2_foundation::NSError;
use objc2_screen_capture_kit::{
    SCContentFilter, SCScreenshotManager, SCShareableContent, SCStreamConfiguration, SCWindow,
};

const CAPTURE_TIMEOUT: Duration = Duration::from_secs(8);
const FRAME_MATCH_TOLERANCE_POINTS: f64 = 2.0;
const REDACTION_PADDING_PIXELS: i64 = 3;
pub(super) struct FocusedWindowCaptureSpec {
    pub process_id: i32,
    pub title: String,
    pub frame: ComputerRect,
    pub max_width: u32,
}

pub(super) struct CapturedFrame {
    pub width: u32,
    pub height: u32,
    rgba: Vec<u8>,
}

pub(super) fn capture_focused_window(
    spec: &FocusedWindowCaptureSpec,
) -> Result<CapturedFrame, String> {
    autoreleasepool(|_| capture_focused_window_inner(spec))
}

fn capture_focused_window_inner(spec: &FocusedWindowCaptureSpec) -> Result<CapturedFrame, String> {
    if spec.process_id <= 0 || spec.frame.width == 0 || spec.frame.height == 0 {
        return Err("focused window has no capturable identity or frame".to_string());
    }

    let (sender, receiver) = mpsc::sync_channel(1);
    let sender = Arc::new(Mutex::new(Some(sender)));
    let callback_sender = Arc::clone(&sender);
    let process_id = spec.process_id;
    let title = spec.title.clone();
    let expected_frame = spec.frame;
    let max_width = spec.max_width;
    let content_callback = RcBlock::new(
        move |content: *mut SCShareableContent, error: *mut NSError| unsafe {
            if content.is_null() {
                finish(
                    &callback_sender,
                    Err(framework_error("enumerate capturable windows", error)),
                );
                return;
            }
            let windows = (*content).windows().to_vec();
            let mut candidates = windows
                .into_iter()
                .filter(|window| window_matches(window, process_id, expected_frame))
                .collect::<Vec<_>>();
            if candidates.len() > 1 && !title.is_empty() {
                candidates.retain(|window| {
                    window
                        .title()
                        .is_some_and(|candidate| candidate.to_string() == title)
                });
            }
            if candidates.len() != 1 {
                finish(
                    &callback_sender,
                    Err(
                        "focused window could not be matched unambiguously for capture".to_string(),
                    ),
                );
                return;
            }
            let window = candidates.pop().expect("one checked capture window");
            let filter = SCContentFilter::initWithDesktopIndependentWindow(
                SCContentFilter::alloc(),
                &window,
            );
            let native_scale = f64::from(filter.pointPixelScale()).clamp(1.0, 4.0);
            let window_frame = window.frame();
            let (width, height) = target_dimensions(
                window_frame.size.width * native_scale,
                window_frame.size.height * native_scale,
                max_width,
            );
            let configuration = SCStreamConfiguration::new();
            configuration.setWidth(width as usize);
            configuration.setHeight(height as usize);
            configuration.setScalesToFit(true);
            configuration.setPreservesAspectRatio(true);
            configuration.setShowsCursor(false);
            configuration.setIgnoreShadowsSingleWindow(true);
            configuration.setIncludeChildWindows(false);
            configuration.setShouldBeOpaque(true);

            let image_sender = Arc::clone(&callback_sender);
            let filter_lifetime = filter.clone();
            let configuration_lifetime = configuration.clone();
            let image_callback = RcBlock::new(move |image: *mut CGImage, error: *mut NSError| {
                let _keep_alive = (&filter_lifetime, &configuration_lifetime);
                let result = if image.is_null() {
                    Err(framework_error("capture focused window", error))
                } else {
                    // ScreenCaptureKit documents this API as returning BGRA.
                    copy_bgra_frame(&*image)
                };
                finish(&image_sender, result);
            });
            SCScreenshotManager::captureImageWithFilter_configuration_completionHandler(
                &filter,
                &configuration,
                Some(&image_callback),
            );
        },
    );

    unsafe {
        SCShareableContent::getShareableContentExcludingDesktopWindows_onScreenWindowsOnly_completionHandler(
            true,
            true,
            &content_callback,
        );
    }
    receiver
        .recv_timeout(CAPTURE_TIMEOUT)
        .map_err(|_| "focused-window screenshot timed out".to_string())?
}

pub(super) fn encode_redacted_png(
    mut frame: CapturedFrame,
    window_frame: ComputerRect,
    sensitive_regions: &[ComputerRect],
) -> Result<Vec<u8>, String> {
    let expected_len = usize::try_from(frame.width)
        .ok()
        .and_then(|width| {
            usize::try_from(frame.height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "captured frame dimensions overflowed".to_string())?;
    if frame.rgba.len() != expected_len
        || frame.width == 0
        || frame.height == 0
        || window_frame.width == 0
        || window_frame.height == 0
    {
        return Err("captured frame does not match the focused window".to_string());
    }

    for region in sensitive_regions {
        let bounds = scaled_redaction_bounds(*region, window_frame, frame.width, frame.height)
            .ok_or_else(|| {
                "sensitive accessibility bounds fall outside the captured window".to_string()
            })?;
        fill_black(&mut frame.rgba, frame.width, bounds);

        // Accessibility and image coordinate origins have differed across
        // APIs and display arrangements. Redacting the vertical mirror too is
        // cheap and fails toward hiding more pixels rather than leaking a
        // secure field when an origin convention changes.
        let mirrored = (
            bounds.0,
            frame.height.saturating_sub(bounds.3),
            bounds.2,
            frame.height.saturating_sub(bounds.1),
        );
        if mirrored != bounds {
            fill_black(&mut frame.rgba, frame.width, mirrored);
        }
    }

    let mut png = Vec::new();
    PngEncoder::new(&mut png)
        .write_image(
            &frame.rgba,
            frame.width,
            frame.height,
            ExtendedColorType::Rgba8,
        )
        .map_err(|error| format!("encode focused-window screenshot: {error}"))?;
    if png.is_empty() || png.len() > MAX_COMPUTER_SCREENSHOT_BYTES {
        return Err("focused-window screenshot exceeded its encoded byte bound".to_string());
    }
    Ok(png)
}

fn finish(
    sender: &Arc<Mutex<Option<mpsc::SyncSender<Result<CapturedFrame, String>>>>>,
    result: Result<CapturedFrame, String>,
) {
    if let Ok(mut sender) = sender.lock()
        && let Some(sender) = sender.take()
    {
        let _ = sender.send(result);
    }
}

unsafe fn window_matches(window: &SCWindow, process_id: i32, expected: ComputerRect) -> bool {
    if !unsafe { window.isOnScreen() } {
        return false;
    }
    let Some(application) = (unsafe { window.owningApplication() }) else {
        return false;
    };
    if unsafe { application.processID() } != process_id {
        return false;
    }
    let frame = unsafe { window.frame() };
    approximately(frame.origin.x, f64::from(expected.x))
        && approximately(frame.origin.y, f64::from(expected.y))
        && approximately(frame.size.width, f64::from(expected.width))
        && approximately(frame.size.height, f64::from(expected.height))
}

fn approximately(left: f64, right: f64) -> bool {
    left.is_finite() && right.is_finite() && (left - right).abs() <= FRAME_MATCH_TOLERANCE_POINTS
}

fn target_dimensions(source_width: f64, source_height: f64, max_width: u32) -> (u32, u32) {
    if !source_width.is_finite()
        || !source_height.is_finite()
        || source_width <= 0.0
        || source_height <= 0.0
    {
        return (1, 1);
    }
    let width_scale = f64::from(max_width) / source_width;
    let height_scale = f64::from(MAX_COMPUTER_SCREENSHOT_HEIGHT) / source_height;
    let pixel_scale =
        (MAX_COMPUTER_SCREENSHOT_PIXELS as f64 / (source_width * source_height)).sqrt();
    let scale = width_scale.min(height_scale).min(pixel_scale).min(1.0);
    let width = (source_width * scale)
        .floor()
        .clamp(1.0, f64::from(max_width)) as u32;
    let height = (source_height * scale)
        .floor()
        .clamp(1.0, f64::from(MAX_COMPUTER_SCREENSHOT_HEIGHT)) as u32;
    (width, height)
}

unsafe fn copy_bgra_frame(image: &CGImage) -> Result<CapturedFrame, String> {
    let width = CGImage::width(Some(image));
    let height = CGImage::height(Some(image));
    let bytes_per_row = CGImage::bytes_per_row(Some(image));
    if width == 0
        || height == 0
        || width > u32::MAX as usize
        || height > u32::MAX as usize
        || width.saturating_mul(height) > MAX_COMPUTER_SCREENSHOT_PIXELS as usize
        || CGImage::bits_per_component(Some(image)) != 8
        || CGImage::bits_per_pixel(Some(image)) != 32
        || bytes_per_row < width.saturating_mul(4)
    {
        return Err("ScreenCaptureKit returned an unsupported image layout".to_string());
    }
    let provider = CGImage::data_provider(Some(image))
        .ok_or_else(|| "ScreenCaptureKit image has no data provider".to_string())?;
    let data = CGDataProvider::data(Some(&provider))
        .ok_or_else(|| "ScreenCaptureKit image has no pixel data".to_string())?;
    let source = unsafe { data.as_bytes_unchecked() };
    let source_len = bytes_per_row
        .checked_mul(height)
        .ok_or_else(|| "ScreenCaptureKit image layout overflowed".to_string())?;
    if source.len() < source_len {
        return Err("ScreenCaptureKit image data was shorter than its layout".to_string());
    }
    let mut rgba = vec![0_u8; width.saturating_mul(height).saturating_mul(4)];
    for row in 0..height {
        let source_row = &source[row * bytes_per_row..row * bytes_per_row + width * 4];
        let output_row = &mut rgba[row * width * 4..(row + 1) * width * 4];
        for (bgra, rgba) in source_row
            .chunks_exact(4)
            .zip(output_row.chunks_exact_mut(4))
        {
            rgba.copy_from_slice(&[bgra[2], bgra[1], bgra[0], bgra[3]]);
        }
    }
    Ok(CapturedFrame {
        width: width as u32,
        height: height as u32,
        rgba,
    })
}

fn framework_error(operation: &str, error: *mut NSError) -> String {
    let detail = unsafe { error.as_ref() }
        .map(|error| error.localizedDescription().to_string())
        .filter(|detail| !detail.trim().is_empty())
        .unwrap_or_else(|| "the framework returned no result".to_string());
    format!("{operation}: {detail}")
}

fn scaled_redaction_bounds(
    region: ComputerRect,
    window: ComputerRect,
    image_width: u32,
    image_height: u32,
) -> Option<(u32, u32, u32, u32)> {
    if region.width == 0 || region.height == 0 {
        return None;
    }
    let left = i64::from(region.x) - i64::from(window.x);
    let top = i64::from(region.y) - i64::from(window.y);
    let right = left.saturating_add(i64::from(region.width));
    let bottom = top.saturating_add(i64::from(region.height));
    let scale_x = f64::from(image_width) / f64::from(window.width);
    let scale_y = f64::from(image_height) / f64::from(window.height);
    let x0 = ((left as f64 * scale_x).floor() as i64 - REDACTION_PADDING_PIXELS)
        .clamp(0, i64::from(image_width));
    let y0 = ((top as f64 * scale_y).floor() as i64 - REDACTION_PADDING_PIXELS)
        .clamp(0, i64::from(image_height));
    let x1 = ((right as f64 * scale_x).ceil() as i64 + REDACTION_PADDING_PIXELS)
        .clamp(0, i64::from(image_width));
    let y1 = ((bottom as f64 * scale_y).ceil() as i64 + REDACTION_PADDING_PIXELS)
        .clamp(0, i64::from(image_height));
    (x1 > x0 && y1 > y0).then_some((x0 as u32, y0 as u32, x1 as u32, y1 as u32))
}

fn fill_black(rgba: &mut [u8], width: u32, bounds: (u32, u32, u32, u32)) {
    for y in bounds.1..bounds.3 {
        for x in bounds.0..bounds.2 {
            let offset = (u64::from(y) * u64::from(width) + u64::from(x)) as usize * 4;
            rgba[offset..offset + 4].copy_from_slice(&[0, 0, 0, 255]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_dimensions_never_exceed_transport_bounds() {
        let (width, height) = target_dimensions(8_000.0, 12_000.0, 1_600);
        assert!(width <= 1_600);
        assert!(height <= MAX_COMPUTER_SCREENSHOT_HEIGHT);
        assert!(u64::from(width) * u64::from(height) <= MAX_COMPUTER_SCREENSHOT_PIXELS);
    }

    #[test]
    fn secure_bounds_are_redacted_in_both_vertical_conventions() {
        let frame = CapturedFrame {
            width: 10,
            height: 10,
            rgba: vec![255; 10 * 10 * 4],
        };
        let png = encode_redacted_png(
            frame,
            ComputerRect {
                x: 100,
                y: 200,
                width: 100,
                height: 100,
            },
            &[ComputerRect {
                x: 110,
                y: 220,
                width: 20,
                height: 10,
            }],
        )
        .expect("redacted png");
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    }
}
