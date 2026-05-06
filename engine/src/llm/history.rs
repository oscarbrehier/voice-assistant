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
            max_messages: 10
        }
    }

    // pub fn build_history_string(&mut self) -> Vec<Value> {
    //     let max_history_turns = 10;

    //     while self.messages.len() > max_history_turns {
    //         if self.messages.len() >= 2 {
    //             self.messages.remove(0);
    //             self.messages.remove(0);
    //         } else {
    //             self.messages.remove(0);
    //         }
    //     }

    //     self.messages
    //         .iter()
    //         .map(|m| {
    //             serde_json::json!({
    //                 "role": m.role,
    //                 "content": m.content
    //             })
    //         })
    //         .collect()
    // }

    pub fn add_user_input(&mut self, text: &str) {
        self.messages.push(Message::User { content: text.to_string() });
        self.trim_if_needed();
    }

    pub fn add_assistant_response(&mut self, content: Option<String>, tool_calls: Option<Vec<ToolCall>>) {
        self.messages.push(Message::Assistant { content, tool_calls });
        self.trim_if_needed();
    }

    pub fn add_tool_result(&mut self, tool_call_id: String, name: String, result: String) {
        self.messages.push(Message::Tool { content: result, tool_call_id, name });
    }

    fn trim_if_needed(&mut self) {
        if self.messages.len() > self.max_messages {
            self.messages.drain(0..2);
        }
    }
}
