use crate::parser::extract_and_parse_json;
use async_openai::{
    config::OpenAIConfig,
    types::chat::{
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
        ChatCompletionRequestUserMessage, CreateChatCompletionRequestArgs,
    },
    Client,
};
use async_trait::async_trait;
use pinch_core::{LlmClient, LlmError};
use serde::de::DeserializeOwned;
use tracing::debug;

pub struct OpenAiClient {
    client: Client<OpenAIConfig>,
    model: String,
    temperature: f32,
    max_tokens: u32,
}

impl OpenAiClient {
    pub fn new(
        api_key: Option<String>,
        base_url: Option<String>,
        model: String,
        temperature: f32,
        max_tokens: u32,
    ) -> Self {
        let mut config = OpenAIConfig::new();
        if let Some(key) = api_key {
            config = config.with_api_key(key);
        }
        if let Some(base) = base_url {
            config = config.with_api_base(base);
        }

        let client = Client::with_config(config);
        Self {
            client,
            model,
            temperature,
            max_tokens,
        }
    }
}

#[async_trait]
impl LlmClient for OpenAiClient {
    async fn generate_text(&self, system: &str, user: &str) -> Result<String, LlmError> {
        let system_msg = ChatCompletionRequestMessage::System(ChatCompletionRequestSystemMessage {
            content: system.into(),
            name: None,
        });

        let user_msg = ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
            content: user.into(),
            name: None,
        });

        let request = CreateChatCompletionRequestArgs::default()
            .model(&self.model)
            .messages([system_msg, user_msg])
            .temperature(self.temperature)
            .max_completion_tokens(self.max_tokens)
            .build()
            .map_err(|e| LlmError::Other(e.to_string()))?;

        debug!("Sending completion request to model {}", self.model);
        let response = self
            .client
            .chat()
            .create(request)
            .await
            .map_err(|e| LlmError::Api {
                status: None,
                message: e.to_string(),
            })?;

        let content = response
            .choices
            .first()
            .and_then(|c| c.message.content.clone())
            .unwrap_or_default();

        Ok(content)
    }

    async fn generate_structured<T: DeserializeOwned + Send>(
        &self,
        system: &str,
        user: &str,
    ) -> Result<T, LlmError> {
        let text = self.generate_text(system, user).await?;
        extract_and_parse_json(&text)
    }
}
