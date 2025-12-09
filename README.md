# Homebrew Tool Assistant

A Rust-based TUI (Terminal User Interface) application that helps you discover and understand your Homebrew-installed tools through AI-powered recommendations using local Ollama.

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

4. **Ollama Model** (e.g., llama3.2)
   ```bash
   ollama pull llama3.2
   ```

## Installation

1. Clone or navigate to the project directory:
   ```bash
   cd /Users/shichaopeng/Work/self-dir/mac-bin-analyse
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

- **↑/↓ Arrow Keys**: Navigate through the package list
- **i**: Enter input mode to type your query
- **Enter**: Submit your query to the AI
- **Esc**: Exit input mode
- **q**: Quit the application

### Example Queries

Try asking questions like:

- "I need to process JSON files"
- "How can I convert images to different formats?"
- "Show me tools for monitoring network traffic"
- "I want to edit videos from the command line"
- "What tools can help me work with Docker?"

### Interface Layout

```
┌─────────────────────┬──────────────────────────────────────┐
│  📦 Packages        │  🔍 Query                            │
│                     │  (Type your question here)           │
│  - git              ├──────────────────────────────────────┤
│  - jq               │  🤖 AI Response                      │
│  - ffmpeg           │                                      │
│  - docker           │  The AI will recommend tools and     │
│  - ...              │  provide usage examples here         │
│                     │                                      │
└─────────────────────┴──────────────────────────────────────┘
```

## Configuration

### Changing the Ollama Model

By default, the application uses the `llama3.2` model. To use a different model:

1. Pull the desired model:
   ```bash
   ollama pull mistral
   ```

2. Edit `src/main.rs` and change the model name:
   ```rust
   let ollama = OllamaClient::new("mistral".to_string());
   ```

### Custom Ollama URL

If your Ollama instance is running on a different host/port, edit `src/ollama.rs`:

```rust
impl OllamaClient {
    pub fn new(model: String) -> Self {
        Self {
            client: Client::new(),
            base_url: "http://your-host:port".to_string(),  // Change this
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
     "model": "llama3.2",
     "prompt": "Hello",
     "stream": false
   }'
   ```

## Development

### Project Structure

```
src/
├── main.rs      # Application entry point and event loop
├── app.rs       # Application state management
├── brew.rs      # Homebrew package integration
├── ollama.rs    # Ollama API client
└── ui.rs        # TUI rendering with Ratatui
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

## License

MIT

## Contributing

Contributions are welcome! Please feel free to submit issues or pull requests.
