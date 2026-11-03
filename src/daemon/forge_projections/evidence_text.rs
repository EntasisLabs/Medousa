//! Streaming text views of durable evidence. File size and individual line
//! length must not dictate retained memory in an interactive review.
use std::io::{self, BufRead};

pub(super) const PAGE_BYTES: usize = 64 * 1024;

/// Visit every logical line, retaining only a bounded prefix. The bool marks
/// an incomplete line prefix. Like str::lines, a trailing newline adds no line.
pub(super) fn visit_line_prefixes(
    mut reader: impl BufRead,
    max_prefix: usize,
    mut visit: impl FnMut(&[u8], bool),
) -> io::Result<()> {
    let mut prefix = Vec::new();
    let mut has_data = false;
    let mut clipped = false;
    loop {
        let available = match reader.fill_buf() {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if available.is_empty() {
            if has_data {
                visit(&prefix, clipped);
            }
            return Ok(());
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let length = newline.unwrap_or(available.len());
        let take = length.min(max_prefix.saturating_sub(prefix.len()));
        prefix.extend_from_slice(&available[..take]);
        clipped |= take < length;
        has_data = true;
        let consumed = length + usize::from(newline.is_some());
        reader.consume(consumed);
        if newline.is_some() {
            if !clipped && prefix.last() == Some(&b'\r') {
                prefix.pop();
            }
            visit(&prefix, clipped);
            prefix.clear();
            clipped = false;
            has_data = false;
        }
    }
}

pub(super) fn read_page(
    reader: impl BufRead,
    offset: usize,
    limit: usize,
) -> io::Result<(Vec<String>, usize, bool)> {
    let limit = limit.min(2000);
    let mut lines = Vec::new();
    let mut total = 0usize;
    let mut bytes = 0usize;
    let mut clipped = false;
    let mut page_full = false;
    visit_line_prefixes(reader, PAGE_BYTES, |prefix, line_clipped| {
        if total >= offset && lines.len() < limit && !page_full {
            let decoded = String::from_utf8_lossy(prefix);
            // Invalid UTF-8 is a lossy preview, never exact evidence.
            clipped |= matches!(decoded, std::borrow::Cow::Owned(_));
            let mut text = decoded.into_owned();
            let remaining = PAGE_BYTES - bytes;
            if text.len() > remaining {
                let mut end = remaining;
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                text.truncate(end);
                clipped = true;
            }
            clipped |= line_clipped;
            bytes += text.len();
            lines.push(text);
            page_full = bytes >= PAGE_BYTES;
        }
        total = total.saturating_add(1);
    })?;
    let truncated = clipped || offset.min(total).saturating_add(lines.len()) < total;
    Ok((lines, total, truncated))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufReader, Cursor};

    #[test]
    fn pages_preserve_line_offsets_crlf_empty_and_unterminated_lines() {
        let text = "a\r\n\nlast";
        assert_eq!(
            read_page(Cursor::new(text), 0, 2).unwrap(),
            (vec!["a".into(), "".into()], 3, true)
        );
        assert_eq!(
            read_page(Cursor::new(text), 2, 10).unwrap(),
            (vec!["last".into()], 3, false)
        );
        assert_eq!(
            read_page(Cursor::new(text), usize::MAX, 10).unwrap(),
            (vec![], 3, false)
        );
        assert_eq!(
            read_page(Cursor::new(""), 0, 10).unwrap(),
            (vec![], 0, false)
        );
        assert_eq!(
            read_page(Cursor::new("a\n"), 0, 0).unwrap(),
            (vec![], 1, true)
        );
        assert_eq!(read_page(Cursor::new("a\r"), 0, 1).unwrap().0, vec!["a\r"]);
    }

    #[test]
    fn oversized_lines_and_pages_are_bounded_but_totals_remain_exact() {
        let text = format!("{}\nafter\n", "x".repeat(9 * 1024 * 1024));
        let (lines, total, truncated) =
            read_page(BufReader::with_capacity(31, Cursor::new(&text)), 0, 2000).unwrap();
        assert_eq!(total, 2);
        assert!(truncated);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].len(), PAGE_BYTES);
        assert_eq!(
            read_page(Cursor::new(&text), 1, 1).unwrap(),
            (vec!["after".into()], 2, false)
        );
        let text = "short line\n".repeat(20_000);
        let (lines, total, truncated) = read_page(Cursor::new(text), 0, usize::MAX).unwrap();
        assert_eq!(total, 20_000);
        assert!(truncated);
        assert!(lines.iter().map(String::len).sum::<usize>() <= PAGE_BYTES);
    }

    #[test]
    fn empty_line_pages_and_lossy_text_still_have_honest_bounds() {
        let (lines, total, truncated) =
            read_page(Cursor::new("\n".repeat(10_000)), 0, usize::MAX).unwrap();
        assert_eq!(lines.len(), 2000);
        assert_eq!(total, 10_000);
        assert!(truncated);
        let (lines, total, truncated) = read_page(Cursor::new([255, b'\n']), 0, 10).unwrap();
        assert_eq!(lines, vec!["\u{fffd}"]);
        assert_eq!(total, 1);
        assert!(truncated);
    }

    #[test]
    fn prefix_scanner_bounds_each_line_and_preserves_line_starts() {
        let text = format!("diff --git a/a b/a\n+{}\n-old\n", "x".repeat(100_000));
        let mut lines = Vec::new();
        visit_line_prefixes(
            BufReader::with_capacity(7, Cursor::new(text)),
            32,
            |line, clipped| {
                lines.push((line.to_vec(), clipped));
            },
        )
        .unwrap();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[1].0.len(), 32);
        assert_eq!(lines[1].0[0], b'+');
        assert!(lines[1].1);
        assert_eq!(lines[2], (b"-old".to_vec(), false));
    }

    #[test]
    fn unicode_prefix_truncation_keeps_valid_strings_and_signals_truncation() {
        let text = format!("{}\nafter", "世".repeat(PAGE_BYTES));
        let (lines, total, truncated) = read_page(Cursor::new(text), 0, 2000).unwrap();
        assert_eq!(total, 2);
        assert!(truncated);
        assert!(lines.iter().map(String::len).sum::<usize>() <= PAGE_BYTES);
    }
}
