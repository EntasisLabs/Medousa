#[derive(Debug, Clone, Default)]
pub struct TextBuffer {
    text: String,
    cursor: usize,
}

impl TextBuffer {
    pub fn from_text(text: String) -> Self {
        let cursor = text.len();
        Self { text, cursor }
    }

    pub fn as_text(&self) -> &str {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn set_cursor_to_end(&mut self) {
        self.cursor = self.text.len();
    }

    pub fn insert_char(&mut self, c: char) {
        self.text.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    pub fn insert_str(&mut self, value: &str) {
        self.text.insert_str(self.cursor, value);
        self.cursor += value.len();
    }

    pub fn insert_newline(&mut self) {
        self.insert_char('\n');
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let prev = prev_boundary(&self.text, self.cursor);
        self.text.replace_range(prev..self.cursor, "");
        self.cursor = prev;
    }

    pub fn move_left(&mut self) {
        self.cursor = prev_boundary(&self.text, self.cursor);
    }

    pub fn move_right(&mut self) {
        self.cursor = next_boundary(&self.text, self.cursor);
    }

    pub fn move_line_start(&mut self) {
        let (line_start, _) = line_bounds(&self.text, self.cursor);
        self.cursor = line_start;
    }

    pub fn move_line_end(&mut self) {
        let (_, line_end) = line_bounds(&self.text, self.cursor);
        self.cursor = line_end;
    }

    pub fn move_up(&mut self, preferred_col: usize) {
        let (line_start, _) = line_bounds(&self.text, self.cursor);
        if line_start == 0 {
            self.cursor = 0;
            return;
        }

        let prev_line_end = line_start - 1;
        let (prev_start, prev_end) = line_bounds(&self.text, prev_line_end);
        self.cursor = offset_for_visual_col(&self.text, prev_start, prev_end, preferred_col);
    }

    pub fn move_down(&mut self, preferred_col: usize) {
        let (_, line_end) = line_bounds(&self.text, self.cursor);
        if line_end >= self.text.len() {
            self.cursor = self.text.len();
            return;
        }

        let next_start = line_end + 1;
        let (next_line_start, next_line_end) = line_bounds(&self.text, next_start);
        self.cursor =
            offset_for_visual_col(&self.text, next_line_start, next_line_end, preferred_col);
    }

    pub fn line_col(&self) -> (usize, usize) {
        line_col(&self.text, self.cursor)
    }

    pub fn line_count(&self) -> usize {
        if self.text.is_empty() {
            1
        } else {
            self.text.lines().count()
        }
    }

    pub fn line_at(&self, index: usize) -> Option<&str> {
        if self.text.is_empty() {
            return if index == 0 { Some("") } else { None };
        }
        self.text.lines().nth(index)
    }
}

fn prev_boundary(text: &str, cursor: usize) -> usize {
    if cursor == 0 {
        return 0;
    }
    let mut prev = 0usize;
    for (idx, _) in text.char_indices() {
        if idx >= cursor {
            break;
        }
        prev = idx;
    }
    prev
}

fn next_boundary(text: &str, cursor: usize) -> usize {
    if cursor >= text.len() {
        return text.len();
    }
    for (idx, _) in text.char_indices() {
        if idx > cursor {
            return idx;
        }
    }
    text.len()
}

fn line_bounds(text: &str, cursor: usize) -> (usize, usize) {
    let bytes = text.as_bytes();

    let mut start = cursor;
    while start > 0 {
        if bytes[start - 1] == b'\n' {
            break;
        }
        start -= 1;
    }

    let mut end = cursor;
    while end < bytes.len() {
        if bytes[end] == b'\n' {
            break;
        }
        end += 1;
    }

    (start, end)
}

fn line_col(text: &str, cursor: usize) -> (usize, usize) {
    let prefix = &text[..cursor.min(text.len())];
    let line = prefix.bytes().filter(|b| *b == b'\n').count() + 1;
    let col = prefix
        .rsplit('\n')
        .next()
        .map(|v| v.chars().count() + 1)
        .unwrap_or(1);
    (line, col)
}

fn offset_for_visual_col(
    text: &str,
    line_start: usize,
    line_end: usize,
    preferred_col: usize,
) -> usize {
    let line = &text[line_start..line_end];
    let target_col = preferred_col.max(1);

    if target_col == 1 {
        return line_start;
    }

    let mut char_count = 1usize;
    for (idx, c) in line.char_indices() {
        if char_count == target_col {
            return line_start + idx;
        }
        char_count += 1;
        if idx + c.len_utf8() == line.len() {
            break;
        }
    }

    line_end
}

#[cfg(test)]
mod tests {
    use super::TextBuffer;

    #[test]
    fn supports_insert_and_backspace() {
        let mut b = TextBuffer::default();
        b.insert_char('a');
        b.insert_char('b');
        b.backspace();
        assert_eq!(b.as_text(), "a");
    }

    #[test]
    fn supports_line_navigation() {
        let mut b = TextBuffer::from_text("a\nbc".to_string());
        b.move_line_start();
        assert_eq!(b.cursor(), 2);
        b.move_line_end();
        assert_eq!(b.cursor(), 4);
    }

    #[test]
    fn supports_vertical_navigation_with_preferred_column() {
        let mut b = TextBuffer::from_text("abcd\nxy\nmnop".to_string());
        b.move_line_start();
        b.move_up(1);
        b.move_up(1);
        b.move_right();
        b.move_right();
        b.move_right();

        let (_, col) = b.line_col();
        assert_eq!(col, 4);

        b.move_down(col);
        assert_eq!(b.line_col(), (2, 3));

        b.move_down(col);
        assert_eq!(b.line_col(), (3, 4));

        b.move_up(col);
        assert_eq!(b.line_col(), (2, 3));
    }
}
