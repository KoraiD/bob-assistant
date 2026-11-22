//! Incremental Server-Sent Events decoder (requirement BR-11).
//!
//! Both llama.cpp (`/completion`) and OpenAI-compatible servers stream `data: <json>` events separated
//! by a blank line. The decoder accepts arbitrary byte chunks; the decoded events never depend on how
//! the network split the stream (property-tested below).
//!
//! Only the parts of the SSE spec these servers use are implemented: `data:` fields (multi-line data
//! joined with `\n`), comments (`:` lines, used for keep-alive pings) and CR/LF/CRLF line endings.
//! Other fields (`event:`, `id:`, `retry:`) are ignored.

/// Hard cap for one event, so a misbehaving server cannot grow memory without bound.
pub const MAX_EVENT_BYTES: usize = 4 << 20;

#[derive(Debug, Default)]
pub struct SseDecoder {
    line: Vec<u8>,
    data: Vec<String>,
    data_bytes: usize,
    /// The previous byte was `\r`, so a following `\n` belongs to the same line ending.
    after_cr: bool,
    overflowed: bool,
}

/// One decoded item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SseItem {
    /// The `data` of a complete event.
    Data(String),
    /// An event exceeded [`MAX_EVENT_BYTES`] and was dropped.
    Overflow,
}

impl SseDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed bytes; returns every event completed by them, in order.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<SseItem> {
        let mut out = Vec::new();
        for &b in chunk {
            match b {
                b'\n' if self.after_cr => {
                    // CRLF: the line was already ended by the CR.
                    self.after_cr = false;
                }
                b'\r' | b'\n' => {
                    self.after_cr = b == b'\r';
                    self.end_line(&mut out);
                }
                _ => {
                    self.after_cr = false;
                    if !self.overflowed {
                        self.line.push(b);
                        if self.line.len() + self.data_bytes > MAX_EVENT_BYTES {
                            self.overflowed = true;
                            self.line.clear();
                            self.data.clear();
                            self.data_bytes = 0;
                        }
                    }
                }
            }
        }
        out
    }

    /// Flush at end of stream: a final event without a trailing blank line is still delivered.
    pub fn finish(&mut self) -> Vec<SseItem> {
        let mut out = Vec::new();
        if !self.line.is_empty() {
            self.end_line(&mut out);
        }
        self.dispatch(&mut out);
        out
    }

    fn end_line(&mut self, out: &mut Vec<SseItem>) {
        let line = std::mem::take(&mut self.line);
        if line.is_empty() {
            self.dispatch(out);
            return;
        }
        if self.overflowed || line.first() == Some(&b':') {
            return; // dropped or comment
        }
        let text = String::from_utf8_lossy(&line);
        let (field, value) = match text.find(':') {
            Some(i) => {
                let v = &text[i + 1..];
                (&text[..i], v.strip_prefix(' ').unwrap_or(v))
            }
            None => (text.as_ref(), ""),
        };
        if field == "data" {
            self.data_bytes += value.len() + 1;
            self.data.push(value.to_string());
        }
    }

    fn dispatch(&mut self, out: &mut Vec<SseItem>) {
        if self.overflowed {
            self.overflowed = false;
            self.data.clear();
            self.data_bytes = 0;
            out.push(SseItem::Overflow);
            return;
        }
        if !self.data.is_empty() {
            out.push(SseItem::Data(self.data.join("\n")));
            self.data.clear();
            self.data_bytes = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn decode_all(chunks: &[&[u8]]) -> Vec<SseItem> {
        let mut d = SseDecoder::new();
        let mut out: Vec<SseItem> = chunks.iter().flat_map(|c| d.push(c)).collect();
        out.extend(d.finish());
        out
    }

    #[test]
    fn decodes_llama_cpp_style_events() {
        let s = b"data: {\"content\":\"Hel\"}\n\ndata: {\"content\":\"lo\"}\n\n";
        assert_eq!(
            decode_all(&[s]),
            vec![
                SseItem::Data("{\"content\":\"Hel\"}".into()),
                SseItem::Data("{\"content\":\"lo\"}".into())
            ]
        );
    }

    #[test]
    fn handles_crlf_cr_and_missing_space() {
        let s = b"data:a\r\n\r\ndata: b\r\rdata: c\n\n";
        assert_eq!(
            decode_all(&[s]),
            vec![
                SseItem::Data("a".into()),
                SseItem::Data("b".into()),
                SseItem::Data("c".into())
            ]
        );
    }

    #[test]
    fn crlf_split_across_chunks_is_one_line_ending() {
        assert_eq!(
            decode_all(&[b"data: x\r", b"\n\r", b"\n"]),
            vec![SseItem::Data("x".into())]
        );
    }

    #[test]
    fn ignores_comments_and_other_fields_and_joins_multiline_data() {
        let s = b": ping\nevent: message\nid: 7\ndata: line1\ndata: line2\n\n";
        assert_eq!(decode_all(&[s]), vec![SseItem::Data("line1\nline2".into())]);
    }

    #[test]
    fn delivers_a_final_event_without_trailing_blank_line() {
        assert_eq!(
            decode_all(&[b"data: [DONE]"]),
            vec![SseItem::Data("[DONE]".into())]
        );
    }

    #[test]
    fn drops_oversized_events_and_recovers() {
        let mut big = b"data: ".to_vec();
        big.extend(std::iter::repeat_n(b'x', MAX_EVENT_BYTES + 1));
        big.extend(b"\n\ndata: ok\n\n");
        assert_eq!(
            decode_all(&[&big]),
            vec![SseItem::Overflow, SseItem::Data("ok".into())]
        );
    }

    fn encode(events: &[String]) -> Vec<u8> {
        events
            .iter()
            .flat_map(|e| {
                let mut v = Vec::new();
                for line in e.split('\n') {
                    v.extend(b"data: ");
                    v.extend(line.as_bytes());
                    v.push(b'\n');
                }
                v.push(b'\n');
                v
            })
            .collect()
    }

    proptest! {
        /// BR-11 / P1: for any events and any chunking, the decoded events equal the originals.
        #[test]
        fn decoding_is_independent_of_chunking(
            events in proptest::collection::vec("[^\r\n]{0,40}(\n[^\r\n]{0,20}){0,2}", 0..10),
            cuts in proptest::collection::vec(any::<prop::sample::Index>(), 0..16),
        ) {
            let bytes = encode(&events);
            let mut idx: Vec<usize> = cuts.iter().map(|c| c.index(bytes.len() + 1)).collect();
            idx.extend([0, bytes.len()]);
            idx.sort_unstable();
            idx.dedup();
            let chunks: Vec<&[u8]> = idx.windows(2).map(|w| &bytes[w[0]..w[1]]).collect();
            let expected: Vec<SseItem> = events.iter().map(|e| SseItem::Data(e.clone())).collect();
            prop_assert_eq!(decode_all(&chunks), expected);
        }

        /// Arbitrary bytes never panic.
        #[test]
        fn arbitrary_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..4096)) {
            let _ = decode_all(&[&bytes]);
        }
    }
}
