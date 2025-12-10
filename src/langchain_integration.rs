/// Langchain-rust integration for advanced RAG capabilities
/// This module provides chain composition, prompt templates, and retriever patterns
use anyhow::Result;
use crate::ollama::OllamaClient;
use crate::vector_store::VectorStore;

/// Prompt template for RAG queries using langchain pattern
#[allow(dead_code)]
pub struct PromptTemplate {
    template: String,
}

#[allow(dead_code)]
impl PromptTemplate {
    pub fn new(template: String) -> Self {
        Self { template }
    }

    /// Format the template with given variables
    pub fn format(&self, variables: &[(&str, &str)]) -> String {
        let mut result = self.template.clone();
        for (key, value) in variables {
            let placeholder = format!("{{{}}}", key);
            result = result.replace(&placeholder, value);
        }
        result
    }
}

/// Retriever trait for document retrieval (langchain pattern)
#[allow(dead_code)]
pub trait Retriever {
    fn retrieve(&self, query: &str, top_k: usize) -> Result<Vec<String>>;
}

/// Vector store based retriever implementation
#[allow(dead_code)]
pub struct VectorStoreRetriever<'a> {
    vector_store: &'a VectorStore,
    ollama_client: &'a OllamaClient,
}

#[allow(dead_code)]
impl<'a> VectorStoreRetriever<'a> {
    pub fn new(vector_store: &'a VectorStore, ollama_client: &'a OllamaClient) -> Self {
        Self {
            vector_store,
            ollama_client,
        }
    }

    /// Retrieve documents using semantic similarity
    pub async fn retrieve_async(&self, query: &str, top_k: usize) -> Result<Vec<String>> {
        let query_embedding = self.ollama_client.generate_embedding(query).await?;
        let similar_commands = self.vector_store.search_similar(&query_embedding, top_k)?;

        let docs: Vec<String> = similar_commands
            .iter()
            .map(|cmd| {
                format!(
                    "Tool: {}\nDescription: {}\n",
                    cmd.command_name,
                    truncate_text(&cmd.man_content, 500)
                )
            })
            .collect();

        Ok(docs)
    }
}

/// Chain builder for composing RAG pipeline steps (langchain pattern)
#[allow(dead_code)]
pub struct RagChain<'a> {
    retriever: VectorStoreRetriever<'a>,
    ollama_client: &'a OllamaClient,
    prompt_template: PromptTemplate,
}

#[allow(dead_code)]
impl<'a> RagChain<'a> {
    pub fn new(
        vector_store: &'a VectorStore,
        ollama_client: &'a OllamaClient,
    ) -> Self {
        let retriever = VectorStoreRetriever::new(vector_store, ollama_client);
        let prompt_template = Self::create_rag_template();

        Self {
            retriever,
            ollama_client,
            prompt_template,
        }
    }

    /// Create RAG prompt template
    fn create_rag_template() -> PromptTemplate {
        let template = r#"You are a helpful assistant that recommends command-line tools based on user needs.

Available tools:
{packages}

Relevant documentation:
{context}

User query: {query}

Please recommend the most suitable tool(s) and provide:
1. The tool name
2. A brief description
3. A practical usage example
4. The specific use case scenario

Format your response clearly and concisely."#;

        PromptTemplate::new(template.to_string())
    }

    /// Execute the RAG chain: retrieve -> format -> generate
    pub async fn run(
        &self,
        user_query: &str,
        packages: &[String],
        top_k: usize,
    ) -> Result<String> {
        // Step 1: Retrieve relevant documents
        let retrieved_docs = self.retriever.retrieve_async(user_query, top_k).await?;
        let context = retrieved_docs.join("\n---\n");

        // Step 2: Format prompt with retrieved context
        let packages_str = packages.join(", ");
        let _formatted_prompt = self.prompt_template.format(&[
            ("packages", &packages_str),
            ("context", &context),
            ("query", user_query),
        ]);

        // Step 3: Generate response using LLM
        // Note: ollama_client.query handles the actual LLM call
        self.ollama_client
            .query(user_query, packages, Some(&context))
            .await
    }

    /// Run without retrieval (fallback mode)
    pub async fn run_without_retrieval(
        &self,
        user_query: &str,
        packages: &[String],
    ) -> Result<String> {
        self.ollama_client
            .query(user_query, packages, None)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_template_format() {
        let template = PromptTemplate::new("Query: {query}, Context: {context}".to_string());
        let result = template.format(&[("query", "test"), ("context", "info")]);
        assert_eq!(result, "Query: test, Context: info");
    }
}
