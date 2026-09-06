use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::ffi::{CString, c_char, c_void};
use std::ptr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use medousa_computer_bridge::{
    COMPUTER_DRIVER_PROTOCOL_VERSION, COMPUTER_OBSERVATION_SCHEMA_VERSION, ComputerAction,
    ComputerActionReceipt, ComputerActionRequest, ComputerApplication, ComputerDisplay,
    ComputerDriverPreflight, ComputerObservation, ComputerObservationRequest,
    ComputerPermissionKind, ComputerPermissionReport, ComputerPermissionStatus, ComputerRect,
    ComputerSemanticNode, ComputerWindow,
};
use medousa_world::{WorldDriverId, WorldResourceId};
use uuid::Uuid;

use super::PlatformDriverError;

const DRIVER_ID: &str = "driver:computer:macos:accessibility";
const MAX_WINDOWS: usize = 128;
const MAX_DISPLAYS: usize = 32;
const MAX_TEXT_BYTES: usize = 512;
const MAX_AX_DEPTH: usize = 64;
const AX_MESSAGE_TIMEOUT_SECONDS: f32 = 0.25;

pub struct NativeComputerDriver {
    session_id: String,
    observation_generation: String,
    revision: AtomicU64,
    last_observation: RefCell<Option<ObservationCache>>,
}

struct ObservationCache {
    resource_id: WorldResourceId,
    session_id: String,
    observation_generation: String,
    observation_revision: u64,
    elements: BTreeMap<String, OwnedCf>,
}

impl NativeComputerDriver {
    pub fn new() -> Self {
        Self {
            session_id: current_session_id(),
            observation_generation: format!("macos:{}", Uuid::new_v4()),
            revision: AtomicU64::new(0),
            last_observation: RefCell::new(None),
        }
    }

    pub fn preflight(&self) -> ComputerDriverPreflight {
        let accessibility = unsafe { AXIsProcessTrusted() != 0 };
        let screen_capture = unsafe { CGPreflightScreenCaptureAccess() };
        let input_control = unsafe { CGPreflightPostEventAccess() };
        ComputerDriverPreflight {
            protocol_version: COMPUTER_DRIVER_PROTOCOL_VERSION,
            driver_id: driver_id(),
            platform: "macos".to_string(),
            session_id: self.session_id.clone(),
            permissions: vec![
                permission_report(
                    ComputerPermissionKind::Accessibility,
                    accessibility,
                    "Allow medousa-computer in System Settings → Privacy & Security → Accessibility.",
                ),
                permission_report(
                    ComputerPermissionKind::ScreenCapture,
                    screen_capture,
                    "Allow medousa-computer in System Settings → Privacy & Security → Screen & System Audio Recording.",
                ),
                permission_report(
                    ComputerPermissionKind::InputControl,
                    input_control,
                    "Allow medousa-computer to post events when macOS requests input-control access.",
                ),
            ],
            checked_at_ms: now_ms(),
        }
    }

    pub fn observe(
        &self,
        request: ComputerObservationRequest,
    ) -> Result<ComputerObservation, PlatformDriverError> {
        request.validate().map_err(|message| PlatformDriverError {
            code: "invalid_request",
            message,
            retryable: false,
        })?;
        if request.session_id != self.session_id {
            return Err(PlatformDriverError {
                code: "desktop_session_changed",
                message: "the requested macOS login session is not owned by this driver"
                    .to_string(),
                retryable: false,
            });
        }
        if unsafe { AXIsProcessTrusted() == 0 } {
            return Err(PlatformDriverError {
                code: "accessibility_permission_required",
                message: "macOS Accessibility permission is required before observing applications"
                    .to_string(),
                retryable: false,
            });
        }

        let system =
            unsafe { OwnedCf::from_create(AXUIElementCreateSystemWide()) }.ok_or_else(|| {
                PlatformDriverError {
                    code: "accessibility_unavailable",
                    message: "macOS did not provide the system accessibility element".to_string(),
                    retryable: true,
                }
            })?;
        let _ =
            unsafe { AXUIElementSetMessagingTimeout(system.as_ptr(), AX_MESSAGE_TIMEOUT_SECONDS) };
        let focused_application = copy_attribute(system.as_ptr(), "AXFocusedApplication")
            .filter(|value| is_ax_element(value.as_ptr()))
            .ok_or_else(|| PlatformDriverError {
                code: "focused_application_unavailable",
                message: "macOS did not expose a focused application".to_string(),
                retryable: true,
            })?;
        let mut pid = 0_i32;
        let pid_status = unsafe { AXUIElementGetPid(focused_application.as_ptr(), &mut pid) };
        if pid_status != AX_ERROR_SUCCESS || pid <= 0 {
            return Err(PlatformDriverError {
                code: "application_identity_unavailable",
                message: "macOS did not expose the focused application's process identity"
                    .to_string(),
                retryable: true,
            });
        }

        let app_resource_id = WorldResourceId::new(format!("application:pid:{pid}"));
        let app_name = text_attribute(focused_application.as_ptr(), "AXTitle")
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| format!("Application {pid}"));
        let application = ComputerApplication {
            resource_id: app_resource_id.clone(),
            name: app_name,
            application_id: None,
            process_id: u32::try_from(pid).ok(),
            active: true,
        };

        let focused_window = copy_attribute(focused_application.as_ptr(), "AXFocusedWindow")
            .filter(|value| is_ax_element(value.as_ptr()));
        let (mut native_windows, omitted_windows) =
            copy_ax_elements(focused_application.as_ptr(), "AXWindows", MAX_WINDOWS);
        if let Some(focused) = focused_window.as_ref() {
            if let Some(index) = native_windows
                .iter()
                .position(|window| cf_equal(window.as_ptr(), focused.as_ptr()))
            {
                if index != 0 {
                    let focused = native_windows.remove(index);
                    native_windows.insert(0, focused);
                }
            } else if let Some(focused) = retain_cf(focused.as_ptr()) {
                native_windows.insert(0, focused);
            }
        }

        let mut windows = Vec::with_capacity(native_windows.len());
        let mut nodes = Vec::new();
        let mut elements = BTreeMap::new();
        let mut focused_window_resource_id = None;
        let mut truncated = omitted_windows;
        for (index, window) in native_windows.iter().enumerate() {
            let window_resource_id =
                WorldResourceId::new(format!("window:pid:{pid}:index:{index}"));
            let focused = focused_window
                .as_ref()
                .is_some_and(|focused| cf_equal(window.as_ptr(), focused.as_ptr()))
                || bool_attribute(window.as_ptr(), "AXFocused").unwrap_or(false);
            if focused {
                focused_window_resource_id = Some(window_resource_id.clone());
            }
            windows.push(ComputerWindow {
                resource_id: window_resource_id.clone(),
                application_resource_id: app_resource_id.clone(),
                title: text_attribute(window.as_ptr(), "AXTitle").unwrap_or_default(),
                frame: rect_attribute(window.as_ptr(), "AXFrame").unwrap_or_default(),
                minimized: bool_attribute(window.as_ptr(), "AXMinimized").unwrap_or(false),
                focused,
            });

            if nodes.len() >= request.max_nodes as usize {
                truncated = true;
                break;
            }
            truncated |= snapshot_window(
                window,
                &window_resource_id,
                pid,
                index,
                request.max_nodes as usize,
                &mut nodes,
                &mut elements,
            );
        }
        if focused_window_resource_id.is_none() {
            focused_window_resource_id = windows
                .iter()
                .find(|window| window.focused)
                .map(|window| window.resource_id.clone());
        }

        let revision = self.revision.fetch_add(1, Ordering::Relaxed) + 1;
        let observation = ComputerObservation {
            schema_version: COMPUTER_OBSERVATION_SCHEMA_VERSION,
            driver_id: driver_id(),
            resource_id: request.resource_id.clone(),
            session_id: self.session_id.clone(),
            observation_generation: self.observation_generation.clone(),
            revision,
            base_revision: None,
            full: true,
            unchanged: false,
            active_application_resource_id: Some(app_resource_id),
            focused_window_resource_id,
            displays: active_displays(),
            applications: vec![application],
            windows,
            nodes,
            removed_refs: Vec::new(),
            truncated,
            captured_at_ms: now_ms(),
            untrusted_content: true,
        };
        self.last_observation.replace(Some(ObservationCache {
            resource_id: request.resource_id,
            session_id: self.session_id.clone(),
            observation_generation: self.observation_generation.clone(),
            observation_revision: revision,
            elements,
        }));
        Ok(observation)
    }

    pub fn act(
        &self,
        request: ComputerActionRequest,
    ) -> Result<ComputerActionReceipt, PlatformDriverError> {
        request.validate().map_err(|message| PlatformDriverError {
            code: "invalid_request",
            message,
            retryable: false,
        })?;
        if request.session_id != self.session_id {
            return Err(PlatformDriverError {
                code: "desktop_session_changed",
                message: "the requested macOS login session is not owned by this driver"
                    .to_string(),
                retryable: false,
            });
        }
        if unsafe { AXIsProcessTrusted() == 0 } {
            return Err(PlatformDriverError {
                code: "accessibility_permission_required",
                message: "macOS Accessibility permission is required before acting on applications"
                    .to_string(),
                retryable: false,
            });
        }

        let cache = self.last_observation.borrow();
        let cache = cache.as_ref().ok_or_else(|| PlatformDriverError {
            code: "observation_required",
            message: "observe the desktop before requesting a computer action".to_string(),
            retryable: false,
        })?;
        if cache.resource_id != request.resource_id
            || cache.session_id != request.session_id
            || cache.observation_generation != request.observation_generation
            || cache.observation_revision != request.observation_revision
        {
            return Err(PlatformDriverError {
                code: "stale_observation",
                message:
                    "the computer action does not target the driver's latest exact observation"
                        .to_string(),
                retryable: false,
            });
        }
        let element =
            cache
                .elements
                .get(&request.element_ref)
                .ok_or_else(|| PlatformDriverError {
                    code: "element_not_found",
                    message: "the requested element reference is not present in that observation"
                        .to_string(),
                    retryable: false,
                })?;
        if bool_attribute(element.as_ptr(), "AXEnabled") == Some(false) {
            return Err(PlatformDriverError {
                code: "element_disabled",
                message: "the requested accessibility element is disabled".to_string(),
                retryable: false,
            });
        }

        let status = match request.action {
            ComputerAction::Press => {
                let action = cf_string("AXPress").ok_or_else(|| PlatformDriverError {
                    code: "action_unavailable",
                    message: "could not construct the macOS accessibility action".to_string(),
                    retryable: false,
                })?;
                unsafe { AXUIElementPerformAction(element.as_ptr(), action.as_ptr()) }
            }
        };
        if status != AX_ERROR_SUCCESS {
            return Err(PlatformDriverError {
                code: "action_failed",
                message: format!("macOS rejected the accessibility action with status {status}"),
                retryable: false,
            });
        }

        Ok(ComputerActionReceipt {
            driver_id: driver_id(),
            resource_id: request.resource_id,
            session_id: request.session_id,
            observation_generation: request.observation_generation,
            observation_revision: request.observation_revision,
            element_ref: request.element_ref,
            action: request.action,
            completed_at_ms: now_ms(),
        })
    }
}

fn driver_id() -> WorldDriverId {
    WorldDriverId::new(DRIVER_ID)
}

fn permission_report(
    permission: ComputerPermissionKind,
    granted: bool,
    guidance: &str,
) -> ComputerPermissionReport {
    ComputerPermissionReport {
        permission,
        status: if granted {
            ComputerPermissionStatus::Granted
        } else {
            ComputerPermissionStatus::Denied
        },
        can_request: !granted,
        guidance: (!granted).then(|| guidance.to_string()),
    }
}

fn current_session_id() -> String {
    let uid = unsafe { libc::geteuid() };
    let audit_session = unsafe { audit_session_self() };
    format!("macos:uid:{uid}:audit:{audit_session}")
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn snapshot_window(
    window: &OwnedCf,
    window_resource_id: &WorldResourceId,
    pid: i32,
    window_index: usize,
    max_nodes: usize,
    output: &mut Vec<ComputerSemanticNode>,
    elements: &mut BTreeMap<String, OwnedCf>,
) -> bool {
    let Some(root) = retain_cf(window.as_ptr()) else {
        return false;
    };
    let mut queue = VecDeque::from([(root, None, "root".to_string(), 0_usize)]);
    let mut truncated = false;

    while let Some((element, parent_ref, path, depth)) = queue.pop_front() {
        if output.len() >= max_nodes {
            return true;
        }
        let role =
            text_attribute(element.as_ptr(), "AXRole").unwrap_or_else(|| "AXUnknown".to_string());
        let subrole = text_attribute(element.as_ptr(), "AXSubrole");
        let sensitive = role == "AXSecureTextField"
            || subrole
                .as_deref()
                .is_some_and(|value| value.contains("Secure"));
        let element_ref = format!(
            "ax:pid:{pid}:window:{window_index}:path:{:016x}",
            stable_path_hash(&path)
        );
        if let Some(retained) = retain_cf(element.as_ptr()) {
            elements.insert(element_ref.clone(), retained);
        }
        let name = text_attribute(element.as_ptr(), "AXTitle")
            .or_else(|| text_attribute(element.as_ptr(), "AXDescription"))
            .or_else(|| text_attribute(element.as_ptr(), "AXHelp"))
            .unwrap_or_default();
        let value = if sensitive {
            None
        } else {
            text_attribute(element.as_ptr(), "AXValue")
        };
        output.push(ComputerSemanticNode {
            element_ref: element_ref.clone(),
            parent_ref,
            window_resource_id: window_resource_id.clone(),
            role,
            name,
            value,
            bounds: rect_attribute(element.as_ptr(), "AXFrame"),
            enabled: bool_attribute(element.as_ptr(), "AXEnabled").unwrap_or(true),
            focused: bool_attribute(element.as_ptr(), "AXFocused").unwrap_or(false),
            selected: bool_attribute(element.as_ptr(), "AXSelected"),
            sensitive,
        });

        if depth >= MAX_AX_DEPTH {
            truncated |= attribute_value_count(element.as_ptr(), "AXChildren")
                .is_some_and(|count| count > 0);
            continue;
        }
        let available = max_nodes.saturating_sub(output.len() + queue.len());
        let (children, omitted) = copy_ax_elements(element.as_ptr(), "AXChildren", available);
        truncated |= omitted;
        for (index, child) in children.into_iter().enumerate() {
            queue.push_back((
                child,
                Some(element_ref.clone()),
                format!("{path}.{index}"),
                depth + 1,
            ));
        }
    }
    truncated
}

fn active_displays() -> Vec<ComputerDisplay> {
    let mut count = 0_u32;
    if unsafe { CGGetActiveDisplayList(0, ptr::null_mut(), &mut count) } != 0 || count == 0 {
        return Vec::new();
    }
    count = count.min(MAX_DISPLAYS as u32);
    let mut ids = vec![0_u32; count as usize];
    if unsafe { CGGetActiveDisplayList(count, ids.as_mut_ptr(), &mut count) } != 0 {
        return Vec::new();
    }
    ids.truncate(count as usize);
    let main = unsafe { CGMainDisplayID() };
    ids.into_iter()
        .map(|id| {
            let frame = unsafe { CGDisplayBounds(id) };
            let pixel_width = unsafe { CGDisplayPixelsWide(id) } as f64;
            let scale_factor = if frame.size.width > 0.0 {
                pixel_width / frame.size.width
            } else {
                1.0
            };
            ComputerDisplay {
                resource_id: WorldResourceId::new(format!("display:{id}")),
                name: format!("Display {id}"),
                frame: frame.into(),
                scale_factor,
                primary: id == main,
            }
        })
        .collect()
}

fn copy_attribute(element: AXUIElementRef, attribute: &str) -> Option<OwnedCf> {
    let attribute = cf_string(attribute)?;
    let mut value = ptr::null();
    let status = unsafe { AXUIElementCopyAttributeValue(element, attribute.as_ptr(), &mut value) };
    (status == AX_ERROR_SUCCESS)
        .then(|| unsafe { OwnedCf::from_create(value) })
        .flatten()
}

fn text_attribute(element: AXUIElementRef, attribute: &str) -> Option<String> {
    let value = copy_attribute(element, attribute)?;
    cf_value_text(value.as_ptr()).map(|value| truncate_text(&value))
}

fn bool_attribute(element: AXUIElementRef, attribute: &str) -> Option<bool> {
    let value = copy_attribute(element, attribute)?;
    let is_boolean = unsafe { CFGetTypeID(value.as_ptr()) == CFBooleanGetTypeID() };
    is_boolean.then(|| unsafe { CFBooleanGetValue(value.as_ptr()) != 0 })
}

fn rect_attribute(element: AXUIElementRef, attribute: &str) -> Option<ComputerRect> {
    let value = copy_attribute(element, attribute)?;
    let is_ax_value = unsafe { CFGetTypeID(value.as_ptr()) == AXValueGetTypeID() };
    if !is_ax_value || unsafe { AXValueGetType(value.as_ptr()) } != AX_VALUE_TYPE_CG_RECT {
        return None;
    }
    let mut rect = CGRect::default();
    let copied = unsafe {
        AXValueGetValue(
            value.as_ptr(),
            AX_VALUE_TYPE_CG_RECT,
            &mut rect as *mut CGRect as *mut c_void,
        )
    };
    (copied != 0).then(|| rect.into())
}

fn copy_ax_elements(
    element: AXUIElementRef,
    attribute: &str,
    limit: usize,
) -> (Vec<OwnedCf>, bool) {
    let Some(count) = attribute_value_count(element, attribute) else {
        return (Vec::new(), false);
    };
    if count == 0 {
        return (Vec::new(), false);
    }
    let wanted = count.min(limit);
    if wanted == 0 {
        return (Vec::new(), count > 0);
    }
    let Some(attribute) = cf_string(attribute) else {
        return (Vec::new(), true);
    };
    let mut array = ptr::null();
    if unsafe {
        AXUIElementCopyAttributeValues(element, attribute.as_ptr(), 0, wanted as isize, &mut array)
    } != AX_ERROR_SUCCESS
    {
        return (Vec::new(), true);
    }
    let Some(array) = (unsafe { OwnedCf::from_create(array) }) else {
        return (Vec::new(), false);
    };
    if unsafe { CFGetTypeID(array.as_ptr()) != CFArrayGetTypeID() } {
        return (Vec::new(), false);
    }
    let array_count = unsafe { CFArrayGetCount(array.as_ptr()) };
    let values = (0..array_count)
        .filter_map(|index| {
            let value = unsafe { CFArrayGetValueAtIndex(array.as_ptr(), index) };
            is_ax_element(value).then(|| retain_cf(value)).flatten()
        })
        .collect();
    (values, count > wanted)
}

fn attribute_value_count(element: AXUIElementRef, attribute: &str) -> Option<usize> {
    let attribute = cf_string(attribute)?;
    let mut count = 0_isize;
    let status =
        unsafe { AXUIElementGetAttributeValueCount(element, attribute.as_ptr(), &mut count) };
    (status == AX_ERROR_SUCCESS && count >= 0)
        .then(|| usize::try_from(count).ok())
        .flatten()
}

fn is_ax_element(value: CFTypeRef) -> bool {
    !value.is_null() && unsafe { CFGetTypeID(value) == AXUIElementGetTypeID() }
}

fn cf_equal(left: CFTypeRef, right: CFTypeRef) -> bool {
    !left.is_null() && !right.is_null() && unsafe { CFEqual(left, right) != 0 }
}

fn cf_string(value: &str) -> Option<OwnedCf> {
    let value = CString::new(value).ok()?;
    unsafe {
        OwnedCf::from_create(CFStringCreateWithCString(
            ptr::null(),
            value.as_ptr(),
            CF_STRING_ENCODING_UTF8,
        ))
    }
}

fn cf_value_text(value: CFTypeRef) -> Option<String> {
    if value.is_null() {
        return None;
    }
    if unsafe { CFGetTypeID(value) == CFStringGetTypeID() } {
        return cf_string_value(value);
    }
    if unsafe { CFGetTypeID(value) == CFBooleanGetTypeID() } {
        return Some(
            if unsafe { CFBooleanGetValue(value) != 0 } {
                "true"
            } else {
                "false"
            }
            .to_string(),
        );
    }
    None
}

fn cf_string_value(value: CFStringRef) -> Option<String> {
    let length = unsafe { CFStringGetLength(value) };
    if length == 0 {
        return Some(String::new());
    }
    let mut buffer = vec![0_u8; MAX_TEXT_BYTES];
    let mut used = 0_isize;
    let converted = unsafe {
        CFStringGetBytes(
            value,
            CFRange {
                location: 0,
                length,
            },
            CF_STRING_ENCODING_UTF8,
            b'?',
            0,
            buffer.as_mut_ptr(),
            buffer.len() as isize,
            &mut used,
        )
    };
    if converted <= 0 || used <= 0 {
        return None;
    }
    buffer.truncate(usize::try_from(used).ok()?);
    let text = String::from_utf8_lossy(&buffer).into_owned();
    Some(truncate_text_with_marker(&text, converted < length))
}

fn truncate_text(value: &str) -> String {
    truncate_text_with_marker(value, value.len() > MAX_TEXT_BYTES)
}

fn truncate_text_with_marker(value: &str, truncated: bool) -> String {
    if !truncated && value.len() <= MAX_TEXT_BYTES {
        return value.to_string();
    }
    let mut end = value
        .len()
        .min(MAX_TEXT_BYTES.saturating_sub('…'.len_utf8()));
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &value[..end])
}

fn stable_path_hash(value: &str) -> u64 {
    value
        .as_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

fn retain_cf(value: CFTypeRef) -> Option<OwnedCf> {
    if value.is_null() {
        None
    } else {
        unsafe { OwnedCf::from_create(CFRetain(value)) }
    }
}

struct OwnedCf(CFTypeRef);

impl OwnedCf {
    unsafe fn from_create(value: CFTypeRef) -> Option<Self> {
        (!value.is_null()).then_some(Self(value))
    }

    fn as_ptr(&self) -> CFTypeRef {
        self.0
    }
}

impl Drop for OwnedCf {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) };
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct CGSize {
    width: f64,
    height: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct CFRange {
    location: CFIndex,
    length: CFIndex,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

impl From<CGRect> for ComputerRect {
    fn from(value: CGRect) -> Self {
        Self {
            x: finite_i32(value.origin.x),
            y: finite_i32(value.origin.y),
            width: finite_u32(value.size.width),
            height: finite_u32(value.size.height),
        }
    }
}

fn finite_i32(value: f64) -> i32 {
    if value.is_finite() {
        value.round().clamp(i32::MIN as f64, i32::MAX as f64) as i32
    } else {
        0
    }
}

fn finite_u32(value: f64) -> u32 {
    if value.is_finite() {
        value.round().clamp(0.0, u32::MAX as f64) as u32
    } else {
        0
    }
}

type CFTypeRef = *const c_void;
type CFStringRef = CFTypeRef;
type AXUIElementRef = CFTypeRef;
type AXValueRef = CFTypeRef;
type CFTypeId = usize;
type CFIndex = isize;
type Boolean = u8;
type AXError = i32;

const AX_ERROR_SUCCESS: AXError = 0;
const AX_VALUE_TYPE_CG_RECT: u32 = 3;
const CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;

unsafe extern "C" {
    fn audit_session_self() -> u32;
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> Boolean;
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementGetTypeID() -> CFTypeId;
    fn AXUIElementGetPid(element: AXUIElementRef, pid: *mut i32) -> AXError;
    fn AXUIElementSetMessagingTimeout(element: AXUIElementRef, timeout: f32) -> AXError;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> AXError;
    fn AXUIElementGetAttributeValueCount(
        element: AXUIElementRef,
        attribute: CFStringRef,
        count: *mut CFIndex,
    ) -> AXError;
    fn AXUIElementCopyAttributeValues(
        element: AXUIElementRef,
        attribute: CFStringRef,
        index: CFIndex,
        max_values: CFIndex,
        values: *mut CFTypeRef,
    ) -> AXError;
    fn AXUIElementPerformAction(element: AXUIElementRef, action: CFStringRef) -> AXError;
    fn AXValueGetTypeID() -> CFTypeId;
    fn AXValueGetType(value: AXValueRef) -> u32;
    fn AXValueGetValue(value: AXValueRef, value_type: u32, value_ptr: *mut c_void) -> Boolean;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRetain(value: CFTypeRef) -> CFTypeRef;
    fn CFRelease(value: CFTypeRef);
    fn CFGetTypeID(value: CFTypeRef) -> CFTypeId;
    fn CFEqual(left: CFTypeRef, right: CFTypeRef) -> Boolean;
    fn CFStringCreateWithCString(
        allocator: CFTypeRef,
        value: *const c_char,
        encoding: u32,
    ) -> CFStringRef;
    fn CFStringGetTypeID() -> CFTypeId;
    fn CFStringGetLength(value: CFStringRef) -> CFIndex;
    fn CFStringGetBytes(
        value: CFStringRef,
        range: CFRange,
        encoding: u32,
        loss_byte: u8,
        is_external_representation: Boolean,
        buffer: *mut u8,
        max_buffer_length: CFIndex,
        used_buffer_length: *mut CFIndex,
    ) -> CFIndex;
    fn CFBooleanGetTypeID() -> CFTypeId;
    fn CFBooleanGetValue(value: CFTypeRef) -> Boolean;
    fn CFArrayGetTypeID() -> CFTypeId;
    fn CFArrayGetCount(array: CFTypeRef) -> CFIndex;
    fn CFArrayGetValueAtIndex(array: CFTypeRef, index: CFIndex) -> CFTypeRef;
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGPreflightPostEventAccess() -> bool;
    fn CGGetActiveDisplayList(
        max_displays: u32,
        displays: *mut u32,
        display_count: *mut u32,
    ) -> i32;
    fn CGMainDisplayID() -> u32;
    fn CGDisplayBounds(display: u32) -> CGRect;
    fn CGDisplayPixelsWide(display: u32) -> usize;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_preflight_is_read_only_and_complete() {
        let report = NativeComputerDriver::new().preflight();
        assert_eq!(report.protocol_version, COMPUTER_DRIVER_PROTOCOL_VERSION);
        assert_eq!(report.driver_id, driver_id());
        assert_eq!(report.permissions.len(), 3);
        assert!(!report.session_id.is_empty());
    }

    #[test]
    fn geometry_conversion_is_finite_and_saturating() {
        let rect = ComputerRect::from(CGRect {
            origin: CGPoint {
                x: f64::NEG_INFINITY,
                y: -42.4,
            },
            size: CGSize {
                width: f64::NAN,
                height: 720.8,
            },
        });
        assert_eq!(rect.x, 0);
        assert_eq!(rect.y, -42);
        assert_eq!(rect.width, 0);
        assert_eq!(rect.height, 721);
    }

    #[test]
    fn text_truncation_preserves_utf8_and_the_byte_bound() {
        let text = "é".repeat(MAX_TEXT_BYTES);
        let truncated = truncate_text(&text);
        assert!(truncated.ends_with('…'));
        assert!(truncated.len() <= MAX_TEXT_BYTES);
    }
}
