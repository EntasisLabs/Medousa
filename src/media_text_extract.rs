//! Local text extraction for user media at import time (P5a-text).

use std::io::Cursor;
use std::path::Path;
use std::process::{Command, Stdio};

use calamine::{open_workbook_auto_from_rs, Data, Reader};

pub const MAX_MEDIA_EXTRACT_CHARS: usize = 8_000;
const MAX_SPREADSHEET_ROWS: usize = 200;
const MAX_SPREADSHEET_COLS: usize = 24;
const MAX_PDF_PAGES: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaTextExtract {
    pub text: String,
    pub truncated: bool,
    pub method: String,
}

pub fn extract_media_text(bytes: &[u8], mime: &str, label: Option<&str>) -> Option<MediaTextExtract> {
    let mime = mime.trim().to_ascii_lowercase();
    let hint = label
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| None);

    let result = match mime.as_str() {
        "text/plain" | "text/markdown" => extract_plain_text(bytes),
        "text/csv" | "text/tab-separated-values" => {
            extract_delimited(bytes, mime == "text/tab-separated-values")
        }
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
        | "application/vnd.ms-excel" => extract_spreadsheet(bytes, hint),
        "application/pdf" => extract_pdf(bytes, hint),
        _ => None,
    };

    result.map(|mut extract| {
        cap_extract(&mut extract, MAX_MEDIA_EXTRACT_CHARS);
        extract
    })
}

pub fn extract_path_for_media(payload_path: &str) -> std::path::PathBuf {
    Path::new(payload_path).with_extension("extract.txt")
}

fn extract_plain_text(bytes: &[u8]) -> Option<MediaTextExtract> {
    let text = decode_lossy_utf8(bytes);
    if text.trim().is_empty() {
        return None;
    }
    Some(MediaTextExtract {
        text,
        truncated: false,
        method: "plain".to_string(),
    })
}

fn extract_delimited(bytes: &[u8], tsv: bool) -> Option<MediaTextExtract> {
    let text = decode_lossy_utf8(bytes);
    if text.trim().is_empty() {
        return None;
    }
    let delimiter = if tsv { '\t' } else { ',' };
    let rows = parse_delimited_records(&text, delimiter);
    if rows.is_empty() {
        return Some(MediaTextExtract {
            text,
            truncated: false,
            method: "delimited_raw".to_string(),
        });
    }
    rows_to_markdown_table(rows, "sheet")
}

fn extract_spreadsheet(bytes: &[u8], label: Option<&str>) -> Option<MediaTextExtract> {
    let cursor = Cursor::new(bytes);
    let mut workbook = open_workbook_auto_from_rs(cursor).ok()?;
    let sheet_name = workbook.sheet_names().first().cloned()?;
    let range = workbook.worksheet_range(&sheet_name).ok()?;
    let mut rows: Vec<Vec<String>> = Vec::new();
    for row in range.rows().take(MAX_SPREADSHEET_ROWS + 1) {
        let cells = row
            .iter()
            .take(MAX_SPREADSHEET_COLS)
            .map(cell_to_string)
            .collect::<Vec<_>>();
        if cells.iter().any(|cell| !cell.trim().is_empty()) {
            rows.push(cells);
        }
    }
    if rows.is_empty() {
        return None;
    }
    let sheet_label = label.unwrap_or(&sheet_name);
    rows_to_markdown_table(rows, sheet_label)
}

fn extract_pdf(bytes: &[u8], label: Option<&str>) -> Option<MediaTextExtract> {
    if let Some(extract) = extract_pdf_rust(bytes) {
        if !extract.text.trim().is_empty() {
            return Some(extract);
        }
    }
    extract_pdf_pdftotext(bytes, label)
}

fn extract_pdf_rust(bytes: &[u8]) -> Option<MediaTextExtract> {
    let text = pdf_extract::extract_text_from_mem(bytes).ok()?;
    let page_count = text.matches('\u{000c}').count() + 1;
    let limited = if page_count > MAX_PDF_PAGES {
        text.split('\u{000c}')
            .take(MAX_PDF_PAGES)
            .collect::<Vec<_>>()
            .join("\n\n--- page break ---\n\n")
    } else {
        text
    };
    if limited.trim().is_empty() {
        return None;
    }
    Some(MediaTextExtract {
        truncated: page_count > MAX_PDF_PAGES,
        text: limited,
        method: "pdf_extract".to_string(),
    })
}

fn extract_pdf_pdftotext(bytes: &[u8], label: Option<&str>) -> Option<MediaTextExtract> {
    let temp_dir = std::env::temp_dir().join(format!(
        "medousa-pdf-{}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&temp_dir).ok()?;
    let pdf_path = temp_dir.join(label.unwrap_or("attachment.pdf"));
    let txt_path = temp_dir.join("out.txt");
    if std::fs::write(&pdf_path, bytes).is_err() {
        let _ = std::fs::remove_dir_all(&temp_dir);
        return None;
    }

    let output = Command::new("pdftotext")
        .arg("-l")
        .arg(MAX_PDF_PAGES.to_string())
        .arg(&pdf_path)
        .arg(&txt_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    let result = (|| {
        output.ok().filter(|status| status.success())?;
        let text = std::fs::read_to_string(&txt_path).ok()?;
        if text.trim().is_empty() {
            return None;
        }
        Some(MediaTextExtract {
            text,
            truncated: false,
            method: "pdftotext".to_string(),
        })
    })();

    let _ = std::fs::remove_dir_all(&temp_dir);
    result
}

fn rows_to_markdown_table(mut rows: Vec<Vec<String>>, sheet_label: &str) -> Option<MediaTextExtract> {
    if rows.is_empty() {
        return None;
    }

    let width = rows
        .iter()
        .map(|row| row.len())
        .max()
        .unwrap_or(0)
        .min(MAX_SPREADSHEET_COLS);
    for row in &mut rows {
        row.truncate(width);
        while row.len() < width {
            row.push(String::new());
        }
    }

    let total_rows = rows.len().saturating_sub(1);
    let truncated_rows = total_rows > MAX_SPREADSHEET_ROWS;
    if rows.len() > MAX_SPREADSHEET_ROWS + 1 {
        rows.truncate(MAX_SPREADSHEET_ROWS + 1);
    }

    let header = rows.first()?.clone();
    let body: &[Vec<String>] = if rows.len() > 1 { &rows[1..] } else { &[] };
    let mut out = format!("Sheet: {sheet_label}\n\n");
    out.push('|');
    for cell in &header {
        out.push(' ');
        out.push_str(&sanitize_table_cell(cell));
        out.push_str(" |");
    }
    out.push('\n');
    out.push('|');
    for _ in &header {
        out.push_str(" --- |");
    }
    out.push('\n');
    for row in body {
        out.push('|');
        for cell in row {
            out.push(' ');
            out.push_str(&sanitize_table_cell(cell));
            out.push_str(" |");
        }
        out.push('\n');
    }
    if truncated_rows {
        out.push_str(&format!(
            "\n(table truncated to {} data rows)\n",
            MAX_SPREADSHEET_ROWS
        ));
    }

    Some(MediaTextExtract {
        text: out,
        truncated: truncated_rows,
        method: "spreadsheet".to_string(),
    })
}

fn sanitize_table_cell(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ").trim().to_string()
}

fn cell_to_string(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::String(value) => value.clone(),
        Data::Float(value) => value.to_string(),
        Data::Int(value) => value.to_string(),
        Data::Bool(value) => value.to_string(),
        Data::DateTime(value) => value.to_string(),
        Data::DateTimeIso(value) => value.clone(),
        Data::DurationIso(value) => value.clone(),
        Data::Error(_) => String::new(),
    }
}

fn decode_lossy_utf8(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn cap_extract(extract: &mut MediaTextExtract, max_chars: usize) {
    if extract.text.chars().count() <= max_chars {
        return;
    }
    let truncated: String = extract.text.chars().take(max_chars).collect();
    extract.text = format!("{truncated}\n\n[extract truncated at {max_chars} chars]");
    extract.truncated = true;
}

fn parse_delimited_records(text: &str, delimiter: char) -> Vec<Vec<String>> {
    let input = strip_bom(text);
    let mut records = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut in_quotes = false;
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '"' {
            if in_quotes && chars.peek() == Some(&'"') {
                cell.push('"');
                chars.next();
            } else {
                in_quotes = !in_quotes;
            }
            continue;
        }

        if !in_quotes && ch == delimiter {
            row.push(cell);
            cell = String::new();
            continue;
        }

        if !in_quotes && (ch == '\n' || ch == '\r') {
            if ch == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            row.push(cell);
            if row.iter().any(|value| !value.trim().is_empty()) {
                records.push(row);
            }
            row = Vec::new();
            cell = String::new();
            continue;
        }

        cell.push(ch);
    }

    row.push(cell);
    if row.iter().any(|value| !value.trim().is_empty()) {
        records.push(row);
    }

    records
        .into_iter()
        .map(|row| {
            row.into_iter()
                .take(MAX_SPREADSHEET_COLS)
                .map(|cell| cell.trim().to_string())
                .collect()
        })
        .take(MAX_SPREADSHEET_ROWS + 1)
        .collect()
}

fn strip_bom(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_extracts_markdown_table() {
        let csv = b"name,score\nAda,98\nBob,87\n";
        let extract = extract_media_text(csv, "text/csv", Some("scores.csv")).expect("extract");
        assert!(extract.text.contains("| name |"));
        assert!(extract.text.contains("Ada"));
        assert_eq!(extract.method, "spreadsheet");
    }

    #[test]
    fn plain_text_extract() {
        let extract = extract_media_text(b"hello notes", "text/plain", None).expect("extract");
        assert_eq!(extract.text, "hello notes");
    }

    #[test]
    fn caps_long_plain_text() {
        let bytes = "x".repeat(10_000).into_bytes();
        let extract = extract_media_text(&bytes, "text/plain", None).expect("extract");
        assert!(extract.truncated);
        assert!(extract.text.chars().count() <= MAX_MEDIA_EXTRACT_CHARS + 64);
    }
}
