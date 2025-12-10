use anyhow::Result;
use crate::indexer;
use crate::ollama::OllamaClient;
use crate::vector_store::VectorStore;
use crate::log;
use std::path::PathBuf;
use tokio::sync::mpsc::UnboundedSender;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

pub async fn build_kb(
    db_path: PathBuf,
    packages: Vec<crate::brew::BrewPackage>,
    status_tx: UnboundedSender<String>,
    kb_ready: Arc<std::sync::atomic::AtomicBool>,
) -> Result<()> {
    // Create a local Ollama client for embedding/generation
    let ollama = OllamaClient::new("llama3.2".to_string());

    // Open (or create) the vector store in this task
    let vs = VectorStore::new(db_path.clone())?;

    // Index packages
    let package_names: Vec<String> = packages.iter().map(|p| p.name.clone()).collect();
    let docs = indexer::index_brew_packages(&package_names).await?;
    let total = docs.len();

    // Process docs in batches
    let batch_size = 10;
    for batch_start in (0..total).step_by(batch_size) {
        let batch_end = std::cmp::min(batch_start + batch_size, total);
        let batch = &docs[batch_start..batch_end];

        let texts: Vec<&str> = batch.iter().map(|d| d.man_content.as_str()).collect();

        match ollama.generate_embeddings_batch(&texts).await {
            Ok(embeddings) => {
                for (doc, embedding) in batch.iter().zip(embeddings.iter()) {
                    if let Err(e) = vs.store_command(
                        &doc.package_name,
                        &doc.command_name,
                        &doc.man_content,
                        embedding,
                    ) {
                        let _ = status_tx.send(format!("Failed to store: {}: {}", doc.command_name, e));
                        log::log_error(&format!("Failed to store during build: {}: {}", doc.command_name, e));
                    }
                }
            }
            Err(e) => {
                let _ = status_tx.send(format!("Batch embedding failed (docs {}-{}): {}", batch_start, batch_end, e));
                log::log_error(&format!("Batch embedding failed during build: {}", e));
                // fallback to single
                for doc in batch {
                    match ollama.generate_embedding(&doc.man_content).await {
                        Ok(embedding) => {
                            if let Err(e) = vs.store_command(
                                &doc.package_name,
                                &doc.command_name,
                                &doc.man_content,
                                &embedding,
                            ) {
                                let _ = status_tx.send(format!("Failed to store: {}: {}", doc.command_name, e));
                                log::log_error(&format!("Failed to store during build fallback: {}: {}", doc.command_name, e));
                            }
                        }
                        Err(e) => {
                            let _ = status_tx.send(format!("Failed to embed: {}: {}", doc.command_name, e));
                            log::log_error(&format!("Failed to embed during build fallback: {}: {}", doc.command_name, e));
                        }
                    }
                }
            }
        }

        // Update status and sleep a bit to yield
        let _ = status_tx.send(format!("Indexed {}/{} commands", batch_end, total));
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Final count
    let count = vs.count()?;
    let _ = status_tx.send(format!("Knowledge base built: {} commands indexed.", count));

    // mark ready
    kb_ready.store(true, Ordering::SeqCst);

    Ok(())
}
