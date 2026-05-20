use serde_json::Value;

use crate::llm::{Message, ToolCall};

#[derive(Debug)]
pub struct ConversationHistory {
    pub messages: Vec<Message>,
    max_messages: usize,
}

impl ConversationHistory {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            max_messages: 10,
        }
    }

    pub fn add_user_input(&mut self, text: &str) {
        self.messages.push(Message::User {
            content: text.to_string(),
        });
        self.trim_if_needed();
    }

    pub fn add_assistant_response(
        &mut self,
        content: Option<String>,
        tool_calls: Option<Vec<ToolCall>>,
    ) {
        self.messages.push(Message::Assistant {
            content,
            tool_calls,
        });
        self.trim_if_needed();
    }

    pub fn add_tool_result(&mut self, tool_call_id: String, name: String, result: String) {
        self.messages.push(Message::Tool {
            content: result,
            tool_call_id,
            name,
        });
    }

    fn trim_if_needed(&mut self) {
        if self.messages.len() <= self.max_messages {
            return;
        }

        let target_drop = self.messages.len() - self.max_messages;

        let mut cut = target_drop;
        while cut < self.messages.len() {
            match &self.messages[cut] {
                Message::User { .. } => break,
                _ => cut += 1,
            }
        }

        if cut < self.messages.len() {
            self.messages.drain(0..cut);
        }
    }

    pub fn ensure_valid_start(&mut self) {
        while let Some(first) = self.messages.first() {
            match first {
                Message::User { .. } => break,
                _ => {
                    self.messages.remove(0);
                }
            }
        }
    }
}
