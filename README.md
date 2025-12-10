# Mac Aid: Homebrew Command Assistant

A Rust-based TUI application that helps you discover and understand your Homebrew-installed tools. It indexes local man/help pages into a vector store and uses a local Ollama model to recommend tools and usage examples based on your query.

## Features

- 📦 **Package Discovery**: Automatically lists all Homebrew-installed packages
- 🤖 **AI-Powered Recommendations**: Uses local Ollama to suggest tools based on your needs
- 💡 **Usage Examples**: Provides practical command-line examples for recommended tools
- ⌨️ **Interactive TUI**: Clean, intuitive terminal interface built with Ratatui

## Prerequisites

Before running this application, ensure you have:

1. **Rust** (1.70 or later)
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Homebrew** (macOS package manager)
   ```bash
   /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
   ```

3. **Ollama** (local LLM runtime)
   ```bash
   brew install ollama
   ```

4. **Ollama Models**
   ```bash
   ollama pull all-minilm
   ollama pull qwen3-coder:480b-cloud
   ```

## Installation

1. Clone the repository and enter the directory:
   ```bash
   git clone <repo_url>
   cd mac-aid
   ```

2. Build the project:
   ```bash
   cargo build --release
   ```

3. Run the application:
   ```bash
   cargo run --release
   ```

## Usage

### Controls

- **Enter**: Submit your query
- **Esc**: Clear input
- **q**: Quit
- **Ctrl + r**: Rebuild knowledge base
- **Shift + R**: Reload index data
- **↑/↓**: Scroll response

### Example Queries

Try asking questions like:

- "I need to process JSON files"
- "How can I convert images to different formats?"
- "Show me tools for monitoring network traffic"
- "I want to edit videos from the command line"
- "What tools can help me work with Docker?"

### Interface Layout

```
┌──────────────────────────────────────────────┐
│  Enter your need                           │
│ [type your query here]                       │
└──────────────────────────────────────────────┘
┌──────────────────────────────────────────────┐
│ Status: Ready / Indexing progress messages   │
└──────────────────────────────────────────────┘
┌──────────────────────────────────────────────┐
│ 💡 Recommendation                            │
│ AI suggestions and usage examples appear here│
└──────────────────────────────────────────────┘
```

## Configuration

### Changing the Ollama Model

The interactive query model is set in `src/main.rs` and defaults to `qwen3-coder:480b-cloud`:

```rust
let ollama = OllamaClient::new("qwen3-coder:480b-cloud".to_string());
```

Pull and set any model you prefer by updating this line.

### Custom Ollama URL

If your Ollama instance is running on a different host/port, edit `src/ollama.rs`:

```rust
impl OllamaClient {
    pub fn new(model: String) -> Self {
        Self {
            client: Client::new(),
            base_url: "http://your-host:port".to_string(),
            model,
        }
    }
}
```

## Troubleshooting

### "Failed to execute brew list command"

Make sure Homebrew is installed and accessible in your PATH:
```bash
which brew
brew --version
```

### "Ollama API request failed"

1. Ensure Ollama is running:
   ```bash
   ollama serve
   ```

2. Verify the model is available:
   ```bash
   ollama list
   ```

3. Test the API manually:
   ```bash
   curl http://localhost:11434/api/generate -d '{
     "model": "qwen3-coder:480b-cloud",
     "prompt": "Hello",
     "stream": false
   }'
  ```

## Development

### Project Structure

```
src/
├── main.rs
├── app.rs
├── ui.rs
├── brew.rs
├── indexer.rs
├── vector_store.rs
├── rag.rs
├── langchain_integration.rs
├── kb_builder.rs
└── log.rs
```

### Building for Development

```bash
cargo build
cargo run
```

### Running Tests

```bash
cargo test
```

### Data Location

- Database: `~/.mac-aid/commands.db`
- Logs: `~/.mac-aid/error.log`, `~/.mac-aid/info.log`

## License

MIT

## Contributing

Contributions are welcome! Please feel free to submit issues or pull requests.
