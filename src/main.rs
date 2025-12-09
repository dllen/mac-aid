mod app;
mod brew;
mod indexer;
mod ollama;
mod rag;
mod ui;
mod vector_store;

use anyhow::Result;
use app::{App, AppState};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ollama::OllamaClient;
use rag::RagPipeline;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::path::PathBuf;
use vector_store::VectorStore;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Load Homebrew packages
    let packages = brew::get_installed_packages()?;
    let mut app = App::new(packages.clone());

    // Initialize Ollama client
    let ollama = OllamaClient::new("llama3.2".to_string());

    // Initialize vector store
    let db_path = get_db_path()?;
    let vector_store = VectorStore::new(db_path)?;

    // Check if we need to index
    if vector_store.is_empty()? {
        app.set_response("Indexing man pages... This may take a few minutes.".to_string());
        terminal.draw(|f| ui::render(f, &app))?;
        // Index packages
        let package_names: Vec<String> = packages.iter().map(|p| p.name.clone()).collect();
        let docs = indexer::index_brew_packages(&package_names).await?;

        // Show progress and iterate docs
        let total = docs.len();
        app.set_progress(total);
        terminal.draw(|f| ui::render(f, &app))?;

        for (i, doc) in docs.into_iter().enumerate() {
            let cmd_name = doc.command_name.clone();
            match ollama.generate_embedding(&doc.man_content).await {
                Ok(embedding) => {
                    // store (synchronous)
                    if let Err(e) = vector_store.store_command(
                        &doc.package_name,
                        &cmd_name,
                        &doc.man_content,
                        &embedding,
                    ) {
                        eprintln!("Failed to store embedding for {}: {}", cmd_name, e);
                    }
                }
                Err(e) => {
                    eprintln!("Failed to generate embedding for {}: {}", cmd_name, e);
                }
            }

            // update progress and redraw UI
            app.update_progress(i + 1, Some(cmd_name));
            terminal.draw(|f| ui::render(f, &app))?;
        }

        let count = vector_store.count()?;
        app.clear_progress();
        app.set_response(format!("Indexing complete! Indexed {} commands.", count));
        terminal.draw(|f| ui::render(f, &app))?;
    }

    // Initialize RAG pipeline
    let rag = RagPipeline::new(vector_store, ollama);

    // Run the app
    let res = run_app(&mut terminal, &mut app, &rag).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Error: {:?}", err);
    }

    Ok(())
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    rag: &RagPipeline,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui::render(f, app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            match app.state {
                AppState::Normal => match key.code {
                    KeyCode::Char('q') => {
                        app.quit();
                    }
                    KeyCode::Char('i') => {
                        app.enter_input_mode();
                    }
                    KeyCode::Up => {
                        app.previous();
                    }
                    KeyCode::Down => {
                        app.next();
                    }
                    _ => {}
                },
                AppState::Input => match key.code {
                    KeyCode::Enter => {
                        let query = app.input.clone();
                        if !query.is_empty() {
                            app.set_loading();
                            terminal.draw(|f| ui::render(f, app))?;

                            let package_names: Vec<String> =
                                app.packages.iter().map(|p| p.name.clone()).collect();

                            // Use RAG to query with context
                            match rag.query_with_rag(&query, &package_names, 3).await {
                                Ok(response) => {
                                    app.set_response(response);
                                }
                                Err(e) => {
                                    app.set_response(format!("Error: {}", e));
                                }
                            }
                        } else {
                            app.exit_input_mode();
                        }
                    }
                    KeyCode::Char(c) => {
                        app.push_char(c);
                    }
                    KeyCode::Backspace => {
                        app.pop_char();
                    }
                    KeyCode::Esc => {
                        app.exit_input_mode();
                    }
                    _ => {}
                },
                AppState::Loading => {
                    // Ignore input while loading
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

fn get_db_path() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
    let app_dir = home.join(".mac-aid");
    std::fs::create_dir_all(&app_dir)?;
    Ok(app_dir.join("commands.db"))
}
