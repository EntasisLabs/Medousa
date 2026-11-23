//! Sharing: Shared mode, phones paired with this workshop and the pairing
//! QR, and LAN pairing for a workshop this Mac runs. Channels, peers and
//! share bundles are still set up in medousa-home.
use base64::Engine;
use serde::Deserialize;

/// `/v1/shared-mode`.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct SharedMode {
    pub mode: String,
    #[serde(default)]
    pub root_profile_id: String,
}

impl SharedMode {
    pub(crate) fn shared(&self) -> bool {
        self.mode == "shared"
    }
}

/// One paired device from `/pair/status`.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Device {
    pub pairing_id: String,
    pub phone_name: String,
    #[serde(default)]
    pub last_seen: String,
    #[serde(default = "yes")]
    pub trust_active: bool,
    #[serde(default)]
    pub role: Option<String>,
}

const fn yes() -> bool {
    true
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PairStatus {
    #[serde(default)]
    pub paired_devices: Vec<Device>,
}

/// `/qr/image`, with its PNG decoded for drawing.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Qr {
    pub url: String,
    pub short_code: String,
    pub expires_at: Option<std::time::SystemTime>,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QrPayload {
    url: String,
    #[serde(default)]
    expires_at: String,
    short_code: String,
    png_base64: String,
}

pub(crate) fn decode_qr(value: serde_json::Value) -> Result<Qr, String> {
    let p: QrPayload = serde_json::from_value(value).map_err(|e| e.to_string())?;
    let png = base64::engine::general_purpose::STANDARD
        .decode(p.png_base64.trim())
        .map_err(|e| format!("The pairing QR couldn’t be read: {e}"))?;
    let image = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
        .map_err(|e| format!("The pairing QR couldn’t be read: {e}"))?
        .to_rgba8();
    let expires_at = chrono::DateTime::parse_from_rfc3339(&p.expires_at)
        .ok()
        .map(std::time::SystemTime::from);
    Ok(Qr {
        url: p.url,
        short_code: p.short_code,
        expires_at,
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    })
}

/// medousa-home's `formatShortCode`: groups of three.
pub(crate) fn short_code(raw: &str) -> String {
    let c: String = raw
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_uppercase())
        .collect();
    let groups: Vec<&str> = [0..3, 3..6, 6..9]
        .into_iter()
        .filter_map(|r| c.get(r.start..r.end.min(c.len())))
        .filter(|g| !g.is_empty())
        .collect();
    groups.join("-")
}

/// Seconds until `at`, for the refresh countdown.
pub(crate) fn seconds_left(at: Option<std::time::SystemTime>) -> u64 {
    at.and_then(|t| t.duration_since(std::time::SystemTime::now()).ok())
        .map_or(0, |d| d.as_secs())
}

/// medousa-home's `formatCountdown`.
pub(crate) fn countdown(secs: u64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// How a device row describes itself.
pub(crate) fn device_meta(d: &Device) -> String {
    let kind = if d.role.as_deref() == Some("peer") {
        "Peer · share only"
    } else {
        "Workshop portal"
    };
    let seen = chrono::DateTime::parse_from_rfc3339(&d.last_seen)
        .map(|t| {
            format!(
                "Last used {}",
                t.with_timezone(&chrono::Local)
                    .format("%b %-d, %Y, %-I:%M %p")
            )
        })
        .unwrap_or_else(|_| "Last used recently".into());
    let trust = if d.trust_active { "" } else { " · Expired" };
    format!("{kind} · {seen}{trust}")
}

/// LAN pairing as `peer_lan` reports it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Lan {
    pub enabled: bool,
    pub message: String,
}

pub(crate) fn lan(value: &serde_json::Value) -> Lan {
    Lan {
        enabled: value["enabled"].as_bool().unwrap_or(false),
        message: value["message"].as_str().unwrap_or_default().to_string(),
    }
}

/// One read of the section.
#[derive(Clone, Debug)]
pub(crate) struct Read {
    pub shared: Result<SharedMode, String>,
    pub pairs: Result<PairStatus, String>,
    /// Only for a workshop this Mac runs.
    pub lan: Option<Result<serde_json::Value, String>>,
}

/// An older workshop without the route says so quietly; anything else is
/// shown as it came.
pub(crate) fn missing_or(error: &str, what: &str) -> String {
    if error.contains("404") || error.to_ascii_lowercase().contains("not found") {
        "Not available on this workshop yet".into()
    } else {
        format!("{what} couldn’t be read: {error}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_and_countdowns_read_as_medousa_home_shows_them() {
        assert_eq!(short_code("abc123xyz"), "ABC-123-XYZ");
        assert_eq!(short_code("ab-c12"), "ABC-12");
        assert_eq!(short_code("ab"), "AB");
        assert_eq!(countdown(125), "2:05");
        assert_eq!(seconds_left(None), 0);
        let d: Device = serde_json::from_value(serde_json::json!({
            "pairingId": "p1", "phoneId": "x", "phoneName": "Pixel",
            "pairedAt": "2026-11-01T00:00:00Z", "lastSeen": "bogus", "role": "peer",
            "trustActive": false
        }))
        .unwrap();
        assert_eq!(
            device_meta(&d),
            "Peer · share only · Last used recently · Expired"
        );
    }

    #[test]
    fn the_qr_png_decodes_for_drawing() {
        let mut png = Vec::new();
        image::RgbaImage::from_pixel(4, 4, image::Rgba([0, 0, 0, 255]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let qr = decode_qr(serde_json::json!({
            "url": "medousa://pair?x", "expiresAt": "2099-01-01T00:00:00Z",
            "shortCode": "abc123", "pngBase64": base64::engine::general_purpose::STANDARD.encode(&png)
        }))
        .unwrap();
        assert_eq!((qr.width, qr.height, qr.rgba.len()), (4, 4, 64));
        assert!(seconds_left(qr.expires_at) > 0);
        assert!(
            decode_qr(serde_json::json!({"url": "", "shortCode": "", "pngBase64": "!!"})).is_err()
        );
    }
}
