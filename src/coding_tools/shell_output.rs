//! Bounded model projection of PTY bytes. The session host retains the raw stream.

use std::collections::VecDeque;

pub(super) const OUTPUT_LIMIT: usize = 32 * 1024;
const HALF_LIMIT: usize = OUTPUT_LIMIT / 2;
const OMITTED: &str = "\n[... shell output omitted ...]\n";

#[derive(Default)]
struct BoundedText {
    head: String,
    tail: VecDeque<char>,
    tail_bytes: usize,
    truncated: bool,
}

impl BoundedText {
    fn push(&mut self, text: &str) {
        for ch in text.chars() {
            if self.tail.is_empty() && self.head.len() + ch.len_utf8() <= HALF_LIMIT {
                self.head.push(ch);
                continue;
            }
            self.tail.push_back(ch);
            self.tail_bytes += ch.len_utf8();
            while self.tail_bytes > HALF_LIMIT {
                self.tail_bytes -= self.tail.pop_front().unwrap().len_utf8();
                self.truncated = true;
            }
        }
    }

    fn finish(self) -> (String, bool) {
        let mut text = self.head;
        if self.truncated {
            text.push_str(OMITTED);
        }
        text.extend(self.tail);
        (text, self.truncated)
    }
}

struct Projection {
    text: BoundedText,
    line: String,
    line_flushed: bool,
    marker: Option<String>,
    started: bool,
    completed: Option<i32>,
}

impl Projection {
    fn newline(&mut self) {
        if !self.line_flushed
            && let Some(marker) = &self.marker
        {
            if self.line == format!("{marker}:begin") {
                self.started = true;
                self.line.clear();
                return;
            }
            if self.started
                && let Some(code) = self.line.strip_prefix(&format!("{marker}:"))
                && let Ok(code) = code.parse::<i32>()
            {
                self.completed = Some(code);
                self.line.clear();
                return;
            }
        }
        if self.started {
            self.text.push(&self.line);
            self.text.push("\n");
        }
        self.line.clear();
        self.line_flushed = false;
    }
}

impl vte::Perform for Projection {
    fn print(&mut self, ch: char) {
        if self.completed.is_some() {
            return;
        }
        self.line.push(ch);
        // Keep only enough pending text to identify a whole marker line.
        if self.line.len() > 256 {
            if self.started {
                self.text.push(&self.line);
            }
            self.line.clear();
            self.line_flushed = true;
        }
    }

    fn execute(&mut self, byte: u8) {
        if self.completed.is_some() {
            return;
        }
        match byte {
            b'\n' => self.newline(),
            b'\t' => self.print('\t'),
            // CR is terminal presentation, not a new transcript line. Other
            // controls, OSC titles/links and CSI styling never enter model text.
            _ => {}
        }
    }
}

pub(super) struct ShellOutput {
    parser: vte::Parser,
    projection: Projection,
}

impl ShellOutput {
    pub(super) fn new(marker: Option<&str>) -> Self {
        Self {
            parser: vte::Parser::new(),
            projection: Projection {
                text: BoundedText::default(),
                line: String::new(),
                line_flushed: false,
                marker: marker.map(str::to_owned),
                started: marker.is_none(),
                completed: None,
            },
        }
    }

    pub(super) fn push(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.projection, bytes);
    }

    pub(super) fn exit_code(&self) -> Option<i32> {
        self.projection.completed
    }

    pub(super) fn finish(mut self) -> (String, bool) {
        if self.projection.started && self.projection.completed.is_none() {
            self.projection.text.push(&self.projection.line);
        }
        self.projection.text.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_sequences_and_utf8_survive_arbitrary_transport_chunking() {
        let raw = "\x1b]0;secret terminal title\x07\x1b[31merror\x1b[0m\r\n\x1b]8;;https://example.test\x1b\\λ link\x1b]8;;\x1b\\\tend";
        let mut output = ShellOutput::new(None);
        for byte in raw.as_bytes() {
            output.push(&[*byte]);
        }
        assert_eq!(output.finish(), ("error\nλ link\tend".into(), false));
    }

    #[test]
    fn completion_needs_a_whole_line_and_excludes_prelude_and_trailing_prompts() {
        let mut output = ShellOutput::new(Some("__DONE__"));
        output.push(b"echoed wrapper\n__DONE__:begin\nresult\n__DONE__:1");
        assert_eq!(output.exit_code(), None);
        output.push(b"27\r\nprompt $ ");
        assert_eq!(output.exit_code(), Some(127));
        assert_eq!(output.finish(), ("result\n".into(), false));
    }

    #[test]
    fn verbose_commands_keep_head_and_tail_and_complete_after_the_output_cap() {
        let mut output = ShellOutput::new(Some("__DONE__"));
        output.push(b"__DONE__:begin\nfirst diagnostic\n");
        output.push("λ".repeat(OUTPUT_LIMIT).as_bytes());
        output.push(b"\nlast diagnostic\n__DONE__:0\n");
        assert_eq!(output.exit_code(), Some(0));
        let (text, truncated) = output.finish();
        assert!(truncated);
        assert!(text.starts_with("first diagnostic\n"));
        assert!(text.ends_with("last diagnostic\n"));
        assert!(text.contains(OMITTED));
        assert!(text.len() <= OUTPUT_LIMIT + OMITTED.len());
    }

    #[test]
    fn oversized_terminal_metadata_is_discarded_without_losing_following_text() {
        let mut output = ShellOutput::new(None);
        output.push(b"\x1b]0;");
        output.push(&vec![b'x'; OUTPUT_LIMIT * 4]);
        output.push(b"\x07diagnostic");
        assert_eq!(output.finish(), ("diagnostic".into(), false));
    }

    #[test]
    fn marker_like_command_output_is_preserved() {
        let mut output = ShellOutput::new(Some("__DONE__"));
        output.push(b"__DONE__:begin\nprefix __DONE__:0\n__DONE__:not-a-code\n");
        assert_eq!(output.exit_code(), None);
        assert_eq!(
            output.finish().0,
            "prefix __DONE__:0\n__DONE__:not-a-code\n"
        );
    }
}
