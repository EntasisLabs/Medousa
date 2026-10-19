//! Safari-compatible desktop identification for the system WebKit renderer.

pub(super) fn desktop_user_agent(
    safari_version: Option<&str>,
    macos_major: i64,
    macos_minor: i64,
) -> String {
    let version = safari_version
        .filter(|value| valid_version(value))
        .map(str::to_owned)
        .unwrap_or_else(|| bundled_safari_version(macos_major, macos_minor));
    format!("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/{version} Safari/605.1.15")
}

fn valid_version(version: &str) -> bool {
    let parts: Vec<_> = version.split('.').collect();
    (1..=3).contains(&parts.len())
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        && parts[0].parse::<u32>().is_ok_and(|major| major > 0)
}

// Baseline Safari shipped with macOS, used only if Safari's bundle metadata is unavailable.
fn bundled_safari_version(major: i64, minor: i64) -> String {
    let safari_major = if major >= 26 {
        major
    } else if major >= 11 {
        major + 3
    } else {
        (minor - 2).max(1)
    };
    let safari_minor = if major >= 26 { minor.max(0) } else { 0 };
    format!("{safari_major}.{safari_minor}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_installed_safari_version_even_after_a_browser_only_update() {
        let ua = desktop_user_agent(Some("26.4.1"), 15, 7);
        assert!(ua.ends_with("Version/26.4.1 Safari/605.1.15"));
        assert!(ua.contains("AppleWebKit/605.1.15 (KHTML, like Gecko)"));
        assert!(!ua.contains("Chrome/") && !ua.contains("Mobile/"));
    }

    #[test]
    fn falls_back_to_the_safari_version_bundled_with_each_macos_generation() {
        for (major, minor, expected) in [
            (10, 15, "13.0"),
            (11, 7, "14.0"),
            (14, 6, "17.0"),
            (15, 7, "18.0"),
            (26, 2, "26.2"),
            (27, 0, "27.0"),
        ] {
            assert!(desktop_user_agent(None, major, minor)
                .ends_with(&format!("Version/{expected} Safari/605.1.15")));
        }
    }

    #[test]
    fn rejects_invalid_bundle_versions_instead_of_emitting_an_invalid_http_header() {
        for version in [
            "",
            "0.0",
            "27..1",
            "27.0 Beta",
            "27.0\r\nOther: value",
            "27.0.1.2",
            "99999999999999999999",
        ] {
            assert!(
                desktop_user_agent(Some(version), 27, 0).ends_with("Version/27.0 Safari/605.1.15")
            );
        }
    }
}
