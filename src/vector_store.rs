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
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        
        // Performance optimization: enable WAL mode for better concurrent read performance
        conn.execute_batch("PRAGMA journal_mode = WAL")?;
        // Increase cache size to reduce disk I/O
        conn.execute_batch("PRAGMA cache_size = 10000")?;
        
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
        
        // Early return if no commands exist
        if all_commands.is_empty() {
            return Ok(Vec::new());
        }
        
        let mut scored_commands: Vec<(f32, StoredCommand)> = all_commands
            .into_iter()
            .map(|cmd| {
                let similarity = cosine_similarity(query_embedding, &cmd.embedding);
                (similarity, cmd)
            })
            .collect();

        // Sort by similarity (descending)
        // Use unwrap_or for safety in case of NaN values
        scored_commands.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

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

    /// Clear all commands from the store (used for rebuild)
    pub fn clear(&mut self) -> Result<()> {
        // Use a transaction for safety and performance
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM commands", [])?;
        tx.commit()?;
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db_path() -> PathBuf {
        let mut p = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        p.push(format!("mac_aid_test_{}.db", nanos));
        p
    }

    #[test]
    fn test_cosine_similarity_basic() {
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0];
        let s = cosine_similarity(&a, &b);
        assert!((s - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let s = cosine_similarity(&a, &b);
        assert!((s - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_mismatch_len() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0];
        let s = cosine_similarity(&a, &b);
        assert!((s - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_store_and_get_all() {
        let path = temp_db_path();
        let vs = VectorStore::new(path).unwrap();

        let id1 = vs
            .store_command("pkg", "cmd", "man", &[0.1, 0.2, 0.3])
            .unwrap();
        let id2 = vs
            .store_command("pkg2", "cmd2", "man2", &[0.0, 1.0, 0.0])
            .unwrap();

        let all = vs.get_all_commands().unwrap();
        assert_eq!(all.len(), 2);
        let e1 = all.iter().find(|c| c.id == id1).unwrap().embedding.clone();
        assert_eq!(e1, vec![0.1, 0.2, 0.3]);
        let e2 = all.iter().find(|c| c.id == id2).unwrap().embedding.clone();
        assert_eq!(e2, vec![0.0, 1.0, 0.0]);
    }

    #[test]
    fn test_search_similar_ordering() {
        let path = temp_db_path();
        let vs = VectorStore::new(path).unwrap();
        vs.store_command("p1", "c1", "m", &[1.0, 0.0]).unwrap();
        vs.store_command("p2", "c2", "m", &[0.0, 1.0]).unwrap();

        let res = vs.search_similar(&[0.9, 0.1], 1).unwrap();
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].command_name, "c1");
    }

    #[test]
    fn test_clear_and_is_empty() {
        let path = temp_db_path();
        let mut vs = VectorStore::new(path).unwrap();
        vs.store_command("p", "c", "m", &[0.1, 0.2]).unwrap();
        assert!(!vs.is_empty().unwrap());
        vs.clear().unwrap();
        assert!(vs.is_empty().unwrap());
    }
}
