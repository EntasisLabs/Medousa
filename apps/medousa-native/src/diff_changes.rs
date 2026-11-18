//! Bounded, UTF-8-safe word changes and exact sealed context reconstruction.
use serde_json::{Value, json};
use std::ops::Range;

fn tokens(text: &str) -> Vec<Range<usize>> {
    let mut out: Vec<Range<usize>> = Vec::new();
    let mut previous = 0;
    for (i, c) in text.char_indices() {
        let kind = if c.is_alphanumeric() || c == '_' {
            1
        } else if c.is_whitespace() {
            2
        } else {
            3
        };
        if kind != 3 && kind == previous {
            out.last_mut().unwrap().end = i + c.len_utf8();
        } else {
            out.push(i..i + c.len_utf8());
        }
        previous = kind;
    }
    out
}

pub(super) fn changed(a: &str, b: &str) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    if a == b {
        return (vec![], vec![]);
    }
    // Never allocate a quadratic matrix for generated/minified source.
    if a.len() + b.len() > 12_000 {
        return (vec![0..a.len()], vec![0..b.len()]);
    }
    let left = tokens(a);
    let right = tokens(b);
    if left.len().saturating_mul(right.len()) > 40_000 {
        return (vec![0..a.len()], vec![0..b.len()]);
    }
    let cols = right.len() + 1;
    let mut table = vec![0u16; (left.len() + 1) * cols];
    for i in 1..=left.len() {
        for j in 1..=right.len() {
            table[i * cols + j] = if a[left[i - 1].clone()] == b[right[j - 1].clone()] {
                table[(i - 1) * cols + j - 1] + 1
            } else {
                table[(i - 1) * cols + j].max(table[i * cols + j - 1])
            };
        }
    }
    let (mut i, mut j) = (left.len(), right.len());
    let (mut old, mut new) = (vec![], vec![]);
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && a[left[i - 1].clone()] == b[right[j - 1].clone()] {
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || table[i * cols + j - 1] >= table[(i - 1) * cols + j]) {
            j -= 1;
            new.push(right[j].clone());
        } else {
            i -= 1;
            old.push(left[i].clone());
        }
    }
    old.reverse();
    new.reverse();
    (old, new)
}

/// Expand only gaps whose two sealed versions agree, never invent coordinates
/// or fill from the mutable checkout. Omit controls for incomplete/invalid data.
pub(crate) fn expanded(value: &Value) -> Option<Value> {
    if value["binary"] == true || value["truncated"] == true {
        return None;
    }
    let old: Vec<_> = value["baseline"]["content"]
        .as_str()?
        .split_inclusive('\n')
        .collect();
    let new: Vec<_> = value["reviewed"]["content"]
        .as_str()?
        .split_inclusive('\n')
        .collect();
    if old.len().saturating_add(new.len()) > 20_000 {
        return None;
    }
    let hunks = value["hunks"].as_array()?;
    if hunks.is_empty() {
        return None;
    }
    let mut lines = vec![];
    let (mut o, mut n) = (0usize, 0usize);
    let mut added = 0;
    let mut gap = |o: &mut usize,
                   n: &mut usize,
                   oe: usize,
                   ne: usize,
                   lines: &mut Vec<Value>|
     -> Option<()> {
        if oe < *o || ne < *n || oe > old.len() || ne > new.len() || oe - *o != ne - *n {
            return None;
        }
        while *o < oe {
            if old[*o] != new[*n] {
                return None;
            }
            lines.push(json!({"kind":"context","old_line":*o+1,"new_line":*n+1,"content":old[*o]}));
            *o += 1;
            *n += 1;
            added += 1;
        }
        Some(())
    };
    for h in hunks {
        let os = usize::try_from(h["old_start"].as_u64()?).ok()?;
        let ns = usize::try_from(h["new_start"].as_u64()?).ok()?;
        let oc = usize::try_from(h["old_count"].as_u64()?).ok()?;
        let nc = usize::try_from(h["new_count"].as_u64()?).ok()?;
        // Unified diff zero-count starts point *after* that line.
        let start_o = if oc == 0 { os } else { os.checked_sub(1)? };
        let start_n = if nc == 0 { ns } else { ns.checked_sub(1)? };
        gap(&mut o, &mut n, start_o, start_n, &mut lines)?;
        let source = h["lines"].as_array()?;
        let (mut consumed_old, mut consumed_new) = (0, 0);
        for line in source {
            let kind = line["kind"].as_str()?;
            if kind != "addition" {
                if line["old_line"].as_u64()? != (start_o + consumed_old + 1) as u64 {
                    return None;
                }
                consumed_old += 1;
            }
            if kind != "deletion" {
                if line["new_line"].as_u64()? != (start_n + consumed_new + 1) as u64 {
                    return None;
                }
                consumed_new += 1;
            }
            if !matches!(kind, "context" | "addition" | "deletion") {
                return None;
            }
        }
        if consumed_old != oc || consumed_new != nc {
            return None;
        }
        lines.extend(source.iter().cloned());
        o = start_o.checked_add(oc)?;
        n = start_n.checked_add(nc)?;
    }
    gap(&mut o, &mut n, old.len(), new.len(), &mut lines)?;
    if added == 0 {
        return None;
    }
    let mut result = value.clone();
    result["hunks"] = json!([{"old_start":if old.is_empty(){0}else{1},"old_count":old.len(),
        "new_start":if new.is_empty(){0}else{1},"new_count":new.len(),"lines":lines}]);
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_words_whitespace_and_multiple_edits() {
        let a = "let café = old(10); // 保留 🦀";
        let b = "let café = new(20); // 保留 🦀";
        let (left, right) = changed(a, b);
        assert_eq!(
            left.iter().map(|r| &a[r.clone()]).collect::<Vec<_>>(),
            ["old", "10"]
        );
        assert_eq!(
            right.iter().map(|r| &b[r.clone()]).collect::<Vec<_>>(),
            ["new", "20"]
        );
        assert!(changed(a, a).0.is_empty());
        let (left, right) = changed("a b", "a  b");
        assert_eq!(left, [1..2]);
        assert_eq!(right, [1..3]);
        assert_eq!(changed("", "🦀").1, [0..4]);
    }
    #[test]
    fn generated_lines_have_bounded_work() {
        let a = "a ".repeat(5000);
        let b = "b ".repeat(5000);
        assert_eq!(changed(&a, &b), (vec![0..a.len()], vec![0..b.len()]));
        let a = "a+".repeat(150);
        let b = "b+".repeat(150);
        assert_eq!(changed(&a, &b), (vec![0..a.len()], vec![0..b.len()]));
    }
    pub(super) fn sample() -> Value {
        json!({"baseline":{"content":"lead\r\nold\r\n\r\ntail"},"reviewed":{"content":"lead\r\nnew\r\n\r\ntail"},
            "hunks":[{"old_start":2,"old_count":1,"new_start":2,"new_count":1,"lines":[
                {"kind":"deletion","old_line":2,"content":"old\r\n"},
                {"kind":"addition","new_line":2,"content":"new\r\n"}]}]})
    }
    #[test]
    fn sealed_context_preserves_both_coordinates_crlf_and_no_final_newline() {
        let expanded = expanded(&sample()).unwrap();
        let lines = expanded["hunks"][0]["lines"].as_array().unwrap();
        assert_eq!(lines.len(), 5);
        assert_eq!(lines[0]["content"], "lead\r\n");
        assert_eq!(lines[3]["old_line"], 3);
        assert_eq!(lines[3]["new_line"], 3);
        assert_eq!(lines[4]["content"], "tail");
        assert!(super::expanded(&expanded).is_none());
    }
    #[test]
    fn incomplete_mismatched_or_pathological_context_never_expands() {
        let mut v = sample();
        v["truncated"] = json!(true);
        assert!(expanded(&v).is_none());
        v["truncated"] = json!(false);
        v["binary"] = json!(true);
        assert!(expanded(&v).is_none());
        v["binary"] = json!(false);
        v["baseline"]["content"] = json!("wrong\nold\n\ntail");
        assert!(expanded(&v).is_none());
        let mut v = sample();
        v["hunks"][0]["old_count"] = json!(9);
        assert!(expanded(&v).is_none());
        let mut v = sample();
        v["hunks"][0]["old_start"] = json!(u64::MAX);
        assert!(expanded(&v).is_none());
        assert!(expanded(&json!({})).is_none());
    }
    #[test]
    fn zero_count_insertion_uses_git_after_line_coordinate() {
        let v = json!({"baseline":{"content":"a\nb\n"},"reviewed":{"content":"a\nx\nb\n"},
            "hunks":[{"old_start":1,"old_count":0,"new_start":2,"new_count":1,
                "lines":[{"kind":"addition","new_line":2,"content":"x\n"}]}]});
        let v = expanded(&v).unwrap();
        assert_eq!(v["hunks"][0]["lines"][2]["old_line"], 2);
        assert_eq!(v["hunks"][0]["lines"][2]["new_line"], 3);
    }
}

#[cfg(test)]
#[test]
fn context_between_hunks_tracks_coordinate_shifts_and_deletion_ranges() {
    let value = json!({"baseline":{"content":"a\nb\nc\nd\ne\nf\n"},
        "reviewed":{"content":"a\nx\ny\nc\nd\nf\n"},"hunks":[
        {"old_start":2,"old_count":1,"new_start":2,"new_count":2,"lines":[
            {"kind":"deletion","old_line":2,"content":"b\n"},
            {"kind":"addition","new_line":2,"content":"x\n"},
            {"kind":"addition","new_line":3,"content":"y\n"}]},
        {"old_start":5,"old_count":1,"new_start":5,"new_count":0,"lines":[
            {"kind":"deletion","old_line":5,"content":"e\n"}]}]});
    let expanded = expanded(&value).unwrap();
    let lines = expanded["hunks"][0]["lines"].as_array().unwrap();
    assert_eq!(
        (lines[4]["old_line"].as_u64(), lines[4]["new_line"].as_u64()),
        (Some(3), Some(4))
    );
    assert_eq!(lines.last().unwrap()["old_line"], 6);
    assert_eq!(lines.last().unwrap()["new_line"], 6);
}
