use serde_json::Value;

#[derive(Clone, Debug)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug)]
pub struct ConversationHistory {
    pub messages: Vec<Message>,
}

impl ConversationHistory {
    pub fn new() -> Self {
        Self {
            messages: Vec::new()
        }
    }

    pub fn build_history_string(&mut self) -> Vec<Value> {
        let max_history_turns = 10;

        while self.messages.len() > max_history_turns {
            if self.messages.len() >= 2 {
                self.messages.remove(0);
                self.messages.remove(0);
            } else {
                self.messages.remove(0);
            }
        }

        self.messages
            .iter()
            .map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": m.content
                })
            })
            .collect()
    }

    pub fn add_user_input(&mut self, input: &str) {
        self.messages.push(Message {
            role: "user".to_string(),
            content: input.to_string(),
        });
    }

    pub fn add_assistant_response(&mut self, response: &str) {
        self.messages.push(Message {
            role: "assistant".to_string(),
            content: response.to_string(),
        })
    }
}
