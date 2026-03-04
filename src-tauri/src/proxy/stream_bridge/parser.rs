use serde_json::Value;

#[derive(Default)]
pub(super) struct SseDataParser {
    line_buffer: String,
    current_event: Option<String>,
    current_data_lines: Vec<String>,
}

pub(super) struct SseFrame {
    pub(super) event: Option<String>,
    pub(super) payload: SseFramePayload,
}

pub(super) enum SseFramePayload {
    Json(Value),
    Done,
}

impl SseDataParser {
    /// Appends incoming bytes and parses all complete SSE lines currently available.
    pub(super) fn consume_chunk(&mut self, chunk: &[u8]) -> Vec<SseFrame> {
        self.line_buffer.push_str(&String::from_utf8_lossy(chunk));
        let mut out = Vec::new();

        while let Some(newline_idx) = self.line_buffer.find('\n') {
            let mut line = self.line_buffer[..newline_idx].to_string();
            if line.ends_with('\r') {
                let _ = line.pop();
            }
            if let Some(frame) = self.consume_line(&line) {
                out.push(frame);
            }
            self.line_buffer.drain(..=newline_idx);
        }

        out
    }

    /// Flushes any trailing buffered line and pending SSE event on stream completion.
    pub(super) fn drain_remainder(&mut self) -> Vec<SseFrame> {
        let mut out = Vec::new();
        if !self.line_buffer.is_empty() {
            let mut line = std::mem::take(&mut self.line_buffer);
            if line.ends_with('\r') {
                let _ = line.pop();
            }
            if let Some(frame) = self.consume_line(&line) {
                out.push(frame);
            }
        }
        if let Some(frame) = self.flush_event() {
            out.push(frame);
        }
        out
    }

    /// Consumes a single normalized SSE line and updates event/data accumulators.
    fn consume_line(&mut self, line: &str) -> Option<SseFrame> {
        if line.is_empty() {
            return self.flush_event();
        }

        if line.starts_with(':') {
            return None;
        }

        if let Some(rest) = line.strip_prefix("event:") {
            self.current_event = Some(rest.trim_start().to_string());
            return None;
        }

        if let Some(rest) = line.strip_prefix("data:") {
            self.current_data_lines.push(rest.trim_start().to_string());
            return None;
        }

        None
    }

    /// Emits one parsed frame from accumulated event/data lines when a frame boundary is reached.
    fn flush_event(&mut self) -> Option<SseFrame> {
        if self.current_data_lines.is_empty() {
            self.current_event = None;
            return None;
        }

        let event = self.current_event.take();
        let payload = self.current_data_lines.join("\n");
        self.current_data_lines.clear();

        if payload == "[DONE]" {
            return Some(SseFrame {
                event,
                payload: SseFramePayload::Done,
            });
        }

        serde_json::from_str::<Value>(&payload)
            .ok()
            .map(|json| SseFrame {
                event,
                payload: SseFramePayload::Json(json),
            })
    }
}
