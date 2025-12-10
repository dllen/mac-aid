use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::time::{sleep, Duration};
use rand::Rng;

#[derive(Debug, Serialize, Clone)]
pub struct OllamaOptions {
    num_ctx: Option<u32>,
    temperature: Option<f32>,
    top_p: Option<f32>,
    top_k: Option<i32>,
    repeat_penalty: Option<f32>,
    stop: Option<Vec<String>>,
}

impl Default for OllamaOptions {
    fn default() -> Self {
        Self {
            num_ctx: Some(8192),
            temperature: None,
            top_p: None,
            top_k: None,
            repeat_penalty: None,
            stop: None,
        }
    }
}

#[derive(Debug, Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
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
    embed_model: String,
    options: Option<OllamaOptions>,
    // Limit concurrent embedding requests
    limiter: Arc<Semaphore>,
    // Retry configuration
    max_retries: usize,
    base_backoff_ms: u64,
    // Delay between single embedding requests (ms) - used in fallback scenarios
    #[allow(dead_code)]
    single_request_delay_ms: u64,
}

impl OllamaClient {
    pub fn new(model: String) -> Self {
        Self {
            client: Client::new(),
            base_url: "http://localhost:11434".to_string(),
            model,
            embed_model: "all-minilm".to_string(),
            options: None,
            limiter: Arc::new(Semaphore::new(2)), // reduce to 2 concurrent embedding requests to lower QPS
            max_retries: 5,
            base_backoff_ms: 1000, // increase base backoff from 500 to 1000ms
            single_request_delay_ms: 500, // 500ms delay between single requests
        }
    }

    #[allow(dead_code)]
    pub fn set_options(&mut self, options: OllamaOptions) {
        self.options = Some(options);
    }

    fn effective_options(&self) -> OllamaOptions {
        let mut opts = self.options.clone().unwrap_or_default();
        if opts.num_ctx.is_none() {
            opts.num_ctx = Some(8192);
        }
        opts
    }

    fn build_generate_request(&self, prompt: String) -> OllamaRequest {
        OllamaRequest {
            model: self.model.clone(),
            prompt,
            stream: false,
            options: Some(self.effective_options()),
        }
    }

    pub async fn query(&self, user_query: &str, packages: &[String], context: Option<&str>) -> Result<String> {
        let prompt = self.build_prompt(user_query, packages, context);
        
        let request = self.build_generate_request(prompt);

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

    /// Generate embeddings for text using Ollama
    pub async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>> {
        #[derive(Serialize)]
        struct EmbedRequest {
            model: String,
            prompt: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            options: Option<OllamaOptions>,
        }

        #[derive(Deserialize)]
        struct EmbedResponse {
            embedding: Vec<f32>,
        }

        let request = EmbedRequest {
            model: self.embed_model.clone(),
            prompt: text.to_string(),
            options: Some(self.effective_options()),
        };

        // Retry loop with concurrency limiting and exponential backoff with jitter
        for attempt in 0..self.max_retries {
            // Acquire a permit to limit concurrency
            let permit = self.limiter.clone().acquire_owned().await.unwrap();

            let resp_result = self
                .client
                .post(format!("{}/api/embeddings", self.base_url))
                .json(&request)
                .send()
                .await;

            // permit is dropped here when it goes out of scope
            drop(permit);

            match resp_result {
                Ok(response) => {
                    if response.status().is_success() {
                        let embed_response: EmbedResponse = response.json().await?;
                        return Ok(embed_response.embedding);
                    } else {
                        let status = response.status();
                        // Retry on 429 or 5xx
                        if status.as_u16() == 429 || status.is_server_error() {
                            if attempt + 1 == self.max_retries {
                                anyhow::bail!("Ollama embedding request failed after retries: {}", status);
                            }
                            // backoff below
                        } else {
                            anyhow::bail!("Ollama embedding request failed: {}", status);
                        }
                    }
                }
                Err(e) => {
                    // network or other transient error — retry
                    if attempt + 1 == self.max_retries {
                        return Err(anyhow::anyhow!(e));
                    }
                    // else fall through to backoff
                }
            }

            // Exponential backoff with jitter
            let exp = 2u64.pow(attempt as u32);
            let base = self.base_backoff_ms.saturating_mul(exp);
            // jitter 0..base
            let mut rng = rand::thread_rng();
            let jitter: u64 = rng.gen_range(0..=base);
            let backoff = Duration::from_millis(base.saturating_add(jitter));
            sleep(backoff).await;
        }

        anyhow::bail!("Failed to get embedding after retries")
    }

    #[allow(dead_code)]
    pub fn set_embed_model(&mut self, embed_model: String) {
        self.embed_model = embed_model;
    }


    fn build_prompt(&self, user_query: &str, packages: &[String], context: Option<&str>) -> String {
        let context_section = if let Some(ctx) = context {
            format!(
                r#"

Relevant documentation from installed tools:
{}

"#,
                ctx
            )
        } else {
            String::new()
        };

        format!(
            r#"You are a helpful assistant that recommends command-line tools based on user needs.

Available tools installed via Homebrew:
{}{}
User query: {}

Please recommend the most suitable tool(s) from the available list and provide:
1. The tool name
2. A brief description of what it does
3. A practical usage example with command-line syntax
4. The specific use case scenario

Format your response clearly and concisely."#,
            packages.join(", "),
            context_section,
            user_query
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_prompt_includes_packages_and_query() {
        let client = OllamaClient::new("model".to_string());
        let packages = vec!["git".to_string(), "jq".to_string()];
        let p = client.build_prompt("need", &packages, None);
        assert!(p.contains("git, jq"));
        assert!(p.contains("need"));
        assert!(!p.contains("Relevant documentation from installed tools:"));
    }

    #[test]
    fn test_build_prompt_with_context() {
        let client = OllamaClient::new("model".to_string());
        let packages = vec!["git".to_string()];
        let p = client.build_prompt("q", &packages, Some("CTX"));
        assert!(p.contains("Relevant documentation from installed tools:"));
        assert!(p.contains("CTX"));
    }

    #[test]
    fn test_build_generate_request_includes_options() {
        let mut client = OllamaClient::new("model".to_string());
        client.set_options(OllamaOptions { num_ctx: Some(4096), ..Default::default() });
        let req = client.build_generate_request("prompt".to_string());
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"options\":"));
        assert!(json.contains("num_ctx"));
        assert!(json.contains("4096"));
    }

    #[test]
    fn test_embed_request_includes_options() {
        #[derive(Serialize)]
        struct EmbedRequestMirror {
            model: String,
            prompt: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            options: Option<OllamaOptions>,
        }

        let mut client = OllamaClient::new("model".to_string());
        client.set_options(OllamaOptions { num_ctx: Some(2048), ..Default::default() });
        let req = EmbedRequestMirror {
            model: "all-minilm".to_string(),
            prompt: "text".to_string(),
            options: Some(client.effective_options()),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"options\":"));
        assert!(json.contains("num_ctx"));
        assert!(json.contains("2048"));
    }

    #[test]
    fn test_default_num_ctx_is_8192_for_generate() {
        let client = OllamaClient::new("model".to_string());
        let req = client.build_generate_request("p".to_string());
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"options\":"));
        assert!(json.contains("8192"));
    }

    #[test]
    fn test_effective_options_respects_explicit_num_ctx() {
        let mut client = OllamaClient::new("model".to_string());
        client.set_options(OllamaOptions { num_ctx: Some(1024), ..Default::default() });
        let opts = client.effective_options();
        assert_eq!(opts.num_ctx, Some(1024));
    }
}
