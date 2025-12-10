use anyhow::Result;
use crate::ollama::OllamaClient;
use crate::vector_store::VectorStore;

pub struct RagPipeline<'a> {
    vector_store: &'a VectorStore,
    ollama_client: &'a OllamaClient,
}

impl<'a> RagPipeline<'a> {
    pub fn new(vector_store: &'a VectorStore, ollama_client: &'a OllamaClient) -> Self {
        Self {
            vector_store,
            ollama_client,
        }
    }

    /// Retrieve relevant context for a user query
    pub async fn retrieve_context(&self, query: &str, top_k: usize) -> Result<String> {
        // Generate embedding for the query
        let query_embedding = self.ollama_client.generate_embedding(query).await?;

        // Search for similar commands
        let similar_commands = self.vector_store.search_similar(&query_embedding, top_k)?;

        // Format the context
        let mut context = String::new();
        for (i, cmd) in similar_commands.iter().enumerate() {
            context.push_str(&format!(
                "--- Command {}: {} ---\n{}\n\n",
                i + 1,
                cmd.command_name,
                truncate_text(&cmd.man_content, 500)
            ));
        }

        Ok(context)
    }

    /// Query with RAG - retrieve context and generate response
    pub async fn query_with_rag(
        &self,
        user_query: &str,
        packages: &[String],
        top_k: usize,
    ) -> Result<String> {
        // Check if vector store has data
        if self.vector_store.is_empty()? {
            // Fall back to query without RAG
            return self.ollama_client.query(user_query, packages, None).await;
        }

        // Retrieve relevant context
        let context = self.retrieve_context(user_query, top_k).await?;

        // Query with context
        self.ollama_client
            .query(user_query, packages, Some(&context))
            .await
    }
}

/// Truncate text to a maximum length
fn truncate_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        format!("{}...", &text[..max_len])
    }
}
