//! App-owned links are resolved against the emitting view's daemon, never the
//! current picker or this process's filesystem. Unknown schemes fail closed.
use crate::surfaces::SurfaceId;
use rndr_core::*;

#[derive(Debug, PartialEq, Eq)]
enum Target {
    Note(String, Option<String>),
    Code {
        work: String,
        path: String,
        line: Option<usize>,
    },
    External(String),
}
fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.chars().any(char::is_control)
        && !path.contains(':')
        && !path.split('/').any(|s| s == ".." || s.is_empty())
}
fn decode(s: &str) -> Option<String> {
    // urlencoding accepts malformed percent escapes; app links do not.
    let b = s.as_bytes();
    for i in 0..b.len() {
        if b[i] == b'%'
            && (i + 2 >= b.len() || !b[i + 1].is_ascii_hexdigit() || !b[i + 2].is_ascii_hexdigit())
        {
            return None;
        }
    }
    urlencoding::decode(s).ok().map(|s| s.into_owned())
}
fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    s.get(..prefix.len())
        .filter(|head| head.eq_ignore_ascii_case(prefix))?;
    s.get(prefix.len()..)
}
fn parse(href: &str, note: Option<&str>) -> Option<Target> {
    let href = href.trim();
    if strip_prefix_ci(href, "https://").is_some()
        || strip_prefix_ci(href, "http://").is_some()
        || strip_prefix_ci(href, "mailto:").is_some()
    {
        return check_url(href).ok().map(Target::External);
    }
    if let Some(raw) = strip_prefix_ci(href, "medousa://vault/") {
        let (raw_path, fragment) = raw
            .split_once('#')
            .map_or((raw, None), |(p, f)| (p, Some(f)));
        let path = decode(raw_path)?;
        let fragment = match fragment {
            Some(f) => Some(decode(f)?),
            None => None,
        };
        return safe_path(&path).then_some(Target::Note(path, fragment));
    }
    if let Some(raw) = strip_prefix_ci(href, "medousa://undertaking/") {
        let (location, query) = raw.split_once('?')?;
        let work = decode(location.strip_suffix("/location")?)?;
        if work.is_empty() || work.contains('/') || work.chars().any(char::is_control) {
            return None;
        }
        let mut path = None;
        let mut line = None;
        for pair in query.split('#').next()?.split('&') {
            let (key, value) = pair.split_once('=')?;
            match key {
                "path" => path = Some(decode(&value.replace('+', " "))?),
                "line" => line = Some(value.parse::<usize>().ok().filter(|n| *n > 0)?),
                _ => {}
            }
        }
        let path = path?;
        return safe_path(&path).then_some(Target::Code { work, path, line });
    }
    // Relative note links and core MarkdownView wikilinks share this path.
    // A chat wikilink is vault-root-relative. Code files still require an
    // explicit undertaking ID; never infer one from the active Code picker.
    let base = note.unwrap_or("");
    let (raw, fragment) = href
        .split_once('#')
        .map_or((href, None), |(p, f)| (p, Some(f)));
    let fragment = match fragment {
        Some(f) => Some(decode(f)?),
        None => None,
    };
    if raw.is_empty() && fragment.is_some() && note.is_some() {
        return Some(Target::Note(base.to_owned(), fragment));
    }
    if raw.is_empty() || raw.contains('?') || raw.contains(':') {
        return None;
    }
    let decoded = decode(raw)?;
    if note.is_none()
        && decoded
            .rsplit('/')
            .next()?
            .rsplit_once('.')
            .is_some_and(|(_, ext)| {
                !ext.eq_ignore_ascii_case("md") && !ext.eq_ignore_ascii_case("markdown")
            })
    {
        return None;
    }
    if decoded.starts_with('/') || decoded.contains('\\') {
        return None;
    }
    let parent = base.rsplit_once('/').map_or("", |(p, _)| p);
    let mut parts: Vec<&str> = parent.split('/').filter(|s| !s.is_empty()).collect();
    for part in decoded.split('/') {
        match part {
            "." => {}
            ".." => {
                parts.pop()?;
            }
            "" => return None,
            p => parts.push(p),
        }
    }
    let mut path = parts.join("/");
    if !path.rsplit('/').next()?.contains('.') {
        path.push_str(".md");
    }
    safe_path(&path).then_some(Target::Note(path, fragment))
}

/// Read in Update, after RNDR's Input phase. The host calls this once per frame.
pub fn tick(world: &mut World) {
    for event in events::<LinkActivated>(world).to_vec() {
        let note = crate::notes::link_context(world, event.view);
        let session = crate::timeline::owning_session(world, event.view);
        let owner = if note.is_some() {
            crate::workshops::binding(world, SurfaceId::Notes).map(|b| b.authority)
        } else if let Some(session) = session.as_deref() {
            crate::workshops::owner(world, Some(session))
        } else {
            continue;
        };
        let Some(target) = parse(&event.href, note.as_deref()) else {
            let message = "Cannot open this reference. Use a vault note link or a code location with an explicit work ID; device-file and unsafe links are not opened.";
            if let Some(path) = note.as_deref() {
                crate::notes::link_notice(world, path, message);
            } else if let Some(session) = session.as_deref() {
                crate::ui::link_notice(world, session, message);
            }
            continue;
        };
        if let Target::External(url) = target {
            let _ = request_open_url(world, &url);
            continue;
        }
        let Some(owner) = owner else {
            continue;
        };
        match target {
            Target::Note(path, fragment) => {
                if crate::workshops::navigate_binding(world, SurfaceId::Notes, &owner) {
                    crate::notes::open_path(world, path, fragment);
                }
            }
            Target::Code { work, path, line } => {
                if crate::workshops::navigate_binding(world, SurfaceId::Code, &owner) {
                    crate::code::open_location(world, &work, &path, line);
                }
            }
            Target::External(_) => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_navigation_vault_and_code_contract() {
        assert_eq!(
            parse("medousa://vault/notes%2FHello%20world.md", None),
            Some(Target::Note("notes/Hello world.md".into(), None))
        );
        assert_eq!(
            parse(
                "medousa://undertaking/work-1/location?path=src%2Flib.rs&line=42&entity=opaque",
                None
            ),
            Some(Target::Code {
                work: "work-1".into(),
                path: "src/lib.rs".into(),
                line: Some(42)
            })
        );
    }
    #[test]
    fn native_navigation_relative_note_links() {
        assert_eq!(
            parse("../Other%20note.md", Some("folder/sub/current.md")),
            Some(Target::Note("folder/Other note.md".into(), None))
        );
        assert_eq!(
            parse("Sibling", Some("folder/current.md")),
            Some(Target::Note("folder/Sibling.md".into(), None))
        );
        assert_eq!(parse("../../escape", Some("folder/current.md")), None);
        assert_eq!(parse("src/lib.rs", None), None);
        assert_eq!(
            parse("Notes/Topic", None),
            Some(Target::Note("Notes/Topic.md".into(), None))
        );
    }
    #[test]
    fn native_navigation_rejects_unsafe_links() {
        for href in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "medousa://vault/%2e%2e/secret",
            "medousa://vault/%2Fetc/passwd",
            "medousa://vault/C%3A/secret",
            "medousa://vault/bad%xx",
            "medousa://vault/a%00.md",
            "medousa://undertaking/w/location?path=..%2Fsecret",
            "medousa://undertaking/w/location?path=a&line=0",
        ] {
            assert_eq!(parse(href, Some("note.md")), None, "{href}");
        }
        assert!(matches!(
            parse("https://example.org", None),
            Some(Target::External(_))
        ));
    }
    #[test]
    fn native_navigation_chat_uses_emitting_authority_not_destination() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = crate::ui::app();
        let _channels = crate::workshops::install_fixture(
            app.world_mut(),
            dir.path().join("sources.json"),
            &[("authority-a", "A"), ("authority-b", "B")],
        );
        app.world_mut().insert_resource(Viewport::new(1000., 800.));
        app.update();
        let w = app.world_mut();
        let parent = w.spawn((Layout::column(),));
        let root = crate::timeline::setup(w, parent);
        let snapshot: medousa_app_core::ChatSnapshot = serde_json::from_value(serde_json::json!({
            "sessionId": crate::routing::qualify("authority-b", "s"),
            "authorityId": "authority-b", "revision": 1, "items": []
        }))
        .unwrap();
        crate::timeline::sync(w, root, Some(&snapshot));
        let view = w.spawn((Layout::column(),));
        set_parent(w, view, root);
        send(
            w,
            LinkActivated {
                view,
                href: "medousa://vault/target.md".into(),
            },
        );
        // Event buffers publish at the beginning of the next frame.
        app.update();
        tick(app.world_mut());
        assert_eq!(crate::surfaces::active(app.world()), SurfaceId::Notes);
        assert_eq!(
            crate::workshops::binding(app.world(), SurfaceId::Notes)
                .unwrap()
                .authority,
            "authority-b"
        );
        crate::notes::complete_identity_fixture(app.world_mut());
        assert_eq!(
            crate::notes::selected_path_fixture(app.world()).as_deref(),
            Some("target.md")
        );
    }
    #[test]
    fn native_navigation_retains_note_heading_fragments() {
        assert_eq!(
            parse("#Some%20Heading", Some("folder/current.md")),
            Some(Target::Note(
                "folder/current.md".into(),
                Some("Some Heading".into())
            ))
        );
        assert_eq!(
            parse("medousa://vault/Other.md#deep-heading", None),
            Some(Target::Note("Other.md".into(), Some("deep-heading".into())))
        );
    }
}
