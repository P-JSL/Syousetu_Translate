use super::process::Message;
use super::{Job, MAX_LOG, MAX_OUTPUT};
impl Job {
    pub(super) fn apply_message(&mut self, message: Message) {
        let Some(line) = message.line else {
            self.readers_done += 1;
            return;
        };
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
            if value["event"] == "progress" {
                if let (Some(done), Some(total), Some(unit)) = (
                    value["done"].as_u64(),
                    value["total"].as_u64(),
                    value["unit"].as_str(),
                ) {
                    if done <= total {
                        self.progress = Some((done as usize, total as usize, unit.into()));
                    }
                }
                return;
            }
        }
        if message.stdout && !self.output_truncated {
            if self.output.len() + line.len() < MAX_OUTPUT {
                self.output.push_str(&line);
                self.output.push('\n');
            } else {
                self.output_truncated = true;
            }
        }
        self.log.push_str(&line);
        self.log.push('\n');
        if self.log.len() > MAX_LOG {
            let mut start = self.log.len() - MAX_LOG;
            while !self.log.is_char_boundary(start) {
                start += 1;
            }
            self.log.drain(..start);
        }
    }
}
