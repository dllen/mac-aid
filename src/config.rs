use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub ollama_model: String,
    pub embedding_model: String,
}

fn config_path() -> Result<std::path::PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
    let dir = home.join(".mac-aid");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("config.json"))
}

pub fn load_config() -> Result<Config> {
    let path = config_path()?;
    if path.exists() {
        let bytes = std::fs::read(&path)?;
        let cfg: Config = serde_json::from_slice(&bytes)?;
        return Ok(cfg);
    }

    let default = Config {
        ollama_model: "qwen3-coder:480b-cloud".to_string(),
        embedding_model: "all-minilm".to_string(),
    };
    let json = serde_json::to_vec_pretty(&default)?;
    std::fs::write(path, json)?;
    Ok(default)
}

