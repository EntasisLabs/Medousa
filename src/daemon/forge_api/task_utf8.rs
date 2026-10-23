//! Preserve UTF-8 characters split across pipe reads or PTY frames.
#[derive(Default)]
pub(super) struct TaskUtf8 {
    pending: Vec<u8>,
}

impl TaskUtf8 {
    pub fn push(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        let mut result = String::new();
        let mut used = 0;
        while used < self.pending.len() {
            match std::str::from_utf8(&self.pending[used..]) {
                Ok(text) => {
                    result.push_str(text);
                    used = self.pending.len();
                }
                Err(error) => {
                    let valid_end = used + error.valid_up_to();
                    result.push_str(std::str::from_utf8(&self.pending[used..valid_end]).unwrap());
                    used = valid_end;
                    if let Some(invalid) = error.error_len() {
                        result.push('\u{fffd}');
                        used += invalid;
                    } else {
                        break;
                    }
                }
            }
        }
        self.pending.drain(..used);
        result
    }
    pub fn finish(&mut self) -> String {
        let rest = String::from_utf8_lossy(&self.pending).into_owned();
        self.pending.clear();
        rest
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_unicode_at_every_chunk_boundary_and_replaces_invalid_bytes() {
        let text = "▲ Next.js ✓ Ready 🐍 café\x1b[32m green\x1b[0m";
        for split in 0..text.len() {
            let mut decoder = TaskUtf8::default();
            let first = decoder.push(&text.as_bytes()[..split]);
            let second = decoder.push(&text.as_bytes()[split..]);
            assert_eq!(format!("{first}{second}{}", decoder.finish()), text);
        }
        let mut decoder = TaskUtf8::default();
        assert_eq!(decoder.push(b"hi\xff\xe2"), "hi�");
        assert_eq!(decoder.finish(), "�");
    }
}
