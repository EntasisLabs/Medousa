//! Bounded Library/Notes inventory, not a file reader or note-content renderer.
use crate::admin_model::{Record, display_text};
use medousa_sdk::SdkError;
use medousa_types::VaultNotesListResponse;
use std::collections::BTreeSet;

fn incompatible() -> SdkError {
    SdkError::Compatibility("unsupported note inventory".into())
}
/// Conservative subset of daemon-relative note identifiers. These are only
/// displayed; they are never interpolated into URLs or passed to the filesystem.
fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 512
        // Keep the whole identifier within the unchanged shared display bound.
        // Reject the page rather than project silently colliding prefixes.
        && path.chars().count() <= 240
        && !path.chars().any(|c| c.is_control() || invisible(c))
        && !path.contains(['\\', ':', '%', '?', '#'])
        && path
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != "..")
        && path.to_ascii_lowercase().ends_with(".md")
}
fn invisible(c: char) -> bool {
    matches!(c, '\u{061c}' | '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{206f}' | '\u{feff}')
}
fn text(value: &str) -> String {
    display_text(&value.chars().filter(|c| !invisible(*c)).collect::<String>())
}
/// Production's default Library hides system/developer notes (vaultSpaces.ts).
/// This preview is more conservative: no opt-in, hidden paths, linked chat/session
/// or profile notes. Never project tags: they can encode session/private metadata.
fn private(path: &str, title: &str, tags: &[String]) -> bool {
    let path = path.to_lowercase();
    let title = title.to_lowercase();
    path.split('/').any(|s| s.starts_with('.'))
        || path.starts_with("system/")
        || path.starts_with("bugs/")
        || path.contains("medousa-daemon")
        || title.starts_with("ask ·")
        || title.starts_with("ask -")
        || title.starts_with("link test")
        || path
            .rsplit('/')
            .next()
            .unwrap_or("")
            .starts_with("link test")
        || tags.iter().any(|tag| {
            let tag = tag.trim().to_lowercase();
            tag == "session"
                || tag.starts_with("session:")
                || tag.starts_with("chat:")
                || tag.starts_with("profile:")
        })
}

pub fn notes(page: VaultNotesListResponse) -> Result<Vec<Record>, SdkError> {
    if page.reset_required {
        return Err(incompatible());
    }
    // Always first-page-only, including old adapters without pagination metadata.
    // Never treat missing cursor/truncated flags as complete-inventory proof.
    let mut paths = BTreeSet::new();
    let mut rows = Vec::new();
    for note in page.notes.into_iter().take(100) {
        if !safe_path(&note.path) || !paths.insert(note.path.clone()) {
            return Err(incompatible());
        }
        if private(&note.path, &text(&note.title), &note.tags) {
            continue;
        }
        let title = text(&note.title);
        rows.push(Record::new(
            if title.trim().is_empty() {
                text(&note.path)
            } else {
                title
            },
            vec![
                ("Vault-relative path", text(&note.path)),
                ("Modified UTC", note.modified_at_utc.to_rfc3339()),
                (
                    "Coverage",
                    "First-page summary only; not a complete vault listing".into(),
                ),
            ],
        ));
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn page(notes: serde_json::Value) -> VaultNotesListResponse {
        serde_json::from_value(json!({"notes": notes})).unwrap()
    }
    fn note(path: &str, title: &str) -> serde_json::Value {
        json!({"path":path,"title":title,"modified_at_utc":"2026-11-13T00:00:00Z", "content":"NEVER-RENDER", "diagnostics":"NEVER-RENDER"})
    }
    #[test]
    fn unsafe_identifiers_fail_closed_without_echoing_input() {
        for path in [
            "",
            "/secret.md",
            "../secret.md",
            "a/../b.md",
            "a/./b.md",
            "a//b.md",
            "C:/a.md",
            "a\\b.md",
            "%2e%2e/a.md",
            "a.md?key=secret",
            "a.md#secret",
            "a\nb.md",
            "a\u{202e}b.md",
            "a.html",
            "https://host/a.md",
        ] {
            let error = notes(page(json!([note(path, "safe")]))).unwrap_err();
            assert!(matches!(error, SdkError::Compatibility(_)));
            assert_eq!(error.to_string(), incompatible().to_string());
        }
        assert!(
            notes(page(json!([note(
                &format!("{}.md", "x".repeat(512)),
                "safe"
            )])))
            .is_err()
        );
    }
    #[test]
    fn private_system_and_linked_notes_are_not_searchable_or_projected() {
        for (path, title) in [
            ("system/a.md", "secret"),
            (".trash/a.md", "secret"),
            ("a/.hidden.md", "secret"),
            ("bugs/a.md", "secret"),
            ("medousa-daemon/a.md", "secret"),
            ("journal/a.md", "Ask · secret prompt"),
            ("journal/a.md", "Ask - secret prompt"),
            ("journal/link test a.md", "secret"),
        ] {
            assert!(notes(page(json!([note(path, title)]))).unwrap().is_empty());
        }
        for tag in ["session", "session:s", "chat:s", "profile:private"] {
            let mut n = note("journal/a.md", "private");
            n["tags"] = json!([tag]);
            assert!(notes(page(json!([n]))).unwrap().is_empty());
        }
    }
    #[test]
    fn excluded_tags_normalize_whitespace_and_case_in_every_category() {
        for tag in ["session", "session:abc", "chat:abc", "profile:work"] {
            for padded in [
                tag.to_string(),
                tag.to_uppercase(),
                format!(" \t{}\n", tag.to_uppercase()),
                format!("\u{2003}{tag}\u{2003}"),
            ] {
                let mut n = note("journal/ordinary.md", "Ordinary authored note");
                n["tags"] = json!([padded]);
                assert!(notes(page(json!([n]))).unwrap().is_empty());
            }
        }
        let mut n = note("journal/ordinary.md", "Ordinary");
        n["tags"] = json!([" research "]);
        assert_eq!(notes(page(json!([n]))).unwrap().len(), 1);
    }
    #[test]
    fn long_paths_fail_closed_before_shared_display_can_alias_them() {
        for prefix in ["a".repeat(245), "é".repeat(245)] {
            let x = format!("{prefix}x.md");
            let y = format!("{prefix}y.md");
            assert!(notes(page(json!([note(&x, "same"), note(&y, "same")]))).is_err());
        }
        // Multibyte paths obey both independent limits: bytes and characters.
        assert!(
            notes(page(json!([note(
                &format!("{}.md", "界".repeat(170)),
                "same"
            )])))
            .is_err()
        );
        assert!(
            notes(page(json!([note(
                &format!("{}.md", "a".repeat(238)),
                "same"
            )])))
            .is_err()
        );
        for prefix in ["a".repeat(236), "界".repeat(150)] {
            let x = format!("{prefix}x.md");
            let y = format!("{prefix}y.md");
            let rows = notes(page(json!([note(&x, "same"), note(&y, "same")]))).unwrap();
            assert_ne!(rows[0], rows[1]);
            assert_eq!(rows[0].fields[0].1, x);
            assert_eq!(rows[1].fields[0].1, y);
            assert!(rows[0].matches("x.md"));
            assert!(!rows[1].matches("x.md"));
            assert!(rows[1].matches("y.md"));
        }
        // No partial success: an overlong path invalidates even an otherwise safe page.
        assert!(
            notes(page(json!([
                note("ok.md", "OK"),
                note(&format!("{}.md", "a".repeat(240)), "long")
            ])))
            .is_err()
        );
    }
    #[test]
    fn summary_is_bounded_inert_and_omits_tags_kind_bodies_and_cursor() {
        let mut n = note("journal/today.md", "<script>alert(1)</script>\u{202e}\n");
        n["tags"] = json!(["NEVER-RENDER"]);
        n["kind"] = json!("NEVER-RENDER");
        let mut p = page(json!([n]));
        p.next_cursor = Some("NEVER-RENDER".into());
        p.truncated = true;
        let rows = notes(p).unwrap();
        assert_eq!(rows[0].title, "<script>alert(1)</script>");
        assert!(rows[0].matches("TODAY"));
        assert!(!format!("{rows:?}").contains("NEVER-RENDER"));
        assert_eq!(rows[0].fields.len(), 3);
        assert!(rows[0].fields[2].1.contains("not a complete"));
    }
    #[test]
    fn reset_duplicate_empty_and_old_adapter_boundaries() {
        let mut p = page(json!([]));
        p.reset_required = true;
        assert!(notes(p).is_err());
        let n = note("a.md", "A");
        assert!(notes(page(json!([n, n]))).is_err());
        assert!(notes(page(json!([]))).unwrap().is_empty());
        let values: Vec<_> = (0..110)
            .map(|i| note(&format!("journal/{i}.md"), "A"))
            .collect();
        assert_eq!(notes(page(json!(values))).unwrap().len(), 100);
        let rows = notes(page(json!([note("日本語/今日.md", "")]))).unwrap();
        assert_eq!(rows[0].title, "日本語/今日.md");
        assert_eq!(text(&"x".repeat(500)).len(), 240);
    }
}
