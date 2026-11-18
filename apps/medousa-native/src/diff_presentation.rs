//! Shared presentation of daemon diff payloads. Coordinates remain authoritative;
//! source text never contains UI gutters or invented line numbers.
use rndr_core::*;
use rndr_ui::{
    Typography, UiTheme,
    collections::ItemId,
    diff::{DiffAnnotation, DiffMarker, DiffRow, HunkId},
};
use serde_json::Value;

pub(crate) fn syntax_spans(w: &World, content: &str) -> Vec<TextSpan> {
    // Same bounded generic lexer as the editor, not language-semantic tokens.
    let palette = w
        .resource::<UiTheme>()
        .cloned()
        .unwrap_or_default()
        .markdown_theme();
    let original = MarkdownTheme::default();
    let mut runs = code_token_spans(content);
    for run in &mut runs {
        run.color = if run.color == original.keyword {
            palette.keyword
        } else if run.color == original.string {
            palette.string
        } else if run.color == original.number {
            palette.number
        } else if run.color == original.comment {
            palette.comment
        } else {
            run.color
        };
    }
    runs
}

pub(crate) fn row_height(w: &World) -> f32 {
    let style = w
        .resource::<UiTheme>()
        .cloned()
        .unwrap_or_default()
        .typography(Typography::Code);
    text_engine(w).line_height(&style)
}

pub(crate) fn rows(w: &World, value: &Value) -> (Vec<DiffRow>, Vec<DiffAnnotation>) {
    let height = row_height(w);
    let mut rows = Vec::new();
    let mut annotations = Vec::new();
    for (index, hunk) in value["hunks"].as_array().into_iter().flatten().enumerate() {
        let hunk_id = HunkId(index as u64 + 1);
        rows.push(DiffRow {
            id: ItemId(rows.len() as u64 + 1),
            hunk: hunk_id,
            text: format!(
                "@@ -{},{} +{},{} @@",
                hunk["old_start"], hunk["old_count"], hunk["new_start"], hunk["new_count"]
            ),
            marker: DiffMarker::Header,
            height,
        });
        for line in hunk["lines"].as_array().into_iter().flatten() {
            let id = ItemId(rows.len() as u64 + 1);
            // Strip exactly the transport line ending, preserving whitespace,
            // indentation, UTF-8 and blank source lines.
            let content = line["content"].as_str().unwrap_or_default();
            let text = content
                .strip_suffix("\r\n")
                .or_else(|| content.strip_suffix('\n'))
                .unwrap_or(content)
                .to_owned();
            let marker = match line["kind"].as_str() {
                Some("addition") => DiffMarker::Added,
                Some("deletion") => DiffMarker::Removed,
                _ => DiffMarker::Unchanged,
            };
            let coordinate = |key: &str| line[key].as_u64().and_then(|n| u32::try_from(n).ok());
            annotations.push(DiffAnnotation {
                id,
                old_line: coordinate("old_line"),
                new_line: coordinate("new_line"),
                spans: syntax_spans(w, &text),
            });
            rows.push(DiffRow {
                id,
                hunk: hunk_id,
                text,
                marker,
                height,
            });
        }
    }
    (rows, annotations)
}

// Retain only the bounded presentation, not mutable checkout/review authority.
// Remount with the same key so RNDR preserves scroll anchors on theme changes.
#[derive(Clone)]
struct Mounted {
    parent: Entity,
    key: rndr_ui::UiId,
    name: String,
    rows: Vec<DiffRow>,
    annotations: Vec<DiffAnnotation>,
}
impl Component for Mounted {}

pub(crate) fn mount(
    w: &mut World,
    parent: Entity,
    key: rndr_ui::UiId,
    name: String,
    value: &Value,
) -> Result<rndr_ui::diff::DiffHandle, rndr_ui::UiError> {
    let (rows, annotations) = rows(w, value);
    let handle = rndr_ui::diff::DiffView::mount_annotated(
        w,
        parent,
        key,
        rndr_ui::diff::DiffProps::new(&name),
        &rows,
        &annotations,
    )?;
    w.insert(
        handle.root,
        Mounted {
            parent,
            key,
            name,
            rows,
            annotations,
        },
    );
    Ok(handle)
}

pub(crate) fn refresh_appearance(w: &mut World) {
    let retained: Vec<_> = w
        .query::<Mounted>()
        .into_iter()
        .map(|(e, m)| (e, m.clone()))
        .collect();
    let height = row_height(w);
    for (root, mut mounted) in retained {
        for row in &mut mounted.rows {
            row.height = height;
        }
        for (annotation, row) in mounted.annotations.iter_mut().zip(
            mounted
                .rows
                .iter()
                .filter(|r| r.marker != DiffMarker::Header),
        ) {
            annotation.spans = syntax_spans(w, &row.text);
        }
        if rndr_ui::diff::DiffView::mount_annotated(
            w,
            mounted.parent,
            mounted.key,
            rndr_ui::diff::DiffProps::new(&mounted.name),
            &mounted.rows,
            &mounted.annotations,
        )
        .is_ok()
        {
            w.insert(root, mounted);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn source_coordinates_and_utf8_spans_are_not_display_prefixes() {
        let w = World::new();
        let value = json!({"hunks":[{"old_start":40,"old_count":2,"new_start":90,"new_count":3,
            "lines":[{"kind":"context","old_line":40,"new_line":90,"content":"\tlet café = \"你好\";  \r\n"},
                     {"kind":"deletion","old_line":41,"content":"old\n"},
                     {"kind":"addition","new_line":91,"content":"\n"},
                     {"kind":"addition","new_line":92,"content":"    // 🦀\n"}]}]});
        let (rows, annotations) = rows(&w, &value);
        assert_eq!(rows[1].text, "\tlet café = \"你好\";  ");
        assert_eq!(rows[3].text, "");
        assert_eq!(
            (annotations[0].old_line, annotations[0].new_line),
            (Some(40), Some(90))
        );
        assert_eq!(
            (annotations[1].old_line, annotations[1].new_line),
            (Some(41), None)
        );
        assert_eq!(
            (annotations[2].old_line, annotations[2].new_line),
            (None, Some(91))
        );
        assert!(!annotations[0].spans.is_empty());
        for annotation in &annotations {
            let text = &rows.iter().find(|r| r.id == annotation.id).unwrap().text;
            for span in &annotation.spans {
                assert!(span.start <= span.end && span.end <= text.len());
                assert!(text.is_char_boundary(span.start) && text.is_char_boundary(span.end));
            }
        }
        assert_eq!(rows[0].hunk, rows[4].hunk);
    }

    #[test]
    fn empty_diff_and_unrepresentable_coordinates_are_not_invented() {
        let w = World::new();
        assert!(rows(&w, &json!({})).0.is_empty());
        let (_, annotations) = rows(
            &w,
            &json!({"hunks":[{"lines":[
            {"old_line":4294967296_u64,"new_line":-1,"content":""}]}]}),
        );
        assert_eq!(
            (annotations[0].old_line, annotations[0].new_line),
            (None, None)
        );
    }
}

#[cfg(test)]
#[test]
fn rows_follow_reading_scale_not_control_minimum_height() {
    let mut w = World::new();
    w.insert_resource(UiTheme::default());
    let normal = row_height(&w);
    w.resource_mut::<UiTheme>().unwrap().reading_scale = 1.5;
    let (rows, _) = rows(
        &w,
        &serde_json::json!({"hunks":[{"lines":[{"content":"gypq"}]}]}),
    );
    assert!((rows[1].height - normal * 1.5).abs() < 0.01);
}

#[cfg(test)]
#[test]
fn retained_diff_refreshes_annotations_and_metrics_in_place() {
    let mut app = App::new();
    app.add_plugin(rndr_ui::UiPlugin);
    let w = app.world_mut();
    let parent = w.spawn((Layout::column(),));
    let handle = mount(w, parent, rndr_ui::UiId(0xdeadbeef), "Changes".into(),
        &serde_json::json!({"hunks":[{"lines":[{"kind":"addition","new_line":42,"content":"let n = 42;"}]}]})).unwrap();
    let before = w.get::<Mounted>(handle.root).unwrap().clone();
    w.insert_resource(UiTheme {
        appearance: rndr_ui::Appearance::Light,
        reading_scale: 1.5,
        ..Default::default()
    });
    refresh_appearance(w);
    let after = w.get::<Mounted>(handle.root).unwrap();
    assert!(after.rows[1].height > before.rows[1].height);
    assert_eq!(after.annotations[0].new_line, Some(42));
    assert_eq!(
        after.annotations[0].spans,
        syntax_spans(w, &after.rows[1].text)
    );
    assert_eq!(w.query::<Mounted>().len(), 1);
}
