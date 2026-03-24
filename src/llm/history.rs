use serde_json::Value;

#[derive(Clone, Debug)]
pub struct Message {
    pub role: String,
    pub content: String,
}

pub struct ConversationHistory {
    pub messages: Vec<Message>,
    pub system_prompt: String,
}

impl ConversationHistory {
    pub fn new(system_prompt: &str) -> Self {
        Self {
            messages: vec![Message {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            }],
			system_prompt: system_prompt.to_string()
        }
    }

    pub fn build_history_string(&self) -> Vec<Value> {
        self.messages.iter()
            .map(|m| serde_json::json!({
                "role": m.role,
                "content": m.content
            }))
            .collect::<Vec<_>>()
    }

	pub fn add_user_input(&mut self, input: &str) {
		self.messages.push(Message { role: "user".to_string(), content: input.to_string() });
	}

	pub fn add_assistant_response(&mut self, response: &str) {
		self.messages.push(Message { role: "assistant".to_string(), content: response.to_string() })
	}
}
