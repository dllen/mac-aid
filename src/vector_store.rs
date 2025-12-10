use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredCommand {
    pub id: i64,
    pub package_name: String,
    pub command_name: String,
    pub man_content: String,
    pub embedding: Vec<f32>,
}

pub struct VectorStore {
    conn: Connection,
}

impl VectorStore {
    /// Initialize the vector store with database at the given path
    pub fn new(db_path: PathBuf) -> Result<Self> {
        // Create parent directory if it doesn't exist
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(db_path)?;
        
        // Create tables
        conn.execute(
            "CREATE TABLE IF NOT EXISTS commands (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                package_name TEXT NOT NULL,
                command_name TEXT NOT NULL,
                man_content TEXT NOT NULL,
                embedding BLOB NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_package ON commands(package_name)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_command ON commands(command_name)",
            [],
        )?;

        Ok(Self { conn })
    }

    /// Store a command with its embedding
    pub fn store_command(
        &self,
        package_name: &str,
        command_name: &str,
        man_content: &str,
        embedding: &[f32],
    ) -> Result<i64> {
        // Serialize embedding to bytes
        let embedding_bytes = bincode::serialize(embedding)?;

        self.conn.execute(
            "INSERT INTO commands (package_name, command_name, man_content, embedding)
             VALUES (?1, ?2, ?3, ?4)",
            params![package_name, command_name, man_content, embedding_bytes],
        )?;

        Ok(self.conn.last_insert_rowid())
    }

    /// Get all stored commands
    pub fn get_all_commands(&self) -> Result<Vec<StoredCommand>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, package_name, command_name, man_content, embedding FROM commands"
        )?;

        let commands = stmt
            .query_map([], |row| {
                let embedding_bytes: Vec<u8> = row.get(4)?;
                let embedding: Vec<f32> = bincode::deserialize(&embedding_bytes)
                    .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
                        4,
                        rusqlite::types::Type::Blob,
                        Box::new(e),
                    ))?;

                Ok(StoredCommand {
                    id: row.get(0)?,
                    package_name: row.get(1)?,
                    command_name: row.get(2)?,
                    man_content: row.get(3)?,
                    embedding,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(commands)
    }

    /// Search for similar commands using cosine similarity
    pub fn search_similar(&self, query_embedding: &[f32], top_k: usize) -> Result<Vec<StoredCommand>> {
        let all_commands = self.get_all_commands()?;
        
        let mut scored_commands: Vec<(f32, StoredCommand)> = all_commands
            .into_iter()
            .map(|cmd| {
                let similarity = cosine_similarity(query_embedding, &cmd.embedding);
                (similarity, cmd)
            })
            .collect();

        // Sort by similarity (descending)
        scored_commands.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());

        // Take top k
        Ok(scored_commands
            .into_iter()
            .take(top_k)
            .map(|(_, cmd)| cmd)
            .collect())
    }

    /// Check if database is empty
    pub fn is_empty(&self) -> Result<bool> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM commands",
            [],
            |row| row.get(0),
        )?;
        Ok(count == 0)
    }

    /// Get command count
    pub fn count(&self) -> Result<usize> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM commands",
            [],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }
}

/// Calculate cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let magnitude_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let magnitude_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if magnitude_a == 0.0 || magnitude_b == 0.0 {
        return 0.0;
    }

    dot_product / (magnitude_a * magnitude_b)
}
