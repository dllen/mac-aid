use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    stream: bool,
}

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    response: String,
    #[allow(dead_code)]
    done: bool,
}

pub struct OllamaClient {
    client: Client,
    base_url: String,
    model: String,
}

impl OllamaClient {
    pub fn new(model: String) -> Self {
        Self {
            client: Client::new(),
            base_url: "http://localhost:11434".to_string(),
            model,
        }
    }

    pub async fn query(&self, user_query: &str, packages: &[String]) -> Result<String> {
        let prompt = self.build_prompt(user_query, packages);
        
        let request = OllamaRequest {
            model: self.model.clone(),
            prompt,
            stream: false,
        };

        let response = self
            .client
            .post(format!("{}/api/generate", self.base_url))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("Ollama API request failed: {}", response.status());
        }

        let ollama_response: OllamaResponse = response.json().await?;
        Ok(ollama_response.response)
    }

    fn build_prompt(&self, user_query: &str, packages: &[String]) -> String {
        format!(
            r#"You are a helpful assistant that recommends command-line tools based on user needs.

Available tools installed via Homebrew:
{}

User query: {}

Please recommend the most suitable tool(s) from the available list and provide:
1. The tool name
2. A brief description of what it does
3. A practical usage example with command-line syntax
4. The specific use case scenario

Format your response clearly and concisely."#,
            packages.join(", "),
            user_query
        )
    }
}
