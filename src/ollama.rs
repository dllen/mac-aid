use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex, Semaphore};
use tokio::time::{sleep, Duration};
use rand::Rng;

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
    // Limit concurrent embedding requests
    limiter: Arc<Semaphore>,
    // Retry configuration
    max_retries: usize,
    base_backoff_ms: u64,
    // Simple rate limiter: maximum requests per second for batch calls
    requests_per_second: u32,
    last_request: Mutex<Instant>,
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
            limiter: Arc::new(Semaphore::new(2)), // reduce to 2 concurrent embedding requests to lower QPS
            max_retries: 5,
            base_backoff_ms: 1000, // increase base backoff from 500 to 1000ms
            requests_per_second: 2, // reduce from 5 to 2 requests per second
            last_request: Mutex::new(Instant::now() - Duration::from_millis(500)),
            single_request_delay_ms: 500, // 500ms delay between single requests
        }
    }

    pub async fn query(&self, user_query: &str, packages: &[String], context: Option<&str>) -> Result<String> {
        let prompt = self.build_prompt(user_query, packages, context);
        
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

    /// Generate embeddings for text using Ollama
    pub async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>> {
        #[derive(Serialize)]
        struct EmbedRequest {
            model: String,
            prompt: String,
        }

        #[derive(Deserialize)]
        struct EmbedResponse {
            embedding: Vec<f32>,
        }

        //embeddinggemma:latest
        let request = EmbedRequest {
            model:"all-minilm".to_string(),
            prompt: text.to_string(),
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

    /// Generate embeddings for multiple texts in a single batch request (more efficient)
    pub async fn generate_embeddings_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        #[derive(Serialize)]
        struct BatchEmbedRequest {
            model: String,
            prompts: Vec<String>,
        }

        #[derive(Deserialize)]
        struct BatchEmbedResponse {
            embeddings: Vec<Vec<f32>>,
        }

        if texts.is_empty() {
            return Ok(Vec::new());
        }

        //embeddinggemma:latest
        let request = BatchEmbedRequest {
            model:"all-minilm".to_string(),
            prompts: texts.iter().map(|t| t.to_string()).collect(),
        };

        // Retry loop with exponential backoff
        for attempt in 0..self.max_retries {
            let permit = self.limiter.clone().acquire_owned().await.unwrap();

            // Enforce a simple QPS limit for batch requests
            if self.requests_per_second > 0 {
                let min_interval_ms = 1000u64 / (self.requests_per_second as u64);
                let min_interval = Duration::from_millis(min_interval_ms);
                let mut last = self.last_request.lock().await;
                let now = Instant::now();
                if now.duration_since(*last) < min_interval {
                    let wait = min_interval - now.duration_since(*last);
                    sleep(wait).await;
                }
                // update last_request to now before sending
                *last = Instant::now();
            }

            let resp_result = self
                .client
                .post(format!("{}/api/embeddings", self.base_url))
                .json(&request)
                .send()
                .await;

            drop(permit);

            match resp_result {
                Ok(response) => {
                    if response.status().is_success() {
                        let batch_response: BatchEmbedResponse = response.json().await?;
                        return Ok(batch_response.embeddings);
                    } else {
                        let status = response.status();
                        if status.as_u16() == 429 || status.is_server_error() {
                            if attempt + 1 == self.max_retries {
                                crate::log::log_error(&format!("Batch embedding failed after retries: {}", status));
                                anyhow::bail!("Batch embedding failed after retries: {}", status);
                            }
                        } else {
                            anyhow::bail!("Batch embedding request failed: {}", status);
                        }
                    }
                }
                Err(e) => {
                    if attempt + 1 == self.max_retries {
                        return Err(anyhow::anyhow!("Batch embedding network error: {}", e));
                    }
                }
            }

            // Exponential backoff with jitter
            let exp = 2u64.pow(attempt as u32);
            let base = self.base_backoff_ms.saturating_mul(exp);
            let mut rng = rand::thread_rng();
            let jitter: u64 = rng.gen_range(0..=base);
            let backoff = Duration::from_millis(base.saturating_add(jitter));
            sleep(backoff).await;
        }

        anyhow::bail!("Failed to get batch embeddings after retries")
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
