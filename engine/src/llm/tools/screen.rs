use std::io::Cursor;

use async_trait::async_trait;
use screenshots::{Screen, image::{self, EncodableLayout}};

use crate::llm::{
    FunctionDefinition,
    mistral::call_mistral_with_vision,
    tools::{Tool, ToolContext, ToolOutcome},
};

pub struct LookAtScreen;

const PROMPT: &str = "Describe what is currently visible on the user's screen.
{{user_context_block}}
Include:

- All readable text, preserving its structure. Mark unclear text as [unreadable].
- The kind of content (photograph, screenshot, painting, document, video, UI, etc.)
- Visual elements — composition, colors, notable objects, atmosphere
- Identification where you can be confident: name objects, places, brands, artworks, or recognizable items. If unsure, describe rather than guess.
- For people in photographs, describe their appearance, expression, and setting rather than identifying them by name.

Be thorough and factual. Don't interpret intent or offer advice; just describe what is there.";

#[async_trait]
impl Tool for LookAtScreen {
    fn name(&self) -> &'static str {
        "look_at_screen"
    }

    fn definition(&self) -> FunctionDefinition {
        FunctionDefinition {
			name: "look_at_screen".to_string(),
			description: "Read the user's screen to see what they're looking at. Use when the user references something they're looking at, asks 'what's this', or asks about visible content. Pass the user's question or context so the description can focus accordingly.".to_string(),
			parameters: serde_json::json!({
				"type": "object",
				"properties": {
					"query": {
					 	"type": "string",
                    	"description": "The user's question or what they want to know about the screen. Used to focus the description. Leave empty for general description."
					}
				},
				"required": []
			})
		}
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        _ctx: &ToolContext<'_>,
    ) -> anyhow::Result<ToolOutcome> {
        let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");

        let context_block = if query.is_empty() {
            String::new()
        } else {
            format!(
                "\nThe user just asked: \"{}\". Pay particular attention to elements relevant to this question, but don't omit other notable details.\n",
                query
            )
        };

        let prompt = PROMPT.replace("{{user_context_block}}", &context_block);

        let screens = Screen::all()?;
        let primary = screens.first().ok_or_else(|| anyhow::anyhow!("No screen found"))?;
        let image = primary.capture()?;

        let mut png_bytes: Vec<u8> = Vec::new();
        image.write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png)?;

        let response = call_mistral_with_vision(prompt, &png_bytes).await?;
        
        image.save(format!("test/test-{}.png", primary.display_info.id))?;

        Ok(ToolOutcome {
            result: response,
            needs_identity_refresh: false,
        })
    }
}
